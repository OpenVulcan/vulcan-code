//! Local service bridge used by the Tauri WebUI.
//! Tauri WebUI 使用的本地服务桥接层。

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock},
};

use base64::Engine;
use base64::engine::general_purpose::{
    STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD,
};
use futures_util::StreamExt;
use http_range::HttpRange;
use serde_json::{Value, json};
use tauri::http::{
    Method, Response, StatusCode,
    header::{
        ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, CACHE_CONTROL,
        CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
    },
};
use tauri::{Emitter, Manager};
use time::OffsetDateTime;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport,
    ExecutionStreamEvent, RuntimePromptIntervention,
    agent_profile::{AgentProfileCatalog, load_agent_profile_catalog},
    agent_runtime::{
        context_injection_runtime::{
            context_injection_panel_cards_for_kind,
            remove_legacy_vmm_profile_context_injection_state,
        },
        conversation_hooks::{
            ConversationCompletionRequest, ConversationHookRuntime, ConversationPrecheckOutcome,
            ConversationTurnCompletionStatus, VmmWritebackHookEffect,
        },
        events::{RuntimeEvent, RuntimeEventKind},
        question_runtime::QuestionRuntime,
        task_runtime::{DetachedTaskRuntimeEvent, TaskRuntime},
        token_budget_runtime::TokenBudgetRuntime,
        vmm_hooks::{VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks},
    },
    attachment_assets::{
        attachment_descriptor_from_mime_type, attachment_descriptor_from_path,
        stage_attachment_bytes, stage_attachment_file,
    },
    conversation_protocol::{
        conversation_state_from_session_messages, conversation_state_to_session_messages,
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{McpRuntimeAccess, bootstrap_mcp_runtime_access},
    provider_capabilities::{ResolvedModelCapabilities, resolve_effective_model_capabilities},
    provider_config::{
        self, PersistedAiToolConfiguration, ProviderConfigurationFile, ProviderRuntimeKind,
        ReasoningEffortLevel,
    },
    provider_input_normalizer::normalize_provider_user_input_for_model,
    session_event_runtime::{
        SessionStreamingRecordIds, append_runtime_event_projection_to_state,
        append_stream_delta_record_to_state, apply_checkpoint_conversation_state,
        apply_terminal_execution_report_to_state, collapse_streaming_thinking_in_state,
    },
    stream_runtime_run_with_intervention,
    tooling::CommandSessionRuntime,
    turn_stats::build_session_turn_stats_from_state,
    workbench_ai_capability_router::{
        WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS, effective_ai_tool_configuration_for_prompt_overrides,
        effective_workbench_disabled_ai_capability_kinds,
    },
    workbench_prompt_turn::{
        SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnContinueInput,
        SharedPromptTurnStart, begin_shared_prompt_turn,
        continue_shared_prompt_turn_after_precheck, shared_prompt_precheck_plan,
    },
};
use vulcan_context::{
    ContextBudgetSeverity, ContextBudgetSnapshot, SessionConversationRecord, append_record,
};
use vulcan_context_injection::{InjectionKind, InjectionPanelCard};
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_service_api::{
    AiCapabilitySummary, AiCapabilityTargetSummary, CommandSessionSummary,
    ComposerAttachmentCapabilitySummary, ComposerAttachmentSummary, ContextInjectionCardSummary,
    ContextInjectionItemSummary, ContextInjectionSnapshot, ContextUsageSnapshot,
    ConversationMessage, ConversationSummary, FeatureAreaSummary, LogEntrySummary, MemorySummary,
    MessageRole, PendingPromptSummary, PluginSummary, ProjectSummary, ProviderModelSummary,
    ProviderSummary, ReasoningSelectionOptionSummary, ReasoningSelectionSummary,
    RuntimeModuleSnapshot, ServiceEvent, SettingSectionSummary, StartupSnapshot,
    StartupStepSnapshot, StartupStepStatus, SubagentNavigationSnapshot, SubagentTaskSummary,
    ToolCallSummary, WorkbenchSnapshot,
};
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionLifecycleBeginRequest, SessionLifecycleFinishRequest,
    SessionLifecycleHook, SessionLifecycleRunner, SessionTurnTerminalStatus,
};
use vulcan_session_core::{
    ActiveTurnKind, SessionNodeId, SessionRuntimeKernel, TurnInput, TurnInputKind,
    TurnTerminalStatus,
};
use vulcan_task_session::SegmentedTaskSessionStore;
use vulcan_workbench_model::{
    PENDING_PROMPT_QUEUE_LIMIT, PendingPromptBusyLaneContext, PendingPromptDispatchContext,
    PendingPromptModelReference, PendingPromptPanelDropTarget, PendingPromptPanelEntry,
    PendingPromptPanelEntryState, PendingPromptQueueRouteError, PendingPromptQueueRouteOutcome,
    PendingPromptRuntimeOptions, PendingSessionPromptEntry, PendingSessionPromptSource,
    SUBAGENT_TASK_STOPPED_SUMMARY_TEXT, SelectedSubagentTaskPanel, SessionId, SessionMessageEntry,
    SessionMessageKind, SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry,
    SessionState, SessionSummary, SessionTitleSource, SessionTurnStatsStatus,
    SubagentTaskPanelState, SubagentTaskRunState, TaskProjectSummary,
    apply_pending_prompt_panel_drop_target, attach_tool_display_state_to_metadata,
    build_session_record_message_id, build_stopped_task_session_metadata,
    clear_internal_pending_prompt_queue_and_restore_panel,
    collect_running_subagent_task_states_for_tree, find_task_session_tool_message_id_for_payload,
    merge_subagent_task_panel_cache, merge_tool_display_metadata,
    next_pending_prompt_dispatch_source, normalize_subagent_task_panel_state,
    parse_subagent_task_panel_metadata, parse_subagent_task_prompt_metadata,
    pending_prompt_dispatch_block_reason_for_source, pending_prompt_lane_is_busy,
    remove_internal_pending_prompt_entries_by_source, remove_pending_prompt_panel_entry,
    route_pending_prompt_panel_entry_into_internal_queue, select_subagent_navigation_panel_states,
    subagent_navigation_round_should_remain_visible, subagent_navigation_surface_is_visible,
    subagent_navigation_task_label, subagent_task_panel_is_openable,
    subagent_task_panel_state_detail_from_segmented_task_session_store,
    subagent_task_panel_state_has_recoverable_conversation, take_next_pending_prompt_entry,
    take_runtime_intervention_guided_pending_prompt_entries,
    task_session_execution_status_from_tool_metadata, task_session_message_match_score,
    task_session_payload_from_tool_metadata,
};
use vulcan_workspace_store::{
    data_plane::{APP_SPACE_ID, ControllerRuntimeHandle},
    workspace_store::{
        SessionHydrationSnapshot, SessionTaskMirrorSnapshot,
        WorkspaceSegmentedSessionBoundaryStore, WorkspaceSnapshot,
    },
};

use crate::luaskills_bridge::{TauriLuaSkillsCatalog, TauriLuaSkillsHandle};
use crate::mcp_bridge::{TauriMcpRuntimeCatalog, TauriMcpRuntimeHandle};
use crate::token_budget_bridge::TauriTokenBudgetRuntime;

/// Shared runtime handles used to execute real conversation turns.
/// 用于执行真实会话轮次的共享运行时句柄集合。
#[derive(Clone)]
struct AgentRuntimeBridge {
    /// Provider configuration captured during service startup.
    /// 服务启动期间捕获的 Provider 配置。
    provider_configuration: ProviderConfigurationFile,
    /// Standalone AI capability configuration captured during service startup.
    /// 服务启动期间捕获的独立 AI 能力配置。
    ai_tool_configuration: PersistedAiToolConfiguration,
    /// Agent profile catalog reused by runtime preparation.
    /// 运行时准备阶段复用的 Agent 画像目录。
    agent_profile_catalog: AgentProfileCatalog,
    /// LuaSkills runtime handle reused by provider-visible tool execution.
    /// Provider 可见工具执行复用的 LuaSkills 运行时句柄。
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// MCP runtime access reused by provider-visible MCP tools.
    /// Provider 可见 MCP 工具复用的 MCP 运行时访问面。
    mcp_runtime: Option<McpRuntimeAccess>,
    /// Shared task runtime used by nested subagent tools.
    /// 嵌套子代理工具使用的共享任务运行时。
    task_runtime: Arc<TaskRuntime>,
    /// Shared ask-question runtime used by the ask tool.
    /// ask 工具使用的共享提问运行时。
    question_runtime: Arc<QuestionRuntime>,
    /// Shared command-session runtime used by shell session tools.
    /// shell 会话工具使用的共享命令会话运行时。
    command_session_runtime: Arc<CommandSessionRuntime>,
    /// Shared token-budget runtime used by context assembly.
    /// 上下文装配使用的共享 Token 预算运行时。
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    /// Shared conversation hook runtime used to mirror iced turn assembly semantics.
    /// 用于复刻 iced 回合组装语义的共享会话 hook 运行时。
    conversation_hook_runtime: Arc<Mutex<ConversationHookRuntime>>,
    /// Shared VMM hook runtime snapshot used by built-in memory hooks.
    /// 内置记忆 hook 使用的共享 VMM hook 运行时快照。
    vmm_hook_runtime: VmmHookRuntimeHandle,
}

/// No-op session lifecycle hook used by the WebUI external stream lifecycle runner.
/// WebUI 外部流生命周期 runner 使用的空会话生命周期 hook。
#[derive(Debug, Clone, Copy)]
struct WebuiSessionLifecycleHook;

impl SessionLifecycleHook for WebuiSessionLifecycleHook {
    /// Ignores session lifecycle events after durable append.
    /// 在持久化追加后忽略 会话生命周期事件。
    fn after_event_appended(&self, _event: &SessionBoundaryEvent) -> Result<(), String> {
        Ok(())
    }
}

/// Number of recent turns loaded for the selected conversation.
/// 为当前选中会话加载的最近轮次数量。
const SELECTED_SESSION_TURN_LOAD_LIMIT: usize = 24;

/// Maximum loaded subagent transcript surfaces retained by the Tauri service.
/// Tauri 服务保留的已加载子代理转录界面最大数量。
const SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT: usize = 128;

/// Milliseconds before an unused loaded subagent transcript surface can be evicted.
/// 已加载但未使用的子代理转录界面允许被清理前的毫秒数。
const SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS: i128 = 30 * 60 * 1000;

/// Maximum media bytes returned for one range response.
/// 单次范围响应返回的最大媒体字节数。
const MEDIA_ASSET_RANGE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Route prefix used by the generated media custom protocol.
/// 生成媒体自定义协议使用的路由前缀。
const MEDIA_ASSET_ROUTE_PREFIX: &str = "/asset/";

/// Parent-turn binding captured when a detached WebUI subagent starts.
/// WebUI 分离子代理启动时捕获的父回合绑定。
#[derive(Debug, Clone)]
struct WebuiSubagentParentTurnBinding {
    /// Parent session that spawned the detached child task.
    /// 派生分离子任务的父会话。
    parent_session_id: SessionId,
    /// Parent kernel generation that was current at child creation time.
    /// 子任务创建时父级内核的当前代际。
    source_generation: u64,
    /// Parent turn id that was active at child creation time.
    /// 子任务创建时处于活动状态的父级回合标识。
    parent_turn_id: Option<String>,
}

/// Long-lived in-memory detail cache for one loaded WebUI subagent surface.
/// 单个已加载 WebUI 子代理界面的长生命周期内存详情缓存。
#[derive(Debug, Clone)]
struct WebuiSubagentSurfaceDetailCacheEntry {
    /// Parent conversation that owns the child task.
    /// 拥有该子任务的父会话。
    parent_session_id: SessionId,
    /// Cached task panel state including durable child conversation detail when available.
    /// 缓存的任务面板状态，包含可用的持久子会话详情。
    task_state: SubagentTaskPanelState,
    /// Cached projected child transcript messages for direct WebUI surface rendering.
    /// 用于 WebUI 界面直接渲染的子转录消息缓存。
    messages: Vec<SessionMessageEntry>,
    /// Last access timestamp in Unix milliseconds.
    /// 最近访问时间戳，单位为 Unix 毫秒。
    last_access_ms: i128,
}

/// Minimal retained state needed to repair one refreshed session row.
/// 修复单个刷新后会话行所需的最小保留状态。
#[derive(Debug, Clone)]
struct PreservedSessionRunState {
    /// Session index in the retained sidebar list.
    /// 会话在保留侧栏列表中的位置。
    previous_index: usize,
    /// Project session index in the retained project tree.
    /// 会话在保留项目树中的位置。
    previous_project_index: Option<usize>,
    /// Session summary retained from memory.
    /// 从内存保留的会话摘要。
    session: SessionSummary,
    /// Session run phase retained from memory.
    /// 从内存保留的会话运行阶段。
    previous_phase: Option<SessionRunPhase>,
    /// Whether the runtime kernel still owns an active turn for this session.
    /// 运行时内核是否仍持有该会话的活动回合。
    had_active_run: bool,
    /// Runtime state retained from memory when it contains live context.
    /// 包含实时上下文时从内存保留的运行态。
    runtime_state: Option<SessionRuntimeStateEntry>,
    /// Visible messages retained from memory for hydrated or not-yet-persisted sessions.
    /// 为已水合或尚未持久化的会话从内存保留的可见消息。
    messages: Option<Vec<SessionMessageEntry>>,
}

/// Minimal retained workspace state used after a database-backed refresh.
/// 数据库刷新后使用的最小工作区保留状态。
#[derive(Debug, Clone)]
struct PreservedWorkspaceRunState {
    /// Selected session retained from memory when it still exists after refresh.
    /// 刷新后仍存在时从内存保留的选中会话。
    selected_session: Option<SessionId>,
    /// Session-scoped state rows that must be restored after refresh.
    /// 刷新后必须恢复的会话级状态行。
    sessions: Vec<PreservedSessionRunState>,
}

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
struct ServiceBridgeState {
    /// Latest startup snapshot.
    /// 最近一次启动快照。
    startup: StartupSnapshot,
    /// Latest workbench snapshot.
    /// 最近一次工作台快照。
    workbench: WorkbenchSnapshot,
    /// Monotonic version for workspace-driven workbench rebuilds.
    /// 工作区驱动的工作台重建单调版本号。
    workbench_generation: u64,
    /// Latest workspace snapshot retained for streaming turn updates.
    /// 为流式会话轮次更新保留的最近工作区快照。
    workspace_snapshot: Option<WorkspaceSnapshot>,
    /// Long-lived shared agent runtime bridge.
    /// 长生命周期共享 Agent 运行时桥接。
    agent_runtime: Option<AgentRuntimeBridge>,
    /// Tokio runtime that owns bootstrap-spawned background tasks such as shared MCP access.
    /// 持有引导期后台任务的 Tokio 运行时，例如共享 MCP 访问任务。
    service_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Long-lived controller runtime handle.
    /// 长生命周期控制器运行时句柄。
    controller_runtime: Option<ControllerRuntimeHandle>,
    /// Long-lived LuaSkills runtime handle.
    /// 长生命周期 LuaSkills 运行时句柄。
    luaskills_runtime: Option<TauriLuaSkillsHandle>,
    /// Cached LuaSkills catalog captured during startup or explicit refresh.
    /// 启动或显式刷新时捕获的 LuaSkills 目录缓存。
    cached_luaskills_catalog: Option<TauriLuaSkillsCatalog>,
    /// Long-lived token budget runtime handle.
    /// 长生命周期 Token 预算运行时句柄。
    token_budget_runtime: Option<Arc<TauriTokenBudgetRuntime>>,
    /// Long-lived MCP runtime handle.
    /// 长生命周期 MCP 运行时句柄。
    mcp_runtime: Option<TauriMcpRuntimeHandle>,
    /// Cached MCP catalog captured during startup or explicit refresh.
    /// 启动或显式刷新时捕获的 MCP 目录缓存。
    cached_mcp_catalog: Option<TauriMcpRuntimeCatalog>,
    /// Latest context-budget snapshots keyed by session id.
    /// 按会话标识索引的最近上下文预算快照。
    live_context_budgets: BTreeMap<SessionId, ContextBudgetSnapshot>,
    /// Cached subagent task panels keyed by parent session and child task id.
    /// 按父会话与子任务标识索引的子代理任务面板缓存。
    session_subagent_tasks: BTreeMap<SessionId, BTreeMap<SessionId, SubagentTaskPanelState>>,
    /// Long-lived loaded child transcript details keyed by child task id.
    /// 按子任务标识索引的长生命周期已加载子转录详情。
    subagent_surface_detail_cache: BTreeMap<SessionId, WebuiSubagentSurfaceDetailCacheEntry>,
    /// Parent sessions whose current coordination round should keep subagent navigation visible.
    /// 当前协调轮次中应保持子代理导航可见的父会话集合。
    session_subagent_navigation_rounds: BTreeSet<SessionId>,
    /// Currently opened subagent surface selection.
    /// 当前打开的子代理界面选择。
    active_subagent_panel: Option<SelectedSubagentTaskPanel>,
    /// Detached child-task listeners already started by the WebUI bridge.
    /// WebUI 桥接层已经启动的分离子任务监听器集合。
    detached_subagent_task_listeners: BTreeSet<SessionId>,
    /// Parent-turn bindings keyed by detached child task id.
    /// 按分离子任务标识索引的父回合绑定。
    subagent_parent_turn_bindings: BTreeMap<SessionId, WebuiSubagentParentTurnBinding>,
    /// Parent sessions currently blocked inside a model-requested wait_agent call.
    /// 当前正阻塞在模型请求的 wait_agent 调用中的父会话集合。
    parent_sessions_waiting_for_subagents: BTreeSet<SessionId>,
    /// Child-agent selectors already collected by the latest wait_agent result per parent session.
    /// 每个父会话最近一次 wait_agent 结果已经收集的子代理选择器。
    parent_session_completed_wait_agent_selectors: BTreeMap<SessionId, BTreeSet<String>>,
    /// Active WebUI stream task handles keyed by session id.
    /// 按会话标识索引的 WebUI 活动流任务句柄。
    active_stream_tasks: BTreeMap<SessionId, tauri::async_runtime::JoinHandle<()>>,
    /// Session runtime kernels keyed by root WebUI session id.
    /// 按 WebUI 根会话标识索引的会话运行时内核。
    session_runtime_kernels: BTreeMap<SessionId, SessionRuntimeKernel>,
    /// Latest background persistence version keyed by session id.
    /// 按会话标识索引的最新后台持久化版本。
    session_persist_versions: BTreeMap<SessionId, u64>,
}

/// Thread-safe service state handle.
/// 线程安全的服务状态句柄。
type SharedServiceBridgeState = Arc<Mutex<ServiceBridgeState>>;

/// Shared process-wide gate for WebUI workspace persistence.
/// WebUI 工作区持久化使用的进程级共享闸门。
static WEBUI_WORKSPACE_PERSIST_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

/// Returns the shared async persistence gate used by every WebUI workspace write.
/// 返回所有 WebUI 工作区写入共用的异步持久化闸门。
///
/// # Returns
/// 返回值
/// A shared mutex that serializes SQLite-backed workspace writes.
/// 用于串行化 SQLite 工作区写入的共享互斥锁。
fn webui_workspace_persist_lock() -> Arc<tokio::sync::Mutex<()>> {
    WEBUI_WORKSPACE_PERSIST_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Converts the local millisecond clock into the session-core timestamp domain.
/// 将本地毫秒时钟转换到 session-core 使用的时间戳域。
///
/// # Returns
/// 返回值
/// Current Unix milliseconds as an unsigned value.
/// 当前 Unix 毫秒无符号值。
fn current_unix_millis_u64() -> u64 {
    u64::try_from(current_unix_millis_value()).unwrap_or_default()
}

/// Returns the runtime kernel for one WebUI session, creating it on first use.
/// 返回单个 WebUI 会话的运行时内核，并在首次使用时创建。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the root WebUI session.
/// `session_id` 标识 WebUI 根会话。
///
/// # Returns
/// 返回值
/// Mutable access to the session runtime kernel.
/// 对会话运行时内核的可变访问。
fn session_runtime_kernel_mut(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) -> &mut SessionRuntimeKernel {
    state
        .session_runtime_kernels
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeKernel::new(SessionNodeId::from(session_id)))
}

/// Captures the parent session turn binding for a detached WebUI subagent task.
/// 为一个 WebUI 分离子代理任务捕获父会话回合绑定。
///
/// # Parameters
/// 参数
/// `state` owns runtime kernels and subagent bindings.
/// `state` 持有运行时内核与子代理绑定。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` identifies the detached child task.
/// `task_id` 标识分离子任务。
fn bind_webui_subagent_parent_turn(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
) {
    let (source_generation, parent_turn_id) = {
        let kernel = session_runtime_kernel_mut(state, parent_session_id);
        (
            kernel.generation(),
            kernel.active_turn().map(|turn| turn.turn_id.clone()),
        )
    };
    state
        .subagent_parent_turn_bindings
        .entry(task_id)
        .or_insert_with(|| WebuiSubagentParentTurnBinding {
            parent_session_id,
            source_generation,
            parent_turn_id: parent_turn_id.clone(),
        });
    webui_trace(
        "subagent_parent_turn_bound",
        format!(
            "parent={parent_session_id} task={task_id} generation={source_generation} parent_turn_id={}",
            parent_turn_id.as_deref().unwrap_or("<none>")
        ),
    );
}

/// Clears the captured parent-turn binding for a detached WebUI subagent task.
/// 清理一个 WebUI 分离子代理任务捕获的父回合绑定。
///
/// # Parameters
/// 参数
/// `state` owns subagent parent-turn bindings.
/// `state` 持有子代理父回合绑定。
/// `task_id` identifies the detached child task.
/// `task_id` 标识分离子任务。
fn clear_webui_subagent_parent_turn_binding(state: &mut ServiceBridgeState, task_id: SessionId) {
    if let Some(binding) = state.subagent_parent_turn_bindings.remove(&task_id) {
        webui_trace(
            "subagent_parent_turn_binding_cleared",
            format!(
                "parent={} task={task_id} generation={}",
                binding.parent_session_id, binding.source_generation
            ),
        );
    }
}

/// Starts one WebUI user-prompt turn in the session runtime kernel.
/// 在会话运行时内核中启动一个 WebUI 用户提示词回合。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
/// `turn_id` is the shared turn id produced by the prompt reducer.
/// `turn_id` 是提示词归约器生成的共享回合标识。
///
/// # Returns
/// 返回值
/// `Ok(())` when the turn is accepted; otherwise a single-active-turn error.
/// 回合被接受时返回 `Ok(())`，否则返回单活动回合错误。
fn start_webui_session_kernel_user_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
) -> Result<(), String> {
    start_webui_session_kernel_turn(state, session_id, turn_id, ActiveTurnKind::UserPrompt)
}

/// Starts one WebUI turn in the session runtime kernel.
/// 在会话运行时内核中启动一个 WebUI 回合。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
/// `turn_id` is the shared turn id produced by the prompt reducer.
/// `turn_id` 是提示词归约器生成的共享回合标识。
/// `kind` is the runtime kernel turn kind.
/// `kind` 是运行时内核回合类型。
///
/// # Returns
/// 返回值
/// `Ok(())` when the turn is accepted; otherwise a single-active-turn error.
/// 回合被接受时返回 `Ok(())`，否则返回单活动回合错误。
fn start_webui_session_kernel_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    turn_id: &str,
    kind: ActiveTurnKind,
) -> Result<(), String> {
    if kind == ActiveTurnKind::UserPrompt {
        clear_webui_session_wait_agent_coordination(state, session_id, "new_user_turn");
    }
    let kernel = session_runtime_kernel_mut(state, session_id);
    let turn = kernel
        .start_turn(turn_id.to_string(), kind, current_unix_millis_u64())
        .map_err(|error| {
            format!(
                "会话 `{session_id}` 无法启动运行内核回合：{}",
                error.message
            )
        })?;
    webui_trace(
        "session_kernel_turn_started",
        format!(
            "session={session_id} turn_id={} generation={}",
            turn.turn_id, turn.generation
        ),
    );
    Ok(())
}

/// Finishes one WebUI session kernel turn if it is still active.
/// 如果 WebUI 会话内核回合仍处于活动状态则结束它。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
/// `status` is the terminal status to record for diagnostics.
/// `status` 是用于诊断记录的终态状态。
fn finish_webui_session_kernel_turn(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    status: TurnTerminalStatus,
) {
    let Some(kernel) = state.session_runtime_kernels.get_mut(&session_id) else {
        return;
    };
    let finished = kernel.finish_turn(status, current_unix_millis_u64());
    if let Some(turn) = finished {
        webui_trace(
            "session_kernel_turn_finished",
            format!(
                "session={session_id} turn_id={} generation={} status={status:?}",
                turn.turn_id, turn.generation
            ),
        );
    }
}

/// Stops the current WebUI session kernel generation if one exists.
/// 如果当前 WebUI 会话内核代际存在则停止它。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state that owns runtime kernels.
/// `state` 是持有运行时内核的可变服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` when an active kernel turn was stopped.
/// 停止了活动内核回合时返回 `true`。
fn stop_webui_session_kernel_generation(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    let Some(kernel) = state.session_runtime_kernels.get_mut(&session_id) else {
        return false;
    };
    let stopped = kernel.stop_current_generation(current_unix_millis_u64());
    if let Some(turn) = stopped {
        webui_trace(
            "session_kernel_generation_stopped",
            format!(
                "session={session_id} turn_id={} generation={}",
                turn.turn_id, turn.generation
            ),
        );
        clear_webui_session_wait_agent_coordination(state, session_id, "stop_generation");
        true
    } else {
        clear_webui_session_wait_agent_coordination(state, session_id, "stop_generation_idle");
        false
    }
}

/// Clears wait-agent coordination state that must not survive into a new parent turn.
/// 清理不应跨入新父级回合的 wait_agent 协调状态。
fn clear_webui_session_wait_agent_coordination(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    reason: &str,
) {
    let removed_waiting = state
        .parent_sessions_waiting_for_subagents
        .remove(&session_id);
    let removed_completed = state
        .parent_session_completed_wait_agent_selectors
        .remove(&session_id)
        .is_some();
    let removed_wait_scopes = state
        .session_runtime_kernels
        .get_mut(&session_id)
        .map(SessionRuntimeKernel::clear_wait_scopes)
        .unwrap_or(0);
    if removed_waiting || removed_completed || removed_wait_scopes > 0 {
        webui_trace(
            "wait_agent_coordination_cleared",
            format!(
                "session={session_id} reason={reason} removed_waiting={removed_waiting} removed_completed={removed_completed} removed_wait_scopes={removed_wait_scopes}"
            ),
        );
    }
}

/// Returns whether a WebUI session has an active kernel turn.
/// 返回 WebUI 会话是否存在活动的内核回合。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` when the session kernel currently owns an active turn.
/// 当会话内核当前拥有活动回合时返回 `true`。
fn webui_session_kernel_has_active_turn(state: &ServiceBridgeState, session_id: SessionId) -> bool {
    state
        .session_runtime_kernels
        .get(&session_id)
        .is_some_and(SessionRuntimeKernel::has_active_turn)
}

/// Returns whether a WebUI session should be treated as actively running.
/// 返回 WebUI 会话是否应被视为正在运行。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns stream handles and runtime kernels.
/// `state` 是持有流任务句柄与运行时内核的服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// `true` when the kernel has an active turn.
/// 当内核存在活动回合时返回 `true`。
fn webui_session_has_active_run(state: &ServiceBridgeState, session_id: SessionId) -> bool {
    state
        .session_runtime_kernels
        .get(&session_id)
        .is_some_and(SessionRuntimeKernel::has_active_turn)
}

/// Reads the current kernel generation for one WebUI session.
/// 读取单个 WebUI 会话当前的内核代际。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
/// `session_id` identifies the WebUI session.
/// `session_id` 标识 WebUI 会话。
///
/// # Returns
/// 返回值
/// The current generation when a kernel exists.
/// 当内核存在时返回当前代际。
fn webui_session_kernel_generation(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> Option<u64> {
    state
        .session_runtime_kernels
        .get(&session_id)
        .map(SessionRuntimeKernel::generation)
}

/// Returns the session ids whose runtime kernels currently own active turns.
/// 返回当前运行时内核拥有活动回合的会话标识集合。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
///
/// # Returns
/// 返回值
/// Session ids with active kernel turns.
/// 存在活动内核回合的会话标识。
fn active_kernel_session_ids(state: &ServiceBridgeState) -> BTreeSet<SessionId> {
    state
        .session_runtime_kernels
        .iter()
        .filter_map(|(session_id, kernel)| kernel.has_active_turn().then_some(*session_id))
        .collect()
}

/// Returns the owning session id for one execution stream event.
/// 返回单个执行流事件所属的会话标识。
///
/// # Parameters
/// 参数
/// `event` is the execution stream event being routed.
/// `event` 是正在路由的执行流事件。
///
/// # Returns
/// 返回值
/// The WebUI session id carried by the event.
/// 事件携带的 WebUI 会话标识。
fn execution_stream_event_session_id(event: &ExecutionStreamEvent) -> SessionId {
    match event {
        ExecutionStreamEvent::Prepared(preparation) => preparation.session_id,
        ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id }
        | ExecutionStreamEvent::RequestStarted { session_id }
        | ExecutionStreamEvent::ThinkingDelta { session_id, .. }
        | ExecutionStreamEvent::ThinkingCompleted { session_id }
        | ExecutionStreamEvent::OutputDelta { session_id, .. }
        | ExecutionStreamEvent::RuntimeEvent { session_id, .. }
        | ExecutionStreamEvent::ConversationCheckpoint { session_id, .. } => *session_id,
        ExecutionStreamEvent::Finished(report) => report.session_id,
    }
}

/// Returns whether one stream event still belongs to the current kernel generation.
/// 返回单个流事件是否仍属于当前内核代际。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns runtime kernels.
/// `state` 是持有运行时内核的共享服务状态。
/// `event` is the stream event about to be applied.
/// `event` 是即将应用的流事件。
/// `expected_generation` is the generation captured when the stream task started.
/// `expected_generation` 是流任务启动时捕获的代际。
///
/// # Returns
/// 返回值
/// `true` when the event is still current, or no kernel generation was captured.
/// 当事件仍属于当前代际或没有捕获内核代际时返回 `true`。
fn webui_stream_event_matches_kernel_generation(
    state: &SharedServiceBridgeState,
    event: &ExecutionStreamEvent,
    expected_generation: Option<u64>,
) -> bool {
    let Some(expected_generation) = expected_generation else {
        return true;
    };
    let session_id = execution_stream_event_session_id(event);
    let Ok(guard) = state.lock() else {
        return false;
    };
    let Some(kernel) = guard.session_runtime_kernels.get(&session_id) else {
        return true;
    };
    kernel.generation() == expected_generation && !kernel.is_stopped()
}

/// Enqueues one subagent completion event into the parent session kernel mailbox.
/// 将一条子代理完成事件加入父会话内核 mailbox。
///
/// # Parameters
/// 参数
/// `state` is the service state that owns runtime kernels.
/// `state` 是持有运行时内核的服务状态。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` identifies the completed child task session.
/// `task_id` 标识已完成的子任务会话。
/// `metadata` carries the structured child-agent event metadata.
/// `metadata` 携带结构化子代理事件元数据。
///
/// # Returns
/// 返回值
/// `true` when the mailbox accepted the event.
/// mailbox 接受事件时返回 `true`。
fn enqueue_webui_subagent_mailbox_input(
    state: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    metadata: &Value,
) -> bool {
    let timestamp_ms = current_unix_millis_u64();
    let binding = state.subagent_parent_turn_bindings.get(&task_id).cloned();
    if let Some(binding) = binding.as_ref()
        && binding.parent_session_id != parent_session_id
    {
        webui_trace(
            "subagent_mailbox_input_rejected_parent_mismatch",
            format!(
                "parent={parent_session_id} task={task_id} binding_parent={}",
                binding.parent_session_id
            ),
        );
        return false;
    }
    let kernel = session_runtime_kernel_mut(state, parent_session_id);
    let source_generation = binding
        .as_ref()
        .map(|binding| binding.source_generation)
        .unwrap_or_else(|| kernel.generation());
    let parent_turn_id = binding
        .and_then(|binding| binding.parent_turn_id)
        .or_else(|| kernel.active_turn().map(|turn| turn.turn_id.clone()));
    let mailbox_event = metadata
        .get("payload")
        .and_then(|payload| payload.get("subagent_mailbox_event"))
        .cloned()
        .unwrap_or_else(|| metadata.clone());
    let parent_turn_id_for_payload = parent_turn_id.clone();
    let mut input = TurnInput::new(
        format!("subagent-mailbox-{parent_session_id}-{task_id}"),
        SessionNodeId::from(parent_session_id),
        TurnInputKind::SubagentMessage,
        true,
        timestamp_ms,
        json!({
            "task_id": task_id.to_string(),
            "parent_session_id": parent_session_id.to_string(),
            "source_generation": source_generation,
            "parent_turn_id": parent_turn_id_for_payload,
            "mailbox_event": mailbox_event,
        }),
    );
    if let Some(parent_turn_id) = parent_turn_id.clone() {
        input = input.with_parent_turn_id(parent_turn_id);
    }
    let accepted = kernel.enqueue_mailbox_input_for_generation(input, source_generation);
    webui_trace(
        "subagent_mailbox_input_enqueued",
        format!(
            "parent={parent_session_id} task={task_id} accepted={accepted} generation={source_generation}"
        ),
    );
    accepted
}

/// Runs the Tauri application and registers local service commands.
/// 运行 Tauri 应用并注册本地服务命令。
pub fn run() {
    let state = Arc::new(Mutex::new(ServiceBridgeState {
        startup: startup_snapshot_with_phase("pending", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        agent_runtime: None,
        service_runtime: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        parent_session_completed_wait_agent_selectors: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
    }));

    tauri::Builder::default()
        .register_uri_scheme_protocol("vulcan-media", |_context, request| {
            vulcan_media_protocol_response(request)
        })
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            service_health,
            service_startup_snapshot,
            service_workbench_snapshot,
            service_session_messages,
            service_pick_composer_attachments,
            service_stage_composer_attachment_paths,
            service_stage_composer_attachment_bytes,
            service_submit_conversation_prompt,
            service_stop_conversation_run,
            service_open_subagent_panel,
            service_close_subagent_panel,
            service_stop_subagent_task,
            service_insert_pending_prompt,
            service_edit_pending_prompt,
            service_remove_pending_prompt,
            service_reorder_pending_prompts,
            service_select_workbench_model,
            service_select_workbench_reasoning_effort,
            service_open_project_directory,
            service_open_project_directory_path,
            service_add_project_from_folder,
            service_toggle_project_pinned,
            service_reorder_projects,
            service_rename_project,
            service_set_project_icon,
            service_reset_project_icon,
            service_set_expanded_task_projects,
            service_archive_project_conversations,
            service_rename_conversation,
            service_archive_conversation,
            service_mark_conversation_read,
            service_select_conversation,
            service_open_external_url
        ])
        .setup(|app| {
            emit_initial_service_events(app.handle());
            spawn_service_bootstrap(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Vulcan Code Tauri shell");
}

/// Returns a simple service health marker for the WebUI.
/// 为 WebUI 返回简单的服务健康标记。
#[tauri::command]
fn service_health() -> String {
    "ready".to_string()
}

/// Opens a trusted external URL with the system default browser.
/// 使用系统默认浏览器打开可信外部链接。
#[tauri::command]
fn service_open_external_url(url: String) -> Result<String, String> {
    if url != "https://github.com/OpenVulcan/vulcan-code" {
        return Err(String::from("不允许打开未授权的外部链接。"));
    }
    open_url_in_system_browser(&url)?;
    Ok(String::from("已请求使用系统默认浏览器打开 GitHub。"))
}

/// Serves generated and local media files through a local custom protocol with HTTP range support.
/// 通过带 HTTP Range 支持的本地自定义协议提供生成媒体文件和本地媒体文件。
fn vulcan_media_protocol_response(request: tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    match build_vulcan_media_protocol_response(request) {
        Ok(response) => response,
        Err(error) => vulcan_media_error_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

/// Builds one media protocol response for images, audio, video, and PDFs.
/// 为图片、音频、视频和 PDF 构建单个媒体协议响应。
fn build_vulcan_media_protocol_response(
    request: tauri::http::Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, String> {
    let media_path = media_path_from_protocol_request(&request)?;
    if !is_allowed_media_asset_path(&media_path) {
        return Ok(vulcan_media_error_response(
            StatusCode::FORBIDDEN,
            "媒体路径不是允许的媒体资源类型。",
        ));
    }

    let mut file = match File::open(&media_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(vulcan_media_error_response(
                StatusCode::NOT_FOUND,
                "媒体文件不存在。",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(vulcan_media_error_response(
                StatusCode::FORBIDDEN,
                "没有权限读取媒体文件。",
            ));
        }
        Err(error) => return Err(format!("无法打开媒体文件：{error}")),
    };
    let file_len = file
        .metadata()
        .map_err(|error| format!("无法读取媒体文件元数据：{error}"))?
        .len();
    let mime_type = infer_media_asset_mime_type(&media_path);
    let response = Response::builder()
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCESS_CONTROL_EXPOSE_HEADERS, "content-range")
        .header(ACCEPT_RANGES, "bytes")
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_TYPE, mime_type);

    if request.method() == Method::HEAD {
        return response
            .header(CONTENT_LENGTH, file_len)
            .body(Vec::new())
            .map_err(|error| format!("无法构建媒体 HEAD 响应：{error}"));
    }

    if let Some(range_header) = request
        .headers()
        .get("range")
        .and_then(|value| value.to_str().ok())
    {
        let range = single_media_http_range(range_header, file_len)?;
        let range_start = range.start;
        let range_end = range.start + range.length - 1;
        let mut body = Vec::with_capacity(range.length as usize);
        file.seek(SeekFrom::Start(range_start))
            .map_err(|error| format!("无法定位媒体范围：{error}"))?;
        file.take(range.length)
            .read_to_end(&mut body)
            .map_err(|error| format!("无法读取媒体范围：{error}"))?;
        return response
            .status(StatusCode::PARTIAL_CONTENT)
            .header(
                CONTENT_RANGE,
                format!("bytes {range_start}-{range_end}/{file_len}"),
            )
            .header(CONTENT_LENGTH, body.len())
            .body(body)
            .map_err(|error| format!("无法构建媒体范围响应：{error}"));
    }

    let mut body = Vec::with_capacity(file_len.min(MEDIA_ASSET_RANGE_MAX_BYTES) as usize);
    file.read_to_end(&mut body)
        .map_err(|error| format!("无法读取媒体文件：{error}"))?;
    response
        .header(CONTENT_LENGTH, file_len)
        .body(body)
        .map_err(|error| format!("无法构建媒体响应：{error}"))
}

/// Extracts the local media path encoded inside a custom protocol request.
/// 提取自定义协议请求中编码的本地媒体路径。
fn media_path_from_protocol_request(
    request: &tauri::http::Request<Vec<u8>>,
) -> Result<PathBuf, String> {
    let request_path = request.uri().path();
    let encoded_path = request_path
        .strip_prefix(MEDIA_ASSET_ROUTE_PREFIX)
        .ok_or_else(|| String::from("媒体路由格式无效。"))?;
    let path_bytes = BASE64_URL_SAFE_NO_PAD
        .decode(encoded_path)
        .map_err(|error| format!("媒体路径解码失败：{error}"))?;
    let path_text = String::from_utf8(path_bytes)
        .map_err(|error| format!("媒体路径不是有效 UTF-8：{error}"))?;
    Ok(PathBuf::from(path_text))
}

/// Returns whether a media path belongs to the generated attachment storage tree.
/// 返回媒体路径是否属于生成附件存储树。
fn is_allowed_generated_media_path(path: &Path) -> bool {
    let normalized_components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|text| text.to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    normalized_components
        .windows(3)
        .any(|window| window == [".vulcan", "code", "attachments"])
}

/// Returns whether a media path can be served by the local media protocol.
/// 返回媒体路径是否允许通过本地媒体协议提供。
fn is_allowed_media_asset_path(path: &Path) -> bool {
    if is_allowed_generated_media_path(path) {
        return true;
    }
    has_supported_media_asset_extension(path)
}

/// Returns whether a local path extension is supported by the media protocol.
/// 返回本地路径扩展名是否受媒体协议支持。
fn has_supported_media_asset_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png")
            | Some("jpg")
            | Some("jpeg")
            | Some("webp")
            | Some("gif")
            | Some("bmp")
            | Some("svg")
            | Some("mp4")
            | Some("webm")
            | Some("mov")
            | Some("mkv")
            | Some("mp3")
            | Some("wav")
            | Some("m4a")
            | Some("aac")
            | Some("flac")
            | Some("ogg")
            | Some("pdf")
    )
}

/// Parses one bounded HTTP range for media playback.
/// 解析一个有上限的媒体播放 HTTP 范围。
fn single_media_http_range(range_header: &str, file_len: u64) -> Result<HttpRange, String> {
    let mut ranges = HttpRange::parse(range_header, file_len)
        .map_err(|error| format!("媒体 Range 请求无效：{error:?}"))?;
    let Some(mut range) = ranges.drain(..).next() else {
        return Err(String::from("媒体 Range 请求为空。"));
    };
    if range.start >= file_len || range.length == 0 {
        return Err(String::from("媒体 Range 请求越界。"));
    }
    range.length = range.length.min(MEDIA_ASSET_RANGE_MAX_BYTES);
    Ok(range)
}

/// Builds one plain text error response for the generated media protocol.
/// 为生成媒体协议构建一个纯文本错误响应。
fn vulcan_media_error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Infers a conservative MIME type from a generated media path extension.
/// 根据生成媒体路径扩展名推断保守 MIME 类型。
fn infer_media_asset_mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => String::from("image/png"),
        Some("jpg") | Some("jpeg") => String::from("image/jpeg"),
        Some("webp") => String::from("image/webp"),
        Some("gif") => String::from("image/gif"),
        Some("bmp") => String::from("image/bmp"),
        Some("svg") => String::from("image/svg+xml"),
        Some("mp4") => String::from("video/mp4"),
        Some("webm") => String::from("video/webm"),
        Some("mov") => String::from("video/quicktime"),
        Some("mkv") => String::from("video/x-matroska"),
        Some("mp3") => String::from("audio/mpeg"),
        Some("wav") => String::from("audio/wav"),
        Some("m4a") => String::from("audio/mp4"),
        Some("aac") => String::from("audio/aac"),
        Some("flac") => String::from("audio/flac"),
        Some("ogg") => String::from("audio/ogg"),
        Some("pdf") => String::from("application/pdf"),
        _ => String::from("application/octet-stream"),
    }
}

/// Returns the current startup snapshot.
/// 返回当前启动快照。
#[tauri::command]
fn service_startup_snapshot(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<StartupSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.startup.clone())
        .map_err(|error| format!("failed to lock service startup state: {error}"))
}

/// Returns the current workbench snapshot.
/// 返回当前工作台快照。
#[tauri::command]
fn service_workbench_snapshot(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.workbench.clone())
        .map_err(|error| format!("failed to lock service workbench state: {error}"))
}

/// Loads the recent transcript window for one conversation.
/// 加载单个会话的最近转录窗口。
#[tauri::command]
async fn service_session_messages(
    state: tauri::State<'_, SharedServiceBridgeState>,
    session_id: String,
) -> Result<Vec<ConversationMessage>, String> {
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
    let (controller_runtime, agent_runtime, live_listener_task_ids, in_memory_messages) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let in_memory_messages =
            in_memory_session_messages_for_service(&guard.workspace_snapshot, parsed_session_id);
        (
            controller_runtime,
            guard.agent_runtime.clone(),
            guard.detached_subagent_task_listeners.clone(),
            in_memory_messages,
        )
    };
    if let Some(mut messages) = in_memory_messages {
        let task_mirrors =
            vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors(
                &controller_runtime.data_root,
                parsed_session_id,
            )?;
        apply_task_mirror_statuses_to_parent_messages(
            messages.as_mut_slice(),
            None,
            task_mirrors.as_slice(),
            |task_id| {
                webui_subagent_task_has_live_run(
                    agent_runtime.as_ref().map(|runtime| &runtime.task_runtime),
                    live_listener_task_ids.contains(&task_id),
                    task_id,
                )
            },
        );
        return Ok(conversation_messages_from_session(
            &session_id,
            messages.as_slice(),
        ));
    }
    let mut hydration_snapshot =
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime.clone(),
            parsed_session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) if is_missing_segmented_session_file_error(&error) => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
    let task_mirrors =
        vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors(
            &controller_runtime.data_root,
            parsed_session_id,
        )?;
    apply_task_mirror_statuses_to_parent_messages(
        hydration_snapshot.messages.as_mut_slice(),
        hydration_snapshot.runtime_state.as_mut(),
        task_mirrors.as_slice(),
        |task_id| {
            webui_subagent_task_has_live_run(
                agent_runtime.as_ref().map(|runtime| &runtime.task_runtime),
                live_listener_task_ids.contains(&task_id),
                task_id,
            )
        },
    );

    Ok(conversation_messages_from_session(
        &session_id,
        hydration_snapshot.messages.as_slice(),
    ))
}

/// Opens the native attachment picker and stages selected files for composer use.
/// 打开原生附件选择器，并将选中文件暂存给输入区使用。
#[tauri::command]
async fn service_pick_composer_attachments(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    kind: Option<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    let (workspace_path, kind_filter) = composer_attachment_workspace_and_kind(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        kind.as_deref(),
    )?;
    let mut dialog = rfd::FileDialog::new();
    if let Some(kind_filter) = kind_filter.as_ref() {
        dialog = dialog.add_filter(
            composer_attachment_kind_label(kind_filter),
            composer_attachment_kind_extensions(kind_filter),
        );
    } else {
        dialog = dialog.add_filter(
            "附件",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "bmp", "mp3", "wav", "m4a", "aac", "flac",
                "ogg", "mp4", "mov", "mkv", "webm", "avi", "pdf",
            ],
        );
    }
    let Some(paths) = dialog.pick_files() else {
        return Ok(Vec::new());
    };
    stage_composer_attachment_paths_for_workspace(&workspace_path, paths)
}

/// Stages existing local paths as composer attachments.
/// 将已有本地路径暂存为输入区附件。
#[tauri::command]
async fn service_stage_composer_attachment_paths(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    paths: Vec<String>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    stage_composer_attachment_paths_for_workspace(&workspace_path, paths)
}

/// Stages one browser-provided in-memory file as a composer attachment.
/// 将浏览器提供的一份内存文件暂存为输入区附件。
#[tauri::command]
async fn service_stage_composer_attachment_bytes(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    file_name: String,
    mime_type: String,
    data_base64: String,
) -> Result<ComposerAttachmentSummary, String> {
    let workspace_path = composer_attachment_workspace_path(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let descriptor = attachment_descriptor_from_mime_type(&mime_type)
        .or_else(|| attachment_descriptor_from_path_fallback(&file_name))
        .ok_or_else(|| format!("文件 `{}` 不是受支持的附件格式。", file_name.trim()))?;
    let bytes = BASE64_STANDARD
        .decode(data_base64.trim())
        .map_err(|error| format!("解码附件 `{}` 失败：{}。", file_name.trim(), error))?;
    let attachment = stage_attachment_bytes(
        &workspace_path,
        &descriptor,
        file_name.trim(),
        None,
        bytes.as_slice(),
    )?;
    Ok(composer_attachment_summary_from_core(&attachment))
}

/// Resolves workspace path and optional attachment-kind filter for composer staging.
/// 解析输入区附件暂存所需的工作区路径与可选附件类型过滤。
fn composer_attachment_workspace_and_kind(
    state: &tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    kind: Option<&str>,
) -> Result<(String, Option<ConversationAttachmentKind>), String> {
    let workspace_path = composer_attachment_workspace_path(state, project_id, conversation_id)?;
    let kind = kind.map(parse_composer_attachment_kind).transpose()?;
    Ok((workspace_path, kind))
}

/// Resolves the project workspace path used to stage one composer attachment.
/// 解析暂存输入区附件时使用的项目工作区路径。
fn composer_attachment_workspace_path(
    state: &tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(workspace_root().display().to_string());
    };
    let owner_project_id = conversation_id
        .and_then(parse_session_id)
        .and_then(|session_id| {
            workspace_snapshot
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .and_then(|session| session.project_id.as_deref())
        })
        .or(project_id)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_project_id) = owner_project_id else {
        return Ok(workspace_root().display().to_string());
    };
    Ok(workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == owner_project_id)
        .map(|project| project.path.clone())
        .unwrap_or_else(|| workspace_root().display().to_string()))
}

/// Stages local paths for a resolved workspace path and converts them to service DTOs.
/// 为已解析的工作区路径暂存本地路径，并转换为 service DTO。
fn stage_composer_attachment_paths_for_workspace(
    workspace_path: &str,
    paths: Vec<PathBuf>,
) -> Result<Vec<ComposerAttachmentSummary>, String> {
    paths
        .iter()
        .map(|path| stage_attachment_file(workspace_path, path))
        .map(|result| result.map(|attachment| composer_attachment_summary_from_core(&attachment)))
        .collect()
}

/// Converts a core conversation attachment into the WebUI service DTO.
/// 将核心会话附件转换为 WebUI service DTO。
fn composer_attachment_summary_from_core(
    attachment: &ConversationAttachment,
) -> ComposerAttachmentSummary {
    ComposerAttachmentSummary {
        kind: attachment.kind.modality_name().to_string(),
        file_name: attachment.file_name.clone(),
        local_path: attachment.local_path.clone(),
        source_path: attachment.source_path.clone(),
        asset_id: attachment.asset_id.clone(),
        byte_len: attachment.byte_len,
        preview_local_path: attachment.preview_local_path.clone(),
        mime_type: attachment.mime_type.clone(),
    }
}

/// Converts a WebUI service attachment DTO back into the core conversation attachment.
/// 将 WebUI service 附件 DTO 转回核心会话附件。
fn composer_attachment_summary_to_core(
    summary: ComposerAttachmentSummary,
) -> Result<ConversationAttachment, String> {
    Ok(ConversationAttachment {
        kind: parse_composer_attachment_kind(&summary.kind)?,
        file_name: summary.file_name,
        local_path: summary.local_path,
        source_path: summary.source_path,
        asset_id: summary.asset_id,
        byte_len: summary.byte_len,
        preview_local_path: summary.preview_local_path,
        mime_type: summary.mime_type,
    })
}

/// Parses one stable composer attachment kind key.
/// 解析单个稳定输入区附件类型键。
fn parse_composer_attachment_kind(value: &str) -> Result<ConversationAttachmentKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "image" => Ok(ConversationAttachmentKind::Image),
        "audio" => Ok(ConversationAttachmentKind::Audio),
        "video" => Ok(ConversationAttachmentKind::Video),
        "pdf" | "document" => Ok(ConversationAttachmentKind::Pdf),
        other => Err(format!("不支持的附件类型：{other}")),
    }
}

/// Resolves an attachment descriptor from a browser file name when the MIME type is missing.
/// 当浏览器未提供 MIME 类型时，从文件名解析附件描述。
fn attachment_descriptor_from_path_fallback(
    file_name: &str,
) -> Option<vulcan_agent_runtime::attachment_assets::AttachmentDescriptor> {
    attachment_descriptor_from_path(Path::new(file_name))
}

/// Returns the localized label used by the native file picker filter.
/// 返回原生文件选择器过滤器使用的本地化名称。
fn composer_attachment_kind_label(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "图片",
        ConversationAttachmentKind::Audio => "音频",
        ConversationAttachmentKind::Video => "视频",
        ConversationAttachmentKind::Pdf => "PDF",
    }
}

/// Returns file extensions accepted by the native picker for one attachment kind.
/// 返回单种附件类型在原生选择器中接受的扩展名。
fn composer_attachment_kind_extensions(
    kind: &ConversationAttachmentKind,
) -> &'static [&'static str] {
    match kind {
        ConversationAttachmentKind::Image => &["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        ConversationAttachmentKind::Audio => &["mp3", "wav", "m4a", "aac", "flac", "ogg"],
        ConversationAttachmentKind::Video => &["mp4", "mov", "mkv", "webm", "avi"],
        ConversationAttachmentKind::Pdf => &["pdf"],
    }
}

/// Returns the browser file picker accept list for one attachment kind.
/// 返回单种附件类型对应的浏览器文件选择 accept 列表。
fn composer_attachment_kind_accept(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "image/png,image/jpeg,image/webp,image/gif,image/bmp",
        ConversationAttachmentKind::Audio => {
            "audio/mpeg,audio/wav,audio/mp4,audio/aac,audio/flac,audio/ogg"
        }
        ConversationAttachmentKind::Video => {
            "video/mp4,video/quicktime,video/x-matroska,video/webm,video/x-msvideo"
        }
        ConversationAttachmentKind::Pdf => "application/pdf",
    }
}

/// Returns the current in-memory transcript projection for one service session.
/// 返回单个服务会话当前的内存转录投影。
fn in_memory_session_messages_for_service(
    workspace_snapshot: &Option<WorkspaceSnapshot>,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    in_memory_session_messages_from_workspace(workspace_snapshot.as_ref()?, session_id)
}

/// Returns the current in-memory transcript projection for one session from a concrete workspace snapshot.
/// 从具体工作区快照返回单个会话当前的内存转录投影。
fn in_memory_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) {
        let messages = conversation_state_to_session_messages(&runtime_state.conversation_state);
        if !messages.is_empty() {
            return Some(messages);
        }
    }
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .cloned()
}

/// Submits one user prompt through the shared agent runtime and returns the immediate workbench snapshot.
/// 通过共享 Agent 运行时提交一条用户提示词，并返回即时工作台快照。
#[tauri::command]
async fn service_submit_conversation_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    prompt: String,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    disabled_ai_capability_kinds: Option<Vec<String>>,
    disabled_ai_capability_target_selectors: Option<Vec<String>>,
    ai_capability_target_order_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<WorkbenchSnapshot, String> {
    let prompt = prompt.trim().to_string();
    webui_trace(
        "submit_prompt_received",
        format!(
            "conversation_id={} project_id={} prompt_chars={} attachments={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            project_id.as_deref().unwrap_or("<none>"),
            prompt.chars().count(),
            attachments.as_ref().map(Vec::len).unwrap_or(0)
        ),
    );
    let attachments = attachments
        .unwrap_or_default()
        .into_iter()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    if prompt.is_empty() && attachments.is_empty() {
        return Err(String::from("请输入任务指令。"));
    }

    // Measures runtime handle acquisition before workspace hydration begins.
    // 衡量工作区水合开始前获取运行时句柄的耗时。
    let runtime_parts_started_at = current_unix_millis_value();
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    webui_trace(
        "submit_prompt_runtime_parts_ready",
        format!(
            "conversation_id={} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(runtime_parts_started_at)
        ),
    );
    let submission_session_id = conversation_id.as_deref().and_then(parse_session_id);
    // Measures retained workspace reuse without cloning the full workbench snapshot on the hot path.
    // 衡量保留工作区复用，并避免在热路径克隆完整工作台快照。
    let retained_snapshot_started_at = current_unix_millis_value();
    let (preserved_workspace_run_state, retained_context_available, retained_workspace_snapshot) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let retained_context_available = submission_session_id.is_some_and(|session_id| {
            guard
                .workspace_snapshot
                .as_ref()
                .is_some_and(|workspace_snapshot| {
                    retained_submission_snapshot_has_session_context(workspace_snapshot, session_id)
                })
        });
        // Reuses the already hydrated workspace on the submit hot path when it contains the selected session context.
        // 当保留工作区已经包含选中会话上下文时，在发送热路径直接复用它。
        let retained_workspace_snapshot = retained_context_available
            .then(|| guard.workspace_snapshot.clone())
            .flatten();
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            submission_session_id,
        );
        (
            preserved_workspace_run_state,
            retained_context_available,
            retained_workspace_snapshot,
        )
    };
    webui_trace_if_slow(
        "submit_prompt_retained_snapshot_slow",
        retained_snapshot_started_at,
        format!(
            "conversation_id={} retained_context_available={retained_context_available} preserved_sessions={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            preserved_workspace_run_state
                .as_ref()
                .map(|state| state.sessions.len())
                .unwrap_or_default()
        ),
    );
    let workspace_load_started_at = current_unix_millis_value();
    let (mut workspace_snapshot, workspace_source) = if let Some(workspace_snapshot) =
        retained_workspace_snapshot
    {
        (workspace_snapshot, "retained")
    } else {
        (
            load_workspace_for_submission(controller_runtime.clone(), conversation_id.as_deref())
                .await?,
            "disk",
        )
    };
    webui_trace(
        "submit_prompt_workspace_ready",
        format!(
            "conversation_id={} retained_context_available={retained_context_available} source={workspace_source} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(workspace_load_started_at)
        ),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    if let Some(session_id) = conversation_id.as_deref().and_then(parse_session_id) {
        let blocking_check_started_at = current_unix_millis_value();
        // Measures the active stream and kernel state lookup portion of the blocking gate.
        // 衡量阻塞闸门中活跃流与内核状态查询部分的耗时。
        let active_state_started_at = current_unix_millis_value();
        let (has_active_stream_task, has_active_kernel_turn, has_active_session_run) = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            (
                guard.active_stream_tasks.contains_key(&session_id),
                webui_session_kernel_has_active_turn(&guard, session_id),
                webui_session_has_active_run(&guard, session_id),
            )
        };
        webui_trace_if_slow(
            "submit_prompt_active_state_check_slow",
            active_state_started_at,
            format!("session={session_id}"),
        );
        let has_running_subagents = false;
        webui_trace(
            "submit_prompt_subagent_running_check_skipped",
            format!("session={session_id} reason=independent_subagent_lane"),
        );
        webui_trace(
            "submit_prompt_blocking_check",
            format!(
                "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_session_run={has_active_session_run} has_running_subagents={has_running_subagents} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(blocking_check_started_at)
            ),
        );
        if session_has_blocking_system_prompt_work(
            &workspace_snapshot,
            &agent_runtime.provider_configuration,
            session_id,
            has_active_session_run,
            has_running_subagents,
        ) {
            webui_trace(
                "submit_prompt_queued",
                format!(
                    "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_running_subagents={has_running_subagents}"
                ),
            );
            let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
            let entry = build_webui_pending_prompt_panel_entry(
                prompt,
                attachments,
                current_pending_prompt_model_reference(&agent_runtime.provider_configuration),
                pending_prompt_runtime_options_from_submission(
                    &agent_runtime,
                    reasoning_effort.as_deref(),
                    disabled_ai_capability_kinds.as_deref(),
                )?,
            );
            enqueue_webui_pending_prompt_panel_entry(
                &mut workspace_snapshot,
                &agent_runtime,
                session_id,
                &workspace_path,
                entry,
            )?;
            let workbench = store_and_emit_workbench_snapshot(
                &app,
                &state,
                &controller_runtime,
                &luaskills_catalog,
                &token_budget_runtime,
                &mcp_catalog,
                &agent_runtime.provider_configuration,
                &agent_runtime.ai_tool_configuration,
                workspace_snapshot,
                &logs,
            )?;
            let state_for_persist = state.inner().clone();
            let controller_runtime_for_persist = controller_runtime.clone();
            tauri::async_runtime::spawn(async move {
                let snapshot =
                    match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                        Ok(Some(snapshot)) => snapshot,
                        Ok(None) => return,
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] queued submit persist snapshot failed: {error}"
                            );
                            return;
                        }
                    };
                spawn_workspace_session_persist_task(
                    state_for_persist,
                    controller_runtime_for_persist,
                    snapshot,
                    session_id,
                    Some(String::from("queued_submit")),
                );
            });
            return Ok(workbench);
        }
    }
    let materialize_started_at = current_unix_millis_value();
    let (session_id, session_title, workspace_path) = materialize_prompt_session(
        &mut workspace_snapshot,
        project_id.as_deref(),
        conversation_id.as_deref(),
        if prompt.is_empty() {
            "附件会话"
        } else {
            &prompt
        },
    )?;
    webui_trace(
        "submit_prompt_materialize_ready",
        format!(
            "session={session_id} title_chars={} workspace_path_chars={} elapsed_ms={}",
            session_title.chars().count(),
            workspace_path.chars().count(),
            current_unix_millis_value().saturating_sub(materialize_started_at)
        ),
    );
    ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None);
    if conversation_id.is_some() {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.session_subagent_navigation_rounds.remove(&session_id);
        if guard
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            guard.active_subagent_panel = None;
        }
    }
    webui_trace(
        "submit_prompt_starting_run",
        format!(
            "session={session_id} title_chars={}",
            session_title.chars().count()
        ),
    );
    let submit_run_prepare_started_at = current_unix_millis_value();
    let disabled_ai_capability_kinds = effective_workbench_disabled_ai_capability_kinds(
        &agent_runtime.ai_tool_configuration,
        disabled_ai_capability_kinds.as_deref(),
    )?;
    let disabled_ai_capability_target_selectors =
        parse_ai_capability_target_selectors(disabled_ai_capability_target_selectors);
    let ai_capability_target_order_overrides =
        parse_ai_capability_target_order_overrides(ai_capability_target_order_overrides)?;
    webui_trace(
        "submit_prompt_runtime_options_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(submit_run_prepare_started_at)
        ),
    );
    let context_snapshot_started_at = current_unix_millis_value();
    snapshot_webui_context_before_user_prompt(
        &controller_runtime,
        &workspace_snapshot,
        session_id,
        PendingSessionPromptSource::UserFollowup,
    )?;
    webui_trace(
        "submit_prompt_context_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(context_snapshot_started_at)
        ),
    );
    let turn_begin_started_at = current_unix_millis_value();
    let precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: format!(
                    "webui-prompt-{session_id}-{}",
                    current_unix_millis_text()
                ),
                prompt: &prompt,
                attachments,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: execution_mode.unwrap_or_else(|| String::from("build")),
                reasoning_effort: reasoning_effort.as_deref().and_then(parse_reasoning_effort),
                disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors,
                ai_capability_target_order_overrides,
            },
        )
    }?;
    webui_trace(
        "submit_prompt_turn_begin_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(turn_begin_started_at)
        ),
    );
    {
        let kernel_turn_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_user_turn(
            &mut guard,
            session_id,
            precheck_start.precheck_request.turn_id(),
        )?;
        webui_trace(
            "submit_prompt_kernel_turn_started",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(kernel_turn_started_at)
            ),
        );
    }
    let outer_steps_started_at = current_unix_millis_value();
    apply_webui_prompt_begin_outer_steps(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    webui_trace(
        "submit_prompt_outer_steps_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(outer_steps_started_at)
        ),
    );
    let hook_snapshot_started_at = current_unix_millis_value();
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    webui_trace(
        "submit_prompt_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    let precheck_plan_started_at = current_unix_millis_value();
    let precheck_plan = webui_shared_prompt_precheck_plan(
        &agent_runtime,
        &workspace_snapshot,
        session_id,
        &workspace_path,
    )?;
    webui_trace(
        "submit_prompt_precheck_plan_ready",
        format!(
            "session={session_id} has_hooks={} skip={} labels={} elapsed_ms={}",
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len(),
            current_unix_millis_value().saturating_sub(precheck_plan_started_at)
        ),
    );
    let store_emit_started_at = current_unix_millis_value();
    let mut workbench = store_and_emit_workbench_snapshot_preserving_subagent_navigation(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    webui_trace(
        "submit_prompt_workbench_emitted",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(store_emit_started_at)
        ),
    );
    let state_for_persist = state.inner().clone();
    let controller_runtime_for_persist = controller_runtime.clone();
    tauri::async_runtime::spawn(async move {
        let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[tauri-service] submit begin persist snapshot failed: {error}");
                return;
            }
        };
        spawn_workspace_session_persist_task(
            state_for_persist,
            controller_runtime_for_persist,
            snapshot,
            session_id,
            Some(String::from("submit_begin")),
        );
    });
    let service_state = state.inner().clone();
    let precheck_dispatch_started_at = current_unix_millis_value();
    if start_webui_precheck_or_continue(
        &app,
        &service_state,
        &controller_runtime,
        &agent_runtime,
        precheck_start,
        precheck_plan,
    )? {
        workbench = current_workbench_snapshot_from_state(&state)?;
    }
    webui_trace(
        "submit_prompt_precheck_dispatch_returned",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(precheck_dispatch_started_at)
        ),
    );
    Ok(workbench)
}

/// Stops the selected WebUI conversation run and clears guided internal queue work.
/// 停止当前选中的 WebUI 会话运行，并清理已引导的内部队列任务。
#[tauri::command]
async fn service_stop_conversation_run(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    webui_trace(
        "stop_conversation_requested",
        format!("session={selected_session_id}"),
    );
    let active_task = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .active_stream_tasks
        .remove(&selected_session_id);
    let had_active_kernel_turn = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        stop_webui_session_kernel_generation(&mut guard, selected_session_id)
    };
    let had_active_stream_task = active_task.is_some();
    if let Some(active_task) = active_task {
        active_task.abort();
    }
    let mut workspace_snapshot = with_workspace_mutation(&app, state.inner(), |workspace| {
        mark_webui_session_stop_requested_in_workspace(workspace, selected_session_id);
    })?;
    let running_subagent_task_states = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        running_webui_subagent_task_states_for_tree_from_cache(
            &mut guard,
            &workspace_snapshot,
            selected_session_id,
        )
    };
    webui_trace(
        "stop_conversation_state",
        format!(
            "session={selected_session_id} had_active_kernel_turn={had_active_kernel_turn} had_active_stream_task={had_active_stream_task} running_subagents={}",
            running_subagent_task_states.len()
        ),
    );
    for (direct_parent_session_id, task_state) in &running_subagent_task_states {
        let _ = agent_runtime
            .task_runtime
            .stop_task_tree(task_state.task_id);
        mark_webui_subagent_task_stopped_in_workspace(
            &mut workspace_snapshot,
            *direct_parent_session_id,
            task_state.task_id,
            Some(task_state),
        );
    }
    if !running_subagent_task_states.is_empty() {
        let running_subagent_task_ids = running_subagent_task_states
            .iter()
            .map(|(direct_parent_session_id, state)| (*direct_parent_session_id, state.task_id))
            .collect::<Vec<_>>();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        mark_webui_subagent_tasks_stopped_in_state(&mut guard, &running_subagent_task_ids);
    }
    let did_stop_work = stop_webui_session_work(
        &mut workspace_snapshot,
        selected_session_id,
        had_active_kernel_turn,
        !running_subagent_task_states.is_empty(),
    )?;
    if did_stop_work
        && !workspace_snapshot
            .session_runtime_state
            .contains_key(&selected_session_id)
    {
        return Err(String::from("停止后未找到会话运行时状态。"));
    }
    let workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    if did_stop_work {
        let state_for_persist = state.inner().clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!(
                        "[tauri-service] stopped conversation persist snapshot failed: {error}"
                    );
                    return;
                }
            };
            if !snapshot
                .session_runtime_state
                .contains_key(&selected_session_id)
            {
                return;
            }
            if let Err(error) = persist_workspace_session_update(
                Some(&state_for_persist),
                controller_runtime_for_persist,
                &snapshot,
                selected_session_id,
            )
            .await
            {
                eprintln!(
                    "[tauri-service] stopped conversation background persist failed: {error}"
                );
            }
        });
    }
    Ok(workbench)
}

/// Opens one subagent transcript surface under the selected parent conversation.
/// 打开当前父会话下的单个子代理转录界面。
#[tauri::command]
async fn service_open_subagent_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    parent_conversation_id: String,
    task_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let _ = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    webui_trace(
        "open_subagent_panel_requested",
        format!("parent={parent_session_id} task={task_session_id}"),
    );
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if guard.active_subagent_panel
            == Some(SelectedSubagentTaskPanel {
                parent_session_id,
                task_id: task_session_id,
            })
        {
            webui_trace(
                "open_subagent_panel_noop_active",
                format!("parent={parent_session_id} task={task_session_id}"),
            );
            return Ok(guard.workbench.clone());
        }
    }
    let cache_open_started_at = current_unix_millis_value();
    let fast_path_workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let mut workspace_snapshot = guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        restore_webui_subagent_surface_detail_cache_for_task(
            &mut guard,
            &mut workspace_snapshot,
            parent_session_id,
            task_session_id,
        );
        let opened_from_cache = match open_webui_subagent_panel_from_cached_state(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
            task_session_id,
        ) {
            Ok(opened_from_cache) => opened_from_cache,
            Err(error) => {
                guard.workspace_snapshot = Some(workspace_snapshot);
                return Err(error);
            }
        };
        let workbench = opened_from_cache.then(|| {
            build_subagent_display_route_workbench_snapshot_locked(&mut guard, &workspace_snapshot)
        });
        guard.workspace_snapshot = Some(workspace_snapshot);
        workbench
    };
    webui_trace_if_slow(
        "open_subagent_panel_cache_slow",
        cache_open_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} opened_from_cache={}",
            fast_path_workbench.is_some()
        ),
    );
    if let Some(workbench) = fast_path_workbench {
        let emit_started_at = current_unix_millis_value();
        let _ = app.emit(
            "vulcan://service-event",
            ServiceEvent::Workbench(Box::new(workbench.clone())),
        );
        webui_trace_if_slow(
            "open_subagent_panel_emit_slow",
            emit_started_at,
            format!("parent={parent_session_id} task={task_session_id} source=cache"),
        );
        return Ok(workbench);
    }
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let detail_load_started_at = current_unix_millis_value();
    let detailed_state = segmented_webui_subagent_task_panel_state_for_task(
        &workspace_snapshot,
        parent_session_id,
        task_session_id,
    );
    webui_trace_if_slow(
        "open_subagent_panel_detail_load_slow",
        detail_load_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} found={}",
            detailed_state.is_some()
        ),
    );
    {
        let cache_merge_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(detailed_state) = detailed_state {
            cache_webui_subagent_surface_detail_for_task(
                &mut guard,
                &mut workspace_snapshot,
                parent_session_id,
                detailed_state.clone(),
            );
            merge_webui_subagent_task_cache_for_session(
                &mut guard,
                parent_session_id,
                vec![detailed_state],
            );
        }
        if !open_webui_subagent_panel_from_cached_state(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
            task_session_id,
        )? {
            webui_trace(
                "open_subagent_panel_missing",
                format!("parent={parent_session_id} task={task_session_id}"),
            );
            return Err(String::from("没有找到可打开的子代理任务。"));
        }
        webui_trace_if_slow(
            "open_subagent_panel_cache_merge_slow",
            cache_merge_started_at,
            format!("parent={parent_session_id} task={task_session_id}"),
        );
    }
    let emit_started_at = current_unix_millis_value();
    let snapshot =
        emit_subagent_display_route_workbench_snapshot(&app, state.inner(), workspace_snapshot);
    webui_trace_if_slow(
        "open_subagent_panel_emit_slow",
        emit_started_at,
        format!("parent={parent_session_id} task={task_session_id} source=detail"),
    );
    snapshot
}

/// Opens a WebUI subagent panel from the in-memory task cache without touching disk.
/// 从内存任务缓存打开 WebUI 子代理面板，不触碰磁盘。
fn open_webui_subagent_panel_from_cached_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Result<bool, String> {
    let Some(task_state) = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .and_then(|tasks| tasks.get(&task_session_id))
        .cloned()
    else {
        return Ok(false);
    };
    let has_live_listener = guard
        .detached_subagent_task_listeners
        .contains(&task_session_id);
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let is_live_running =
        webui_subagent_task_has_live_run(task_runtime, has_live_listener, task_session_id);
    let normalized = normalize_subagent_task_panel_state(&task_state, is_live_running, None);
    let has_recoverable_detail =
        webui_subagent_panel_has_recoverable_detail(workspace_snapshot, &normalized);
    let is_openable = subagent_task_panel_is_openable(&normalized, has_recoverable_detail);
    webui_trace(
        "open_subagent_panel_evaluated",
        format!(
            "parent={parent_session_id} task={task_session_id} run_state={} has_recoverable_detail={has_recoverable_detail} is_openable={is_openable} messages={}",
            subagent_run_state_key(normalized.run_state),
            workspace_snapshot
                .session_messages
                .get(&task_session_id)
                .map(Vec::len)
                .unwrap_or(0)
        ),
    );
    if !is_openable {
        return Ok(false);
    }
    guard
        .session_subagent_tasks
        .entry(parent_session_id)
        .or_default()
        .insert(task_session_id, normalized);
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: task_session_id,
    });
    Ok(true)
}

/// Closes the active subagent transcript surface and returns to the parent conversation.
/// 关闭活动子代理转录界面并返回父会话。
#[tauri::command]
async fn service_close_subagent_panel(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    let _ = runtime_parts_from_state(&state)?;
    let workspace_snapshot = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        webui_trace(
            "close_subagent_panel_requested",
            format!(
                "active_parent={} active_task={}",
                guard
                    .active_subagent_panel
                    .map(|selection| selection.parent_session_id.to_string())
                    .unwrap_or_else(|| String::from("<none>")),
                guard
                    .active_subagent_panel
                    .map(|selection| selection.task_id.to_string())
                    .unwrap_or_else(|| String::from("<none>"))
            ),
        );
        guard.active_subagent_panel = None;
        guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    emit_subagent_display_route_workbench_snapshot(&app, state.inner(), workspace_snapshot)
}

/// Stops one running subagent task through the shared task runtime.
/// 通过共享任务运行时停止单个运行中的子代理任务。
#[tauri::command]
async fn service_stop_subagent_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    parent_conversation_id: String,
    task_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    webui_trace(
        "stop_subagent_requested",
        format!("parent={parent_session_id} task={task_session_id}"),
    );
    let task_states_to_mark = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let root_state = guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .and_then(|tasks| tasks.get(&task_session_id))
            .cloned();
        let mut task_states_to_mark = vec![(parent_session_id, task_session_id, root_state)];
        task_states_to_mark.extend(
            running_webui_subagent_task_states_for_tree_from_cache(
                &mut guard,
                &workspace_snapshot,
                task_session_id,
            )
            .into_iter()
            .map(|(direct_parent_session_id, task_state)| {
                (
                    direct_parent_session_id,
                    task_state.task_id,
                    Some(task_state),
                )
            }),
        );
        guard.workspace_snapshot = Some(workspace_snapshot);
        task_states_to_mark
    };
    let stopped_task_ids = agent_runtime.task_runtime.stop_task_tree(task_session_id)?;
    webui_trace(
        "stop_subagent_result",
        format!(
            "parent={parent_session_id} task={task_session_id} stopped_count={}",
            stopped_task_ids.len()
        ),
    );
    let stopped_task_id_set = stopped_task_ids.iter().copied().collect::<BTreeSet<_>>();
    let stopped_parent_session_ids = task_states_to_mark
        .iter()
        .filter(|(_, task_id, _)| stopped_task_id_set.contains(task_id))
        .map(|(direct_parent_session_id, _, _)| *direct_parent_session_id)
        .collect::<BTreeSet<_>>();
    let workspace_snapshot = if !stopped_task_id_set.is_empty() {
        with_workspace_mutation(&app, state.inner(), |workspace| {
            for (direct_parent_session_id, task_id, task_state) in &task_states_to_mark {
                if !stopped_task_id_set.contains(task_id) {
                    continue;
                }
                mark_webui_subagent_task_stopped_in_workspace(
                    workspace,
                    *direct_parent_session_id,
                    *task_id,
                    task_state.as_ref(),
                );
            }
        })?
    } else {
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !stopped_task_id_set.is_empty() {
            let stopped_task_pairs = task_states_to_mark
                .iter()
                .filter(|(_, task_id, _)| stopped_task_id_set.contains(task_id))
                .map(|(direct_parent_session_id, task_id, _)| (*direct_parent_session_id, *task_id))
                .collect::<Vec<_>>();
            mark_webui_subagent_tasks_stopped_in_state(&mut guard, &stopped_task_pairs);
            for task_id in &stopped_task_id_set {
                guard.detached_subagent_task_listeners.remove(task_id);
                if guard
                    .active_subagent_panel
                    .is_some_and(|selection| selection.task_id == *task_id)
                {
                    guard.active_subagent_panel = None;
                }
            }
        } else {
            guard
                .detached_subagent_task_listeners
                .remove(&task_session_id);
        }
    }
    let workbench = store_current_workbench_snapshot(
        &app,
        state.inner(),
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )?;
    if !stopped_parent_session_ids.is_empty() {
        let state_for_persist = state.inner().clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[tauri-service] stopped subagent persist snapshot failed: {error}");
                    return;
                }
            };
            for parent_session_id in stopped_parent_session_ids {
                if !snapshot
                    .sessions
                    .iter()
                    .any(|session| session.id == parent_session_id)
                {
                    continue;
                }
                if let Err(error) = persist_workspace_session_update(
                    Some(&state_for_persist),
                    controller_runtime_for_persist.clone(),
                    &snapshot,
                    parent_session_id,
                )
                .await
                {
                    eprintln!(
                        "[tauri-service] stopped subagent background persist failed: {error}"
                    );
                }
            }
        });
    }
    Ok(workbench)
}

/// Inserts one visible pending prompt into the shared internal prompt queue.
/// 将单条可见待发送提示词插入共享内部提示词队列。
#[tauri::command]
async fn service_insert_pending_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    entry_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let workspace_path = workspace_path_for_session(&workspace_snapshot, selected_session_id);
    insert_webui_pending_prompt_into_internal_queue(
        &mut workspace_snapshot,
        &agent_runtime,
        selected_session_id,
        &workspace_path,
        &entry_id,
    )?;
    mark_webui_session_waiting_for_guided_prompt_dispatch(
        &mut workspace_snapshot,
        selected_session_id,
    );
    let mut workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    let snapshot_for_persist = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    spawn_workspace_session_persist_task(
        state.inner().clone(),
        controller_runtime.clone(),
        snapshot_for_persist,
        selected_session_id,
        Some(String::from("guided_insert")),
    );
    let started_next_prompt = match maybe_prepare_next_webui_pending_prompt(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        selected_session_id,
        true,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => return Err(error),
    };
    if started_next_prompt {
        workbench = current_workbench_snapshot_from_state(&state)?;
    }
    Ok(workbench)
}

/// Restores one visible pending prompt into the WebUI composer and removes it from the queue.
/// 将单条可见待发送提示词恢复到 WebUI 输入区，并从队列中移除。
#[tauri::command]
async fn service_edit_pending_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    entry_id: String,
) -> Result<PendingPromptSummary, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let summary = {
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&selected_session_id)
        else {
            return Err(String::from("当前会话没有可编辑的待发送队列。"));
        };
        let Some(entry) = remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id)
        else {
            return Err(String::from("没有找到待编辑的待发送提示词。"));
        };
        runtime_state.updated_at = current_unix_millis_text();
        pending_prompt_summary_from_panel_entry(&entry)
    };
    store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    let snapshot_for_persist = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    spawn_workspace_session_persist_task(
        state.inner().clone(),
        controller_runtime.clone(),
        snapshot_for_persist,
        selected_session_id,
        Some(String::from("pending_edit")),
    );
    Ok(summary)
}

/// Removes one visible pending prompt from the selected conversation queue.
/// 从当前选中会话队列中移除单条可见待发送提示词。
#[tauri::command]
async fn service_remove_pending_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    entry_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let removed = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&selected_session_id)
    {
        if remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id).is_some() {
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    let workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    if removed {
        let snapshot_for_persist =
            current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
        spawn_workspace_session_persist_task(
            state.inner().clone(),
            controller_runtime.clone(),
            snapshot_for_persist,
            selected_session_id,
            Some(String::from("pending_remove")),
        );
    }
    Ok(workbench)
}

/// Reorders the visible pending prompt queue for the selected conversation.
/// 重排当前选中会话的可见待发送提示词队列。
#[tauri::command]
async fn service_reorder_pending_prompts(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: Option<String>,
    dragged_entry_id: String,
    target_entry_id: Option<String>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let selected_session_id = conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .map(Ok)
        .unwrap_or_else(|| selected_session_id_from_state(&state))?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    workspace_snapshot.selected_session = selected_session_id;
    let drop_target = target_entry_id
        .filter(|entry_id| !entry_id.trim().is_empty())
        .map(PendingPromptPanelDropTarget::Before)
        .unwrap_or(PendingPromptPanelDropTarget::End);
    let did_reorder = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&selected_session_id)
    {
        if apply_pending_prompt_panel_drop_target(
            &mut runtime_state.ui_state,
            &dragged_entry_id,
            &drop_target,
        ) {
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    let workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    if did_reorder {
        let snapshot_for_persist =
            current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
        spawn_workspace_session_persist_task(
            state.inner().clone(),
            controller_runtime.clone(),
            snapshot_for_persist,
            selected_session_id,
            Some(String::from("pending_reorder")),
        );
    }
    Ok(workbench)
}

/// Selects and persists the active workbench provider/model pair.
/// 选择并持久化当前工作台使用的供应商与模型组合。
#[tauri::command]
async fn service_select_workbench_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    provider_id: String,
    model_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let provider_id = provider_id.trim().to_string();
    let model_id = model_id.trim().to_string();
    if provider_id.is_empty() || model_id.is_empty() {
        return Err(String::from("模型选择缺少 Provider 或模型标识。"));
    }

    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let has_model = provider_config::effective_model_choices(
            &provider_id,
            &agent_runtime.provider_configuration,
        )
        .into_iter()
        .any(|choice| choice.id == model_id);
        if !has_model {
            return Err(format!(
                "模型 `{model_id}` 不在 `{provider_id}` 的已启用模型列表中。"
            ));
        }

        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.model = Some(format!("{provider_id}/{model_id}"));
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存模型选择失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        guard.workbench.providers = providers(&next_configuration);
        guard.workbench.reasoning_selection = reasoning_selection(&next_configuration);
        guard.workbench.clone()
    };

    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Persists the active workbench reasoning effort through the shared provider configuration.
/// 通过共享 Provider 配置持久化当前工作台思考档位。
#[tauri::command]
async fn service_select_workbench_reasoning_effort(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedServiceBridgeState>,
    effort: String,
) -> Result<WorkbenchSnapshot, String> {
    let selected_effort =
        parse_reasoning_effort(effort.trim()).ok_or_else(|| format!("未知思考档位：{effort}"))?;
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let agent_runtime = guard
            .agent_runtime
            .as_mut()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        let mut next_configuration = agent_runtime.provider_configuration.clone();
        next_configuration.workbench_reasoning_effort = Some(selected_effort);
        provider_config::save_configuration_file(&next_configuration)
            .map_err(|error| format!("保存思考档位失败：{error}"))?;
        agent_runtime.provider_configuration = next_configuration.clone();
        guard.workbench.reasoning_selection = reasoning_selection(&next_configuration);
        guard.workbench.clone()
    };

    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Opens one project folder in the host system file manager.
/// 在宿主系统文件管理器中打开单个项目文件夹。
#[tauri::command]
fn service_open_project_directory(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<String, String> {
    let project = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    open_path_in_system_file_manager(&project.path)?;
    Ok(format!("已请求打开项目目录：{}。", project.name))
}

/// Opens one arbitrary project directory path in the host system file manager.
/// 在宿主系统文件管理器中打开任意项目目录路径。
#[tauri::command]
fn service_open_project_directory_path(path: String) -> Result<String, String> {
    open_path_in_system_file_manager(&path)?;
    Ok(format!("已请求打开目录：{path}。"))
}

/// Opens the host folder picker and adds or restores the selected project.
/// 打开宿主文件夹选择器，并新增或恢复选中的项目。
#[tauri::command]
async fn service_add_project_from_folder(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<Option<WorkbenchSnapshot>, String> {
    let initial_directory = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .first()
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()));
    let Some(selected_path) = pick_task_project_folder(&initial_directory)? else {
        return Ok(None);
    };
    let normalized_path = normalize_task_project_path(&selected_path);
    let normalized_path_text = normalized_workspace_path_text_from_path(&normalized_path);
    let controller_runtime = controller_runtime_from_state(&state)?;
    let workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;

    if workspace_snapshot
        .task_projects
        .iter()
        .any(|project| task_project_matches_path(project, &normalized_path_text))
    {
        return refresh_workbench_snapshot(state).await.map(Some);
    }

    if let Some(project) = workspace_snapshot
        .archived_task_projects
        .iter()
        .find(|project| task_project_matches_path(project, &normalized_path_text))
        .cloned()
    {
        let restored_sessions = project
            .session_ids
            .iter()
            .filter_map(|session_id| {
                workspace_snapshot
                    .archived_sessions
                    .iter()
                    .find(|session| session.id == *session_id)
                    .cloned()
            })
            .collect::<Vec<_>>();
        vulcan_workspace_store::workspace_store::restore_project_with_sessions(
            controller_runtime,
            &workspace_snapshot.task_projects,
            &project,
            &restored_sessions,
        )
        .await?;
        return refresh_workbench_snapshot(state).await.map(Some);
    }

    let title = normalized_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_path_text.clone());
    let order = workspace_snapshot
        .task_projects
        .iter()
        .filter(|project| !project.pinned)
        .map(|project| project.order)
        .max()
        .map(|order| order + 1)
        .unwrap_or(0);
    let project_id = build_task_project_id(&workspace_snapshot, &title, &normalized_path_text);
    let project = TaskProjectSummary::new(
        project_id,
        title,
        normalized_path_text,
        false,
        order,
        None,
        None,
        Vec::new(),
    );
    vulcan_workspace_store::workspace_store::persist_new_project(controller_runtime, &project)
        .await?;
    refresh_workbench_snapshot(state).await.map(Some)
}

/// Toggles one project pinned state and returns the refreshed workbench snapshot.
/// 切换单个项目置顶状态并返回刷新后的工作台快照。
#[tauri::command]
async fn service_toggle_project_pinned(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id),
            ]],
        )
        .await
        .map_err(|error| format!("切换项目置顶状态失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Reorders visible projects inside one pinned or regular section.
/// 在同一个置顶或普通分区内重排可见项目。
#[tauri::command]
async fn service_reorder_projects(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    if project_ids.is_empty() {
        return refresh_workbench_snapshot(state).await;
    }

    let snapshot = service_workbench_snapshot(state.clone())?;
    let first_project = snapshot
        .projects
        .iter()
        .find(|project| project.id == project_ids[0])
        .ok_or_else(|| String::from("重排项目失败：第一个项目不存在。"))?;
    let section_pinned = first_project.pinned;

    for project_id in &project_ids {
        let Some(project) = snapshot
            .projects
            .iter()
            .find(|candidate| candidate.id == *project_id)
        else {
            return Err(format!("重排项目失败：项目不存在：{project_id}"));
        };
        if project.pinned != section_pinned {
            return Err(String::from("重排项目失败：不能跨置顶与普通分区拖拽。"));
        }
    }

    let controller_runtime = controller_runtime_from_state(&state)?;
    let updated_at = current_unix_millis_text();
    let rows = project_ids
        .into_iter()
        .enumerate()
        .map(|(index, project_id)| {
            vec![
                ControllerSqliteValue::Int64(index as i64),
                ControllerSqliteValue::String(updated_at.clone()),
                ControllerSqliteValue::String(project_id),
            ]
        })
        .collect();
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            rows,
        )
        .await
        .map_err(|error| format!("重排项目失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project display name and returns the refreshed workbench snapshot.
/// 应用项目显示名称并返回刷新后的工作台快照。
#[tauri::command]
async fn service_rename_project(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::rename_project(controller_runtime, &project_id, &name)
        .await?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project icon override and returns the refreshed workbench snapshot.
/// 应用项目图标覆盖并返回刷新后的工作台快照。
#[tauri::command]
async fn service_set_project_icon(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
    icon_pack: String,
    icon_key: String,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_icon_pack = normalize_non_empty_name(&icon_pack)?;
    let normalized_icon_key = normalize_non_empty_name(&icon_key)?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = ?1, icon_key = ?2, updated_at = ?3 WHERE id = ?4 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_icon_pack),
                ControllerSqliteValue::String(normalized_icon_key),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id),
            ]],
        )
        .await
        .map_err(|error| format!("设置项目图标失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Clears a project icon override and returns the refreshed workbench snapshot.
/// 清除项目图标覆盖并返回刷新后的工作台快照。
#[tauri::command]
async fn service_reset_project_icon(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = NULL, icon_key = NULL, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id),
            ]],
        )
        .await
        .map_err(|error| format!("还原项目图标失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Persists the expanded project identifiers and returns the refreshed workbench snapshot.
/// 持久化展开项目标识集合并返回刷新后的工作台快照。
#[tauri::command]
async fn service_set_expanded_task_projects(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    let expanded_project_ids = project_ids.into_iter().collect::<BTreeSet<_>>();
    let collapsed_project_ids = workspace_snapshot
        .task_projects
        .iter()
        .filter(|project| !expanded_project_ids.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    vulcan_workspace_store::workspace_store::persist_collapsed_task_project_ids(
        controller_runtime,
        &collapsed_project_ids,
    )
    .await?;
    refresh_workbench_snapshot(state).await
}

/// Archives all visible conversations under one project and returns the refreshed workbench snapshot.
/// 归档单个项目下的全部可见对话并返回刷新后的工作台快照。
#[tauri::command]
async fn service_archive_project_conversations(
    state: tauri::State<'_, SharedServiceBridgeState>,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    let Some(project) = workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == project_id)
    else {
        return Err(format!("未找到要归档的项目：{project_id}"));
    };

    vulcan_workspace_store::workspace_store::archive_project_with_sessions(
        controller_runtime,
        &project_id,
        &project.session_ids,
    )
    .await?;
    refresh_workbench_snapshot(state).await
}

/// Applies a conversation display name and returns the refreshed workbench snapshot.
/// 应用会话显示名称并返回刷新后的工作台快照。
#[tauri::command]
async fn service_rename_conversation(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_name = normalize_non_empty_name(&name)?;
    let parsed_session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::rename_segmented_session_metadata(
        &controller_runtime.data_root,
        parsed_session_id,
        &normalized_name,
        &current_unix_millis_text(),
    )
    .map_err(|error| format!("重命名会话失败：{error}"))?;
    refresh_workbench_snapshot(state).await
}

/// Archives one visible conversation and returns the refreshed workbench snapshot.
/// 归档单个可见会话并返回刷新后的工作台快照。
#[tauri::command]
async fn service_archive_conversation(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::archive_single_session(controller_runtime, session_id)
        .await?;
    refresh_workbench_snapshot(state).await
}

/// Marks one conversation as read and returns the refreshed workbench snapshot.
/// 将单个会话标记为已读并返回刷新后的工作台快照。
#[tauri::command]
async fn service_mark_conversation_read(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    let mut workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    let marked_read = if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        if runtime_state.ui_state.unread {
            runtime_state.ui_state.unread = false;
            runtime_state.updated_at = current_unix_millis_text();
            true
        } else {
            false
        }
    } else {
        false
    };
    if !marked_read {
        return refresh_workbench_snapshot(state).await;
    }
    persist_webui_session_runtime_state_update(
        state.inner(),
        controller_runtime,
        session_id,
        &workspace_snapshot,
    )
    .await?;
    refresh_workbench_snapshot(state).await
}

/// Selects one conversation in the retained WebUI workspace state.
/// 在 WebUI 保留的工作区状态中选中一个会话。
#[tauri::command]
async fn service_select_conversation(
    state: tauri::State<'_, SharedServiceBridgeState>,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        mut workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            workspace_snapshot,
        )
    };
    let exists = workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id);
    if !exists {
        return refresh_workbench_snapshot(state).await;
    }
    workspace_snapshot.selected_session = session_id;
    let (surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.active_subagent_panel = None;
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    Ok(workbench)
}

/// Opens one filesystem directory with the host file manager.
/// 使用宿主文件管理器打开单个文件系统目录。
fn open_path_in_system_file_manager(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Opens one URL with the host default browser.
/// 使用宿主默认浏览器打开单个 URL。
fn open_url_in_system_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Normalizes and validates a user-provided display name.
/// 规范化并校验用户提供的显示名称。
fn normalize_non_empty_name(name: &str) -> Result<String, String> {
    let normalized_name = name.trim();
    if normalized_name.is_empty() {
        return Err(String::from("名称不能为空。"));
    }
    Ok(normalized_name.to_string())
}

/// Reads the controller runtime handle from shared service state.
/// 从共享服务状态中读取控制器运行时句柄。
fn controller_runtime_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<ControllerRuntimeHandle, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; controller runtime is not ready"))
}

/// Reads the selected session identifier from the retained workspace state.
/// 从保留的工作区状态中读取当前选中会话标识。
fn selected_session_id_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<SessionId, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref()
        && workspace_snapshot.selected_session != SessionId::nil()
    {
        return Ok(workspace_snapshot.selected_session);
    }
    guard
        .workbench
        .selected_conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前没有选中的会话。"))
}

/// Clones the latest in-memory workspace snapshot retained by the service bridge.
/// 克隆服务桥接层保留的最近内存工作区快照。
fn current_workspace_snapshot_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<Option<WorkspaceSnapshot>, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workspace_snapshot.clone())
}

/// Clones the latest in-memory workspace snapshot from a shared service state handle.
/// 从共享服务状态句柄克隆最近的内存工作区快照。
fn current_workspace_snapshot_from_shared_state(
    state: &SharedServiceBridgeState,
) -> Result<Option<WorkspaceSnapshot>, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workspace_snapshot.clone())
}

/// Clones the latest in-memory workbench snapshot retained by the service bridge.
/// 克隆服务桥接层保留的最近内存工作台快照。
///
/// # Parameters
/// 参数
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
///
/// # Returns
/// 返回值
/// The latest workbench snapshot already emitted by an in-memory mutation.
/// 返回内存变更已经发出的最近工作台快照。
fn current_workbench_snapshot_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench.clone())
}

/// Reads all long-lived runtime handles required by a WebUI prompt submission.
/// 读取 WebUI 提示词提交所需的全部长生命周期运行时句柄。
fn runtime_parts_from_state(
    state: &tauri::State<'_, SharedServiceBridgeState>,
) -> Result<
    (
        ControllerRuntimeHandle,
        AgentRuntimeBridge,
        TauriLuaSkillsCatalog,
        Arc<TauriTokenBudgetRuntime>,
        TauriMcpRuntimeCatalog,
        Vec<String>,
    ),
    String,
> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let agent_runtime = guard
        .agent_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
    let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
    let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; token budget runtime is not ready")
    })?;
    let mcp_catalog = cached_mcp_catalog_from_state(&guard);
    Ok((
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        guard.startup.recent_logs.clone(),
    ))
}

/// Loads the workspace snapshot and hydrates the selected submitted session when it already exists.
/// 加载工作区快照，并在提交目标会话已存在时水合该会话。
async fn load_workspace_for_submission(
    controller_runtime: ControllerRuntimeHandle,
    conversation_id: Option<&str>,
) -> Result<WorkspaceSnapshot, String> {
    // Measures full workspace snapshot loading for prompt submission.
    // 衡量提示词提交时完整工作区快照载入耗时。
    let workspace_load_started_at = current_unix_millis_value();
    let mut workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    webui_trace_if_slow(
        "submit_prompt_workspace_snapshot_load_slow",
        workspace_load_started_at,
        format!("conversation_id={}", conversation_id.unwrap_or("<draft>")),
    );
    if let Some(session_id) = conversation_id.and_then(parse_session_id) {
        // Measures selected session hydration when retained context cannot be reused.
        // 衡量无法复用保留上下文时的选中会话水合耗时。
        let hydration_started_at = current_unix_millis_value();
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime.clone(),
            session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        {
            Ok(hydration) => {
                workspace_snapshot
                    .session_messages
                    .insert(session_id, hydration.messages);
                if let Some(runtime_state) = hydration.runtime_state {
                    workspace_snapshot
                        .session_runtime_state
                        .insert(session_id, runtime_state);
                }
                workspace_snapshot.hydrated_session_ids.insert(session_id);
                apply_persisted_task_mirror_statuses_to_workspace(
                    &controller_runtime,
                    &mut workspace_snapshot,
                    session_id,
                    |_| false,
                )?;
            }
            Err(error) if is_missing_segmented_session_file_error(&error) => {}
            Err(error) => return Err(error),
        }
        webui_trace_if_slow(
            "submit_prompt_session_hydration_slow",
            hydration_started_at,
            format!("session={session_id}"),
        );
    }
    Ok(workspace_snapshot)
}

/// Returns whether a retained WebUI snapshot has enough session context for prompt submission.
/// 判断保留的 WebUI 快照是否已有足够的会话上下文可直接提交提示词。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` selects the submitted conversation.
/// `session_id` 选择本次提交目标会话。
///
/// # Returns
/// 返回值
/// `true` when the session was hydrated or already owns concrete conversation context.
/// 当会话已水合或已拥有具体会话上下文时返回 `true`。
fn retained_submission_snapshot_has_session_context(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    if workspace_snapshot
        .hydrated_session_ids
        .contains(&session_id)
    {
        return true;
    }
    if workspace_snapshot
        .session_messages
        .contains_key(&session_id)
    {
        return true;
    }
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(runtime_state_has_concrete_conversation_context)
}

/// Returns whether a runtime state carries concrete conversation content rather than a shell row.
/// 判断运行态是否携带具体会话内容，而不是仅有轻量 shell 行。
///
/// # Parameters
/// 参数
/// `runtime_state` is the session runtime state to inspect.
/// `runtime_state` 是待检查的会话运行态。
///
/// # Returns
/// 返回值
/// `true` when canonical messages or visible records are already loaded.
/// 当规范消息或可见记录已经加载时返回 `true`。
fn runtime_state_has_concrete_conversation_context(
    runtime_state: &SessionRuntimeStateEntry,
) -> bool {
    !runtime_state.conversation_state.messages.is_empty()
        || !runtime_state.conversation_state.records.is_empty()
}

/// Ensures one WebUI session has an in-memory runtime-state document after SQLite runtime storage was removed.
/// 在 SQLite 运行态存储移除后，确保单个 WebUI 会话拥有内存运行态文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot that will be repaired in memory.
/// `workspace_snapshot` 是将被内存修复的 WebUI 保留工作区快照。
/// `session_id` selects the session that must be runnable.
/// `session_id` 选择必须可运行的会话。
/// `active_precheck_turn_id` optionally restores the turn id carried by the already-created precheck request.
/// `active_precheck_turn_id` 可选恢复已创建 precheck 请求携带的回合标识。
fn ensure_webui_session_runtime_state(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    active_precheck_turn_id: Option<&str>,
) {
    let now = current_unix_millis_text();
    let projected_messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .cloned();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    if !runtime_state_has_concrete_conversation_context(runtime_state)
        && let Some(messages) = projected_messages.as_deref()
        && !messages.is_empty()
    {
        runtime_state.conversation_state = conversation_state_from_session_messages(messages);
        runtime_state.updated_at = now.clone();
    }
    if let Some(turn_id) = active_precheck_turn_id
        .map(str::trim)
        .filter(|turn_id| !turn_id.is_empty())
        && runtime_state
            .active_precheck_turn_id
            .as_deref()
            .is_none_or(|current| current.trim().is_empty())
    {
        runtime_state.active_precheck_turn_id = Some(turn_id.to_string());
        runtime_state.updated_at = now.clone();
    }
    if runtime_state_has_concrete_conversation_context(runtime_state) {
        workspace_snapshot.session_messages.insert(
            session_id,
            conversation_state_to_session_messages(&runtime_state.conversation_state),
        );
    } else {
        workspace_snapshot
            .session_messages
            .entry(session_id)
            .or_default();
    }
    workspace_snapshot.hydrated_session_ids.insert(session_id);
}

/// Returns whether a submitted prompt must enter the visible queue instead of starting immediately.
/// 判断提交的提示词是否必须进入可见队列而不是立即启动。
fn session_has_blocking_system_prompt_work(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
    has_active_run: bool,
    _has_running_subagents: bool,
) -> bool {
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return has_active_run;
    };
    let dispatch_context = webui_pending_prompt_dispatch_context(
        workspace_snapshot,
        provider_configuration,
        session_id,
        has_active_run,
        false,
    );
    let has_precheck_progress = webui_pending_prompt_has_precheck_progress(runtime_state);
    pending_prompt_lane_is_busy(
        &runtime_state.ui_state,
        PendingPromptBusyLaneContext {
            dispatch: dispatch_context,
            has_precheck_progress,
        },
    )
}

/// Returns whether WebUI has a concrete precheck turn in progress.
/// 返回 WebUI 当前是否存在具体的 precheck 回合正在进行。
fn webui_pending_prompt_has_precheck_progress(runtime_state: &SessionRuntimeStateEntry) -> bool {
    runtime_state
        .active_precheck_turn_id
        .as_deref()
        .is_some_and(|turn_id| !turn_id.trim().is_empty())
}

/// Builds the shared pending-prompt dispatch context from WebUI-owned runtime facts.
/// 从 WebUI 持有的运行态事实构建共享待发送提示词分发上下文。
fn webui_pending_prompt_dispatch_context(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
    has_active_run: bool,
    has_running_subagents: bool,
) -> PendingPromptDispatchContext {
    PendingPromptDispatchContext {
        uses_anthropic_runtime: session_uses_anthropic_runtime(
            workspace_snapshot,
            provider_configuration,
            session_id,
        ),
        has_running_subagents,
        has_active_run,
        run_phase: workspace_snapshot
            .session_run_phases
            .get(&session_id)
            .copied(),
        has_active_compaction: false,
    }
}

/// Returns whether cached WebUI state still reports a running child subagent.
/// 返回 WebUI 缓存状态是否仍报告存在运行中的子代理。
#[cfg(test)]
fn session_has_running_subagent_tasks_from_cache(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    !running_webui_subagent_task_states_for_tree_from_cache(guard, workspace_snapshot, session_id)
        .is_empty()
}

/// Gate state used to decide whether a completed model request closes the logical user turn.
/// 用于判断一次模型请求完成是否闭合逻辑用户回合的闸门状态。
#[derive(Debug, Clone, Copy)]
struct WebuiTurnCompletionGate {
    /// Number of still-running descendant subagent tasks under this parent session.
    /// 该父会话下仍在运行的后代子代理任务数量。
    pending_subagent_count: usize,
    /// Whether the shared hook runtime still has the original user turn open.
    /// 共享 hook 运行时是否仍保留原始用户回合。
    has_active_hook_turn: bool,
}

impl WebuiTurnCompletionGate {
    /// Returns whether the logical user turn still has unfinished subagent work.
    /// 返回逻辑用户回合是否仍有未完成的子代理工作。
    fn has_pending_work(self) -> bool {
        self.pending_subagent_count > 0
    }

    /// Converts the gate into the hook payload completion status.
    /// 将闸门状态转换为 hook 载荷完成状态。
    fn completion_status(self) -> ConversationTurnCompletionStatus {
        if self.has_pending_work() {
            ConversationTurnCompletionStatus::AwaitingSubagents
        } else {
            ConversationTurnCompletionStatus::Completed
        }
    }
}

/// Reads the shared turn-completion gate without mutating the transcript application path.
/// 在不改变转录应用路径的情况下读取共享回合完成闸门。
fn webui_turn_completion_gate_from_state(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<WebuiTurnCompletionGate, String> {
    let pending_subagent_count = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, session_id).len()
    };
    let has_active_hook_turn = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map(|hook_runtime| hook_runtime.has_active_turn(session_id))
        .unwrap_or(false);
    Ok(WebuiTurnCompletionGate {
        pending_subagent_count,
        has_active_hook_turn,
    })
}

/// Returns running child task states from the in-memory cache without segmented-store refresh.
/// 不刷新分段存储，仅从内存缓存返回仍在运行的子任务状态。
fn running_webui_subagent_task_states_for_tree_from_cache(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    root_parent_session_id: SessionId,
) -> Vec<(SessionId, SubagentTaskPanelState)> {
    collect_running_subagent_task_states_for_tree(root_parent_session_id, |parent_session_id| {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .map(|tasks| tasks.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|state| {
                normalize_webui_subagent_task_state(
                    Some(workspace_snapshot),
                    &state,
                    task_runtime,
                    live_listener_task_ids.contains(&state.task_id),
                )
            })
            .collect::<Vec<_>>()
    })
}

/// Returns running child task states from live runtime handles without cloning the workspace snapshot.
/// 不克隆工作区快照，仅通过实时运行句柄返回仍在运行的子任务状态。
fn running_webui_subagent_task_states_for_tree_from_live_cache(
    guard: &mut ServiceBridgeState,
    root_parent_session_id: SessionId,
) -> Vec<(SessionId, SubagentTaskPanelState)> {
    collect_running_subagent_task_states_for_tree(root_parent_session_id, |parent_session_id| {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .map(|tasks| tasks.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|state| {
                normalize_webui_subagent_task_state(
                    None,
                    &state,
                    task_runtime,
                    live_listener_task_ids.contains(&state.task_id),
                )
            })
            .collect::<Vec<_>>()
    })
}

/// Marks the provided running WebUI subagent tasks as stopped and clears their live listener routes.
/// 将给定的 WebUI 运行中子代理标记为已停止，并清理它们的实时监听路由。
fn mark_webui_subagent_tasks_stopped_in_state(
    guard: &mut ServiceBridgeState,
    task_ids: &[(SessionId, SessionId)],
) {
    for (direct_parent_session_id, task_id) in task_ids {
        if let Some(tasks) = guard
            .session_subagent_tasks
            .get_mut(direct_parent_session_id)
            && let Some(state) = tasks.get_mut(task_id)
        {
            state.run_state = SubagentTaskRunState::Stopped;
            if state.result_text.trim().is_empty() {
                state.result_text = SUBAGENT_TASK_STOPPED_SUMMARY_TEXT.to_string();
            }
        }
        guard.detached_subagent_task_listeners.remove(task_id);
        clear_webui_subagent_parent_turn_binding(guard, *task_id);
        guard
            .session_subagent_navigation_rounds
            .remove(direct_parent_session_id);
        if guard.active_subagent_panel.is_some_and(|selection| {
            selection.parent_session_id == *direct_parent_session_id
                && selection.task_id == *task_id
        }) {
            guard.active_subagent_panel = None;
        }
    }
}

/// Stops WebUI-visible session work and restores queue state to the iced stopped semantics.
/// 停止 WebUI 可见会话任务，并按 iced 停止语义恢复队列状态。
fn stop_webui_session_work(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    had_active_kernel_turn: bool,
    had_running_subagents: bool,
) -> Result<bool, String> {
    let now = current_unix_millis_text();
    let had_pending_precheck = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .is_some_and(|phase| *phase == SessionRunPhase::Preparing);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let had_pending_precheck =
        had_pending_precheck || runtime_state.active_precheck_turn_id.is_some();
    let cleared_system_queue =
        clear_internal_pending_prompt_queue_and_restore_panel(&mut runtime_state.ui_state);
    if !had_active_kernel_turn
        && !had_running_subagents
        && !had_pending_precheck
        && !cleared_system_queue
    {
        return Ok(false);
    }
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now.clone();
    let stop_body = if had_active_kernel_turn {
        "已按用户请求停止当前 AI 调用、运行中的子代理以及系统内部等待任务；尚未真正执行的排队消息已恢复到可再次插入状态。"
    } else {
        "已按用户请求停止当前会话的子代理与系统内部等待任务；尚未真正执行的排队消息已恢复到可再次插入状态。"
    };
    append_webui_session_message_entry_to_state(
        runtime_state,
        session_id,
        SessionMessageEntry::new(SessionMessageKind::Assistant, "执行已停止", stop_body),
    );
    let provider_request_count = runtime_state
        .last_safe_provider_request_count
        .filter(|count| *count > 0);
    let mut terminated_entry = SessionMessageEntry::hidden(
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::TurnTerminated,
        "Turn Terminated",
        "The run was stopped manually before a summary could be produced.",
    );
    terminated_entry.metadata = Some(json!({ "preview": "" }));
    let terminated_message_id =
        append_webui_session_message_entry_to_state(runtime_state, session_id, terminated_entry);
    let _ = append_webui_session_turn_stats_to_state(
        runtime_state,
        session_id,
        &terminated_message_id,
        SessionTurnStatsStatus::Stopped,
        provider_request_count,
    );
    let projected_messages =
        conversation_state_to_session_messages(&runtime_state.conversation_state);
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已按用户请求停止当前会话运行。");
        session.last_updated_label = String::from("刚刚");
    }
    workspace_snapshot
        .session_messages
        .insert(session_id, projected_messages);
    workspace_snapshot.hydrated_session_ids.insert(session_id);
    Ok(true)
}

/// Applies the immediate stopped gate before slower stop projection and persistence finish.
/// 在较慢的停止投影与持久化完成前先应用即时停止门禁。
fn mark_webui_session_stop_requested_in_workspace(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    clear_internal_pending_prompt_queue_and_restore_panel(&mut runtime_state.ui_state);
    runtime_state.active_precheck_turn_id = None;
    runtime_state.updated_at = now;
}

/// Appends one canonical WebUI session message record and returns its materialized id.
/// 追加一条 WebUI 规范会话消息记录，并返回物化后的消息标识。
fn append_webui_session_message_entry_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    entry: SessionMessageEntry,
) -> String {
    let created_at = current_unix_millis_text();
    let conversation_record_count = runtime_state.conversation_state.records.len();
    let message_id =
        build_session_record_message_id(session_id, &created_at, conversation_record_count);
    let materialized_entry = entry.with_runtime_metadata(message_id.clone(), created_at);
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: materialized_entry.id.clone(),
            kind: materialized_entry.kind,
            record_kind: materialized_entry.record_kind,
            visible: materialized_entry.visible,
            title: materialized_entry.title.clone(),
            body: materialized_entry.body.clone(),
            metadata: materialized_entry.metadata.clone(),
            created_at: materialized_entry.created_at.clone(),
        },
    );
    runtime_state.updated_at = materialized_entry.created_at;
    message_id
}

/// Appends one canonical WebUI turn-statistics record using the shared iced/WebUI turn stats builder.
/// 使用 iced/WebUI 共享的轮次统计构建器追加一条 WebUI 规范轮次统计记录。
fn append_webui_session_turn_stats_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
) -> Option<String> {
    let turn_stats = build_session_turn_stats_from_state(
        &runtime_state.conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
    )?;
    let mut entry = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        format!("Turn Stats · {}", turn_stats.status.label()),
        turn_stats.detail_body(),
    );
    entry.record_kind = SessionMessageRecordKind::TurnStats;
    entry.metadata = serde_json::to_value(json!({
        "turn_stats": turn_stats,
        "turn_closing_record_id": closing_record_id,
    }))
    .ok();
    Some(append_webui_session_message_entry_to_state(
        runtime_state,
        session_id,
        entry,
    ))
}

/// Marks one idle WebUI session as waiting for an already guided internal prompt.
/// 将一个空闲 WebUI 会话标记为正在等待已引导的内部提示词。
fn mark_webui_session_waiting_for_guided_prompt_dispatch(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    if workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id && session.status == SessionState::Running)
    {
        return;
    }
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.updated_at = now.clone();
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已有消息接入主会话，正在等待当前系统任务处理完成后继续。");
        session.last_updated_label = String::from("刚刚");
    }
    workspace_snapshot.session_messages.insert(
        session_id,
        conversation_state_to_session_messages(&runtime_state.conversation_state),
    );
    workspace_snapshot.hydrated_session_ids.insert(session_id);
}

/// Builds one WebUI-originated visible pending prompt entry.
/// 构建一条来自 WebUI 的可见待发送提示词条目。
fn build_webui_pending_prompt_panel_entry(
    prompt: String,
    attachments: Vec<ConversationAttachment>,
    model_reference: Option<PendingPromptModelReference>,
    runtime_options: PendingPromptRuntimeOptions,
) -> PendingPromptPanelEntry {
    let created_at = current_unix_millis_text();
    PendingPromptPanelEntry {
        id: format!("webui-pending-panel-{}", SessionId::new_v4()),
        preview: pending_prompt_preview_text(&prompt, &attachments),
        prompt,
        attachments,
        model_reference,
        runtime_options,
        state: PendingPromptPanelEntryState::Ready,
        created_at,
    }
}

/// Appends one visible pending prompt entry to the selected session queue and mirrors the queued user event.
/// 将一条可见待发送提示词追加到选中会话队列，并镜像已排队用户事件。
fn enqueue_webui_pending_prompt_panel_entry(
    workspace_snapshot: &mut WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    entry: PendingPromptPanelEntry,
) -> Result<(), String> {
    let now = current_unix_millis_text();
    let project_id = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone());
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    if runtime_state.ui_state.pending_prompt_panel_queue.len() >= PENDING_PROMPT_QUEUE_LIMIT {
        return Err(String::from(
            "待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。",
        ));
    }
    let entry_id = entry.id.clone();
    let prompt = entry.prompt.clone();
    runtime_state
        .ui_state
        .pending_prompt_panel_queue
        .push(entry);
    runtime_state.updated_at = now;
    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
        hook_runtime.record_queued_user_prompt(
            session_id,
            &entry_id,
            project_id,
            workspace_path.to_string(),
            PendingSessionPromptSource::UserFollowup,
            &prompt,
        );
    }
    Ok(())
}

/// Inserts one visible WebUI queue row into the internal session prompt queue.
/// 将一条 WebUI 可见队列行插入内部会话提示词队列。
fn insert_webui_pending_prompt_into_internal_queue(
    workspace_snapshot: &mut WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    workspace_path: &str,
    entry_id: &str,
) -> Result<(), String> {
    let project_id = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone());
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| String::from("当前会话没有可引导的待发送队列。"))?;
    let created_at = current_unix_millis_text();
    let internal_entry_id = format!("webui-pending-internal-{}", SessionId::new_v4());
    let route_outcome = route_pending_prompt_panel_entry_into_internal_queue(
        &mut runtime_state.ui_state,
        entry_id,
        internal_entry_id,
        created_at,
    )
    .map_err(|error| match error {
        PendingPromptQueueRouteError::MissingPanelEntry => {
            String::from("没有找到待引导的待发送提示词。")
        }
        PendingPromptQueueRouteError::InternalQueueFull => {
            String::from("待发送消息队列已满，最多保留 10 条，请等待前面的消息完成后再继续追加。")
        }
    })?;
    runtime_state.updated_at = current_unix_millis_text();
    let PendingPromptQueueRouteOutcome::Inserted(internal_entry) = route_outcome else {
        return Ok(());
    };
    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
        hook_runtime.record_queued_user_prompt(
            session_id,
            entry_id,
            project_id,
            workspace_path.to_string(),
            PendingSessionPromptSource::UserFollowup,
            &internal_entry.prompt,
        );
    }
    Ok(())
}

/// Returns whether one session should use Anthropic-specific queue dispatch ordering.
/// 返回单个会话是否应使用 Anthropic 专属队列分发顺序。
fn session_uses_anthropic_runtime(
    workspace_snapshot: &WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
) -> bool {
    if let Some(active_provider_id) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_provider_id.as_deref())
    {
        return provider_config::supported_provider_by_id(active_provider_id)
            .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic);
    }
    provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .and_then(|(provider_id, _)| provider_config::supported_provider_by_id(provider_id))
        .is_some_and(|definition| definition.runtime == ProviderRuntimeKind::Anthropic)
}

/// Returns the workspace path associated with one session.
/// 返回单个会话关联的工作区路径。
fn workspace_path_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> String {
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.as_deref())
        .and_then(|project_id| {
            workspace_snapshot
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
        })
        .map(|project| project.path.clone())
        .unwrap_or_else(|| workspace_root().display().to_string())
}

/// Builds one compact single-line preview used by the pending prompt queue strip.
/// 构建待发送提示词队列条使用的单行紧凑预览。
fn pending_prompt_preview_text(prompt: &str, attachments: &[ConversationAttachment]) -> String {
    let trimmed_prompt = prompt.trim();
    let attachment_prefix = if attachments.is_empty() {
        String::new()
    } else {
        format!("{} 个附件 · ", attachments.len())
    };
    let text = if trimmed_prompt.is_empty() {
        String::from("附件消息")
    } else {
        trimmed_prompt
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let preview = format!("{attachment_prefix}{text}");
    if preview.chars().count() <= 48 {
        preview
    } else {
        format!("{}...", preview.chars().take(48).collect::<String>())
    }
}

/// Captures the active provider/model identity for one queued WebUI prompt.
/// 为单条 WebUI 排队提示词捕获当前激活的供应商与模型身份。
fn current_pending_prompt_model_reference(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<PendingPromptModelReference> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    Some(PendingPromptModelReference {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        provider_label: provider_config::provider_summary(provider_id),
        model_name: provider_config::model_summary(provider_id, model_id, provider_configuration),
        openrouter_endpoint_route: (provider_id == "openrouter")
            .then(|| {
                provider_config::openrouter_endpoint_route_for_model(
                    provider_configuration,
                    model_id,
                )
            })
            .flatten(),
    })
}

/// Captures runtime controls for one queued WebUI prompt from the shared service router input.
/// 从共享服务路由输入捕获单条 WebUI 排队提示词的运行控制参数。
fn pending_prompt_runtime_options_from_submission(
    agent_runtime: &AgentRuntimeBridge,
    reasoning_effort: Option<&str>,
    disabled_ai_capability_kinds: Option<&[String]>,
) -> Result<PendingPromptRuntimeOptions, String> {
    Ok(PendingPromptRuntimeOptions {
        reasoning_effort: reasoning_effort
            .and_then(parse_reasoning_effort)
            .or_else(|| active_reasoning_selection_effort(&agent_runtime.provider_configuration))
            .unwrap_or(ReasoningEffortLevel::Medium),
        temperature: None,
        web_search_override: Default::default(),
        web_search_provider_id: None,
        disabled_ai_capability_kinds: effective_workbench_disabled_ai_capability_kinds(
            &agent_runtime.ai_tool_configuration,
            disabled_ai_capability_kinds,
        )?,
    })
}

/// Clones provider configuration and overrides only the queued prompt's locked model identity.
/// 克隆 Provider 配置，并仅覆盖排队提示词锁定的模型身份。
fn provider_configuration_for_model_reference(
    provider_configuration: &ProviderConfigurationFile,
    model_reference: Option<&PendingPromptModelReference>,
) -> ProviderConfigurationFile {
    let mut next_configuration = provider_configuration.clone();
    if let Some(model_reference) = model_reference {
        next_configuration.model = Some(format!(
            "{}/{}",
            model_reference.provider_id, model_reference.model_id
        ));
        if model_reference.provider_id == "openrouter" {
            provider_config::set_openrouter_endpoint_route_for_model(
                &mut next_configuration,
                &model_reference.model_id,
                model_reference.openrouter_endpoint_route.clone(),
            );
        }
    }
    next_configuration
}

/// Clones the runtime bridge with the queued prompt's locked provider configuration.
/// 使用排队提示词锁定的 Provider 配置克隆运行时桥接。
fn agent_runtime_for_pending_prompt(
    agent_runtime: &AgentRuntimeBridge,
    prompt_entry: &PendingSessionPromptEntry,
) -> AgentRuntimeBridge {
    let mut next_runtime = agent_runtime.clone();
    next_runtime.provider_configuration = provider_configuration_for_model_reference(
        &agent_runtime.provider_configuration,
        prompt_entry.model_reference.as_ref(),
    );
    next_runtime
}

/// Returns the owning project id for one session when it has a persisted project scope.
/// 当会话拥有持久化项目作用域时返回所属项目标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the session summaries.
/// `workspace_snapshot` 包含会话摘要。
/// `session_id` selects the session.
/// `session_id` 选择目标会话。
///
/// # Returns
/// 返回值
/// A non-empty project id when the session belongs to one project.
/// 当会话属于某个项目时返回非空项目标识。
fn owning_project_id_for_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<String> {
    workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone())
        .filter(|project_id| !project_id.trim().is_empty())
}

/// Captures the iced-equivalent context snapshot before a WebUI user prompt mutates context.
/// 在 WebUI 用户提示词改变上下文前捕获与 iced 等价的上下文快照。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the current runtime state.
/// `workspace_snapshot` 包含当前运行态。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `_source` is retained only for temporary call-site stability.
/// `_source` 仅为临时保持调用点稳定而保留。
///
/// # Returns
/// 返回值
/// `Ok(())` after the snapshot either succeeds or is not needed.
/// 快照成功或无需快照后返回 `Ok(())`。
fn snapshot_webui_context_before_user_prompt(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    _source: PendingSessionPromptSource,
) -> Result<(), String> {
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return Ok(());
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return Ok(());
    };
    vulcan_workspace_store::workspace_store::snapshot_segmented_session_context_before_user_prompt(
        &controller_runtime.data_root,
        &project_id,
        session_id,
        runtime_state,
    )
}

/// Applies iced-equivalent outer steps after one WebUI prompt begin reducer has run.
/// 在 WebUI 提示词 begin reducer 执行后应用与 iced 等价的外层步骤。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the mutable WebUI workspace snapshot.
/// `workspace_snapshot` 是可变 WebUI 工作区快照。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
fn apply_webui_prompt_begin_outer_steps(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        && remove_legacy_vmm_profile_context_injection_state(
            &mut runtime_state.context_injection_state,
        )
    {
        runtime_state.updated_at = current_unix_millis_text();
    }
    persist_webui_context_current_document(
        controller_runtime,
        workspace_snapshot,
        session_id,
        session_title,
    );
}

/// Persists the iced-equivalent current context document for a WebUI session when possible.
/// 在可行时为 WebUI 会话持久化与 iced 等价的当前上下文文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the current runtime state.
/// `workspace_snapshot` 包含当前运行态。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
fn persist_webui_context_current_document(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return;
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return;
    };
    if let Err(error) =
        vulcan_workspace_store::workspace_store::persist_segmented_session_context_current_document(
            &controller_runtime.data_root,
            &project_id,
            session_id,
            runtime_state,
        )
    {
        eprintln!("[tauri-service] current context persist failed for `{session_title}`: {error}");
    }
}

/// Resolves or creates the session that should receive the submitted prompt.
/// 解析或创建应该接收本次提示词的会话。
fn materialize_prompt_session(
    workspace_snapshot: &mut WorkspaceSnapshot,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    prompt: &str,
) -> Result<(SessionId, String, String), String> {
    if let Some(session_id) = conversation_id.and_then(parse_session_id)
        && let Some(session) = workspace_snapshot
            .sessions
            .iter_mut()
            .find(|candidate| candidate.id == session_id)
    {
        session.status = SessionState::Running;
        session.last_updated_label = String::from("刚刚");
        workspace_snapshot.selected_session = session_id;
        let workspace_path = session
            .project_id
            .as_deref()
            .and_then(|owner_id| {
                workspace_snapshot
                    .task_projects
                    .iter()
                    .find(|project| project.id == owner_id)
            })
            .map(|project| project.path.clone())
            .unwrap_or_default();
        return Ok((session_id, session.title.clone(), workspace_path));
    }

    let project_id = project_id.ok_or_else(|| String::from("新建会话需要提供项目标识。"))?;
    let project = workspace_snapshot
        .task_projects
        .iter_mut()
        .find(|candidate| candidate.id == project_id)
        .ok_or_else(|| format!("未找到项目：{project_id}"))?;
    let session_id = SessionId::new_v4();
    let session_title = prompt_to_session_title(prompt);
    let session = SessionSummary::new(
        session_id,
        Some(project.id.clone()),
        session_title.clone(),
        "正在执行 WebUI 提交的任务",
        SessionState::Running,
        0,
        0,
        "刚刚",
    )
    .with_title_source(SessionTitleSource::Generated);

    project
        .session_ids
        .retain(|existing| *existing != session_id);
    project.session_ids.insert(0, session_id);
    let workspace_path = project.path.clone();
    workspace_snapshot.sessions.insert(0, session);
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::MemoryRecall);
    workspace_snapshot.selected_session = session_id;
    Ok((session_id, session_title, workspace_path))
}

/// Parses prompt-level AI capability target order overrides from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标顺序覆盖。
///
/// # Parameters
/// 参数
/// `raw_overrides` contains capability-keyed target selector arrays.
/// `raw_overrides` 包含按能力键分组的目标选择器数组。
///
/// # Returns
/// 返回值
/// A semantic capability keyed map accepted by the shared runtime router.
/// 共享运行时路由器可接受的语义能力映射。
fn parse_ai_capability_target_order_overrides(
    raw_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<BTreeMap<vulcan_provider_config::config::AiToolKind, Vec<String>>, String> {
    let mut overrides = BTreeMap::new();
    for (raw_kind, selectors) in raw_overrides.unwrap_or_default() {
        let Some(kind) =
            vulcan_agent_runtime::workbench_ai_capability_router::parse_workbench_ai_tool_kind(
                &raw_kind,
            )
        else {
            return Err(format!("未知的扩展能力排序类型：{raw_kind}"));
        };
        let cleaned_selectors = selectors
            .into_iter()
            .map(|selector| selector.trim().to_string())
            .filter(|selector| !selector.is_empty())
            .collect::<Vec<_>>();
        if !cleaned_selectors.is_empty() {
            overrides.insert(kind, cleaned_selectors);
        }
    }
    Ok(overrides)
}

/// Parses prompt-level AI capability target selectors from the WebUI payload.
/// 从 WebUI 载荷中解析提示词级 AI 能力目标选择器。
///
/// # Parameters
/// 参数
/// `raw_selectors` contains optional target selector strings.
/// `raw_selectors` 包含可选的目标选择器字符串。
///
/// # Returns
/// 返回值
/// A trimmed selector set accepted by the shared prompt-turn core.
/// 共享提示词回合核心可接受的已清洗选择器集合。
fn parse_ai_capability_target_selectors(raw_selectors: Option<Vec<String>>) -> BTreeSet<String> {
    raw_selectors
        .unwrap_or_default()
        .into_iter()
        .map(|selector| selector.trim().to_string())
        .filter(|selector| !selector.is_empty())
        .collect()
}

/// Loads project-local Vulcan Code configuration for one WebUI execution workspace.
/// 为一个 WebUI 执行工作区加载项目级 Vulcan Code 配置。
///
/// # Parameters
/// 参数
/// `workspace_path` is the absolute project workspace path.
/// `workspace_path` 是项目工作区绝对路径。
///
/// # Returns
/// 返回值
/// The project-local configuration, or the default configuration when loading fails.
/// 项目级配置；加载失败时返回默认配置。
fn load_project_configuration(
    workspace_path: &str,
) -> vulcan_provider_config::config::ProjectVulcanCodeConfigurationFile {
    vulcan_provider_config::config::load_project_configuration_file(&PathBuf::from(workspace_path))
        .unwrap_or_default()
}

/// Builds the shared precheck plan for a WebUI session using the iced VMM layering rules.
/// 使用 iced 的 VMM 分层规则为一个 WebUI 会话构建共享 precheck 计划。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns provider configuration and hook runtime state.
/// `agent_runtime` 持有 Provider 配置与 hook 运行态。
/// `workspace_snapshot` contains session-scoped VMM overrides.
/// `workspace_snapshot` 包含会话级 VMM 覆盖配置。
/// `session_id` selects the session being prepared.
/// `session_id` 选择正在准备的会话。
/// `workspace_path` identifies the project configuration file to load.
/// `workspace_path` 标识需要加载的项目配置文件。
///
/// # Returns
/// 返回值
/// A shared precheck plan matching the iced shell.
/// 返回与 iced 外壳一致的共享 precheck 计划。
fn webui_shared_prompt_precheck_plan(
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
) -> Result<SharedPromptPrecheckPlan, String> {
    let project_configuration_started_at = current_unix_millis_value();
    let project_configuration = load_project_configuration(workspace_path);
    webui_trace(
        "precheck_plan_project_config_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(project_configuration_started_at)
        ),
    );
    let session_layer = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| &runtime_state.vmm);
    let effective_vmm_config = vulcan_vmm::VmmEffectiveConfig::resolve(
        &agent_runtime.provider_configuration.vmm,
        Some(&project_configuration.vmm),
        session_layer,
    );
    let hook_snapshot_started_at = current_unix_millis_value();
    let precheck_hook_snapshots = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?
        .precheck_hook_snapshots();
    webui_trace(
        "precheck_plan_hook_snapshots_ready",
        format!(
            "session={session_id} hooks={} elapsed_ms={}",
            precheck_hook_snapshots.len(),
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    Ok(shared_prompt_precheck_plan(
        &effective_vmm_config,
        &precheck_hook_snapshots,
    ))
}

/// Builds the shared VMM hook snapshot from the current Tauri workspace state.
/// 根据当前 Tauri 工作区状态构建共享 VMM hook 快照。
fn build_vmm_hook_config_snapshot(
    provider_configuration: &ProviderConfigurationFile,
    workspace_snapshot: &WorkspaceSnapshot,
) -> VmmHookConfigSnapshot {
    let snapshot_started_at = current_unix_millis_value();
    let mut projects_by_workspace = std::collections::BTreeMap::new();
    let mut project_count = 0usize;
    for project in workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
    {
        project_count += 1;
        let project_configuration = load_project_configuration(&project.path);
        if !project_configuration.vmm.is_empty() {
            projects_by_workspace.insert(project.path.clone(), project_configuration.vmm);
        }
    }

    let sessions = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm.is_empty())
                .then(|| (session_id.to_string(), runtime_state.vmm.clone()))
        })
        .collect();
    let outboxes = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm_writeback_outbox.is_empty()).then(|| {
                (
                    session_id.to_string(),
                    runtime_state.vmm_writeback_outbox.clone(),
                )
            })
        })
        .collect();
    let profile_sync = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .filter_map(|(session_id, runtime_state)| {
            (!runtime_state.vmm_profile_sync.is_empty()).then(|| {
                (
                    session_id.to_string(),
                    runtime_state.vmm_profile_sync.clone(),
                )
            })
        })
        .collect();
    let last_compaction_ids = workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
        .map(|(session_id, runtime_state)| {
            (
                session_id.to_string(),
                runtime_state.last_compaction_id.clone(),
            )
        })
        .collect();
    let snapshot = VmmHookConfigSnapshot {
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

/// Persists only the changed session after a WebUI runtime mutation.
/// 在 WebUI 运行时变更后仅持久化被修改的单个会话。
async fn persist_workspace_session_update(
    state: Option<&SharedServiceBridgeState>,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(state) = state {
        invalidate_webui_session_persist_version(state, session_id)?;
    }
    let persist_lock = webui_workspace_persist_lock();
    let _persist_guard = persist_lock.lock().await;
    persist_workspace_session_update_unlocked(controller_runtime, workspace_snapshot, session_id)
        .await
}

/// Persists one session snapshot after the caller has acquired the shared WebUI write gate.
/// 在调用方已取得共享 WebUI 写入闸门后持久化单个会话快照。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the workspace snapshot that contains the changed session.
/// `workspace_snapshot` 是包含变更会话的工作区快照。
/// `session_id` selects the session to persist.
/// `session_id` 选择要持久化的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` when the session has been saved.
/// 会话保存完成时返回 `Ok(())`。
async fn persist_workspace_session_update_unlocked(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    let (session, archived) = workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
        .map(|session| (session, false))
        .or_else(|| {
            workspace_snapshot
                .archived_sessions
                .iter()
                .find(|candidate| candidate.id == session_id)
                .map(|session| (session, true))
        })
        .ok_or_else(|| format!("单会话保存失败：未找到会话：{session_id}"))?;
    let session_messages = if archived {
        &workspace_snapshot.archived_session_messages
    } else {
        &workspace_snapshot.session_messages
    };
    let session_compactions = if archived {
        &workspace_snapshot.archived_session_compactions
    } else {
        &workspace_snapshot.session_compactions
    };
    let session_runtime_state = if archived {
        &workspace_snapshot.archived_session_runtime_state
    } else {
        &workspace_snapshot.session_runtime_state
    };
    let runtime_state = session_runtime_state.get(&session_id);
    let rebuilt_messages;
    let messages = if let Some(runtime_state) = runtime_state {
        rebuilt_messages =
            conversation_state_to_session_messages(&runtime_state.conversation_state);
        rebuilt_messages.as_slice()
    } else if let Some(messages) = session_messages.get(&session_id) {
        messages.as_slice()
    } else {
        &[]
    };
    let compactions = session_compactions.get(&session_id).map(Vec::as_slice);
    let task_projects = workspace_snapshot
        .task_projects
        .iter()
        .cloned()
        .chain(workspace_snapshot.archived_task_projects.iter().cloned())
        .collect::<Vec<_>>();

    vulcan_workspace_store::workspace_store::persist_single_session_update(
        controller_runtime,
        &task_projects,
        session,
        archived,
        messages,
        compactions,
        runtime_state,
    )
    .await
    .map(|_| ())
}

/// Returns whether a WebUI snapshot owns the id as a regular workspace-store session.
/// 返回 WebUI 快照是否将该标识拥有为普通 workspace-store 会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot.
/// `workspace_snapshot` 是当前保留的 WebUI 工作区快照。
/// `session_id` is the candidate session identifier.
/// `session_id` 是候选会话标识。
///
/// # Returns
/// 返回值
/// `true` when the id belongs to a normal active or archived session, not a persisted TaskSession.
/// 当该标识属于普通活动或归档会话、而不是持久化 TaskSession 时返回 `true`。
fn workspace_snapshot_contains_regular_session(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_snapshot
        .sessions
        .iter()
        .chain(workspace_snapshot.archived_sessions.iter())
        .any(|session| session.id == session_id)
}

/// Persists one WebUI session snapshot in the background without blocking visible UI updates.
/// 在后台持久化一个 WebUI 会话快照，不阻塞可见 UI 更新。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the already-rendered snapshot to save.
/// `workspace_snapshot` 是已经渲染过、待保存的快照。
/// `session_id` selects the session that changed.
/// `session_id` 选择发生变更的会话。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
fn spawn_workspace_session_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[tauri-service] background persist version failed: {error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
            return;
        }
        if let Err(error) = persist_workspace_session_update_unlocked(
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            let reason = reason.unwrap_or_else(|| String::from("session_update"));
            eprintln!("[tauri-service] background persist failed ({reason}): {error}");
        }
    });
}

/// Persists only one WebUI session runtime-state document in the background.
/// 在后台仅持久化一个 WebUI 会话运行态文档。
///
/// # Parameters
/// 参数
/// `state` owns the retained service state and per-session write versions.
/// `state` 持有保留服务状态与按会话划分的写入版本。
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the already-rendered snapshot containing the runtime state.
/// `workspace_snapshot` 是已经渲染过且包含运行态的快照。
/// `session_id` selects the session runtime-state document to persist.
/// `session_id` 选择需要持久化的会话运行态文档。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
fn spawn_workspace_session_runtime_state_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[tauri-service] runtime-state persist version failed: {error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
            return;
        }
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .or_else(|| {
                workspace_snapshot
                    .archived_session_runtime_state
                    .get(&session_id)
                    .cloned()
            })
        else {
            return;
        };
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
                controller_runtime,
                session_id,
                &runtime_state,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("runtime_state");
            eprintln!(
                "[tauri-service] background runtime-state persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Allocates the next background persistence version for one WebUI session.
/// 为一个 WebUI 会话分配下一次后台持久化版本。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
///
/// # Returns
/// 返回值
/// The version assigned to the new background persistence task.
/// 返回分配给新后台持久化任务的版本。
fn next_webui_session_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let next_version = guard
        .session_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_persist_versions
        .insert(session_id, next_version);
    Ok(next_version)
}

/// Returns whether a background persistence task is still the latest one for its session.
/// 判断某个后台持久化任务是否仍是其会话的最新任务。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
/// `persist_version` is the task-local persistence version.
/// `persist_version` 是任务本地持久化版本。
///
/// # Returns
/// 返回值
/// `true` when the task may write to disk.
/// 当该任务可以写入磁盘时返回 `true`。
fn webui_session_persist_version_is_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_version: u64,
) -> bool {
    state
        .lock()
        .ok()
        .and_then(|guard| guard.session_persist_versions.get(&session_id).copied())
        == Some(persist_version)
}

/// Invalidates older background persistence snapshots for one WebUI session.
/// 使某个 WebUI 会话上较旧的后台持久化快照失效。
///
/// # Parameters
/// 参数
/// `state` owns the per-session persistence version table.
/// `state` 持有按会话归属的持久化版本表。
/// `session_id` selects the session whose pending background writes should be invalidated.
/// `session_id` 选择需要使待处理后台写入失效的会话。
fn invalidate_webui_session_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let _ = next_webui_session_persist_version(state, session_id)?;
    Ok(())
}

/// Persists one WebUI session runtime-state mutation through the full session snapshot path.
/// 通过完整会话快照路径持久化一个 WebUI 会话运行态变更。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `session_id` selects the runtime-state document to update.
/// `session_id` 选择要更新的运行态文档。
/// `workspace_snapshot` carries the current complete session snapshot.
/// `workspace_snapshot` 携带当前完整会话快照。
///
/// # Returns
/// 返回值
/// `Ok(())` after the store write finishes.
/// 存储写入完成后返回 `Ok(())`。
async fn persist_webui_session_runtime_state_update(
    state: &SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
    persist_workspace_session_update(
        Some(state),
        controller_runtime,
        workspace_snapshot,
        session_id,
    )
    .await
}

/// Builds an empty LuaSkills catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 LuaSkills 目录。
fn empty_luaskills_catalog() -> TauriLuaSkillsCatalog {
    TauriLuaSkillsCatalog {
        runtime_root: PathBuf::new(),
        skills: Vec::new(),
        entry_count: 0,
    }
}

/// Builds an empty MCP catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 MCP 目录。
fn empty_mcp_catalog() -> TauriMcpRuntimeCatalog {
    TauriMcpRuntimeCatalog {
        configured_server_count: 0,
        enabled_server_count: 0,
        connected_server_count: 0,
        tool_count: 0,
        prompt_count: 0,
        resource_count: 0,
        summary_rows: Vec::new(),
    }
}

/// Returns the cached LuaSkills catalog without touching the LuaSkills engine on hot paths.
/// 在热路径上返回缓存的 LuaSkills 目录，避免触碰 LuaSkills 引擎。
fn cached_luaskills_catalog_from_state(state: &ServiceBridgeState) -> TauriLuaSkillsCatalog {
    state
        .cached_luaskills_catalog
        .clone()
        .unwrap_or_else(empty_luaskills_catalog)
}

/// Returns the cached MCP catalog without querying the MCP runtime on hot paths.
/// 在热路径上返回缓存的 MCP 目录，避免查询 MCP 运行时。
fn cached_mcp_catalog_from_state(state: &ServiceBridgeState) -> TauriMcpRuntimeCatalog {
    state
        .cached_mcp_catalog
        .clone()
        .unwrap_or_else(empty_mcp_catalog)
}

/// Parses a user-facing optional session identifier and ignores local draft identifiers.
/// 解析用户界面传入的可选会话标识，并忽略本地草稿标识。
fn parse_session_id(value: &str) -> Option<SessionId> {
    value.parse::<SessionId>().ok()
}

/// Converts the composer reasoning string into the shared provider-config enum.
/// 将输入区思考档位字符串转换为共享 provider-config 枚举。
fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffortLevel> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(ReasoningEffortLevel::Off),
        "low" => Some(ReasoningEffortLevel::Low),
        "medium" => Some(ReasoningEffortLevel::Medium),
        "high" => Some(ReasoningEffortLevel::High),
        "x-high" | "max" => Some(ReasoningEffortLevel::XHigh),
        _ => None,
    }
}

/// Converts one shared reasoning-effort enum into the WebUI wire value.
/// 将单个共享思考档位枚举转换为 WebUI 传输值。
fn reasoning_effort_wire_value(effort: ReasoningEffortLevel) -> &'static str {
    match effort {
        ReasoningEffortLevel::Off => "off",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
    }
}

/// Resolves the effective reasoning effort for the active model using the shared selector view.
/// 使用共享选择器视图解析当前激活模型的生效思考档位。
fn active_reasoning_selection_effort(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningEffortLevel> {
    let preferred_effort = provider_configuration
        .workbench_reasoning_effort
        .unwrap_or(ReasoningEffortLevel::Medium);
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    Some(
        definition
            .reasoning_selection_view(active_model.as_ref(), preferred_effort)
            .selected_effort,
    )
}

/// Builds the WebUI reasoning selector snapshot from the shared provider configuration.
/// 从共享 Provider 配置构建 WebUI 思考选择器快照。
fn reasoning_selection(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ReasoningSelectionSummary> {
    let preferred_effort = provider_configuration
        .workbench_reasoning_effort
        .unwrap_or(ReasoningEffortLevel::Medium);
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    let view = definition.reasoning_selection_view(active_model.as_ref(), preferred_effort);
    Some(ReasoningSelectionSummary {
        options: view
            .options
            .into_iter()
            .map(|option| ReasoningSelectionOptionSummary {
                value: reasoning_effort_wire_value(option.effort).to_string(),
                label: option.label,
            })
            .collect(),
        selected_effort: reasoning_effort_wire_value(view.selected_effort).to_string(),
        note: view.note,
    })
}

/// Creates a readable generated session title from the submitted prompt.
/// 根据提交提示词创建可读的自动会话标题。
fn prompt_to_session_title(prompt: &str) -> String {
    let title = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("新会话");
    title.chars().take(48).collect()
}

/// Stores and emits one rebuilt workbench snapshot after a workspace mutation.
/// 在工作区发生变更后存储并发出一份重建后的工作台快照。
fn store_and_emit_workbench_snapshot(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, SharedServiceBridgeState>,
    controller_runtime: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace_snapshot: WorkspaceSnapshot,
    logs: &[String],
) -> Result<WorkbenchSnapshot, String> {
    store_and_emit_workbench_snapshot_with_subagent_navigation_mode(
        app,
        state.inner(),
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        workspace_snapshot,
        logs,
        SubagentNavigationRefreshMode::Rebuild,
    )
}

/// Stores and emits one rebuilt workbench snapshot while preserving current subagent navigation.
/// 存储并发出一份重建后的工作台快照，同时保留当前子代理导航。
fn store_and_emit_workbench_snapshot_preserving_subagent_navigation(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, SharedServiceBridgeState>,
    controller_runtime: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace_snapshot: WorkspaceSnapshot,
    logs: &[String],
) -> Result<WorkbenchSnapshot, String> {
    store_and_emit_workbench_snapshot_with_subagent_navigation_mode(
        app,
        state.inner(),
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        workspace_snapshot,
        logs,
        SubagentNavigationRefreshMode::Preserve,
    )
}

/// Stores and emits one rebuilt workbench snapshot with an explicit subagent navigation refresh mode.
/// 使用显式子代理导航刷新模式存储并发出一份重建后的工作台快照。
fn store_and_emit_workbench_snapshot_with_subagent_navigation_mode(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace_snapshot: WorkspaceSnapshot,
    logs: &[String],
    subagent_navigation_mode: SubagentNavigationRefreshMode,
) -> Result<WorkbenchSnapshot, String> {
    let trace_started_at = current_unix_millis_value();
    let trace_lock_wait_started_at = current_unix_millis_value();
    let (live_context_budgets, surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        webui_trace(
            "store_workbench_lock_wait",
            format!(
                "mode={subagent_navigation_mode:?} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(trace_lock_wait_started_at)
            ),
        );
        let trace_surface_started_at = current_unix_millis_value();
        let live_context_budgets = guard.live_context_budgets.clone();
        let (surface_messages, subagent_navigation) = match subagent_navigation_mode {
            SubagentNavigationRefreshMode::Rebuild => (
                active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
                subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            ),
            SubagentNavigationRefreshMode::Preserve => {
                let subagent_navigation =
                    subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot);
                let active_session_id = preserved_active_surface_session_id(
                    &workspace_snapshot,
                    subagent_navigation.as_ref(),
                );
                (
                    surface_messages_for_session_without_subagent_navigation_refresh(
                        &guard,
                        &workspace_snapshot,
                        active_session_id,
                    ),
                    subagent_navigation,
                )
            }
        };
        webui_trace(
            "store_workbench_surface_ready",
            format!(
                "mode={subagent_navigation_mode:?} messages={} has_subagent_navigation={} elapsed_ms={}",
                surface_messages.len(),
                subagent_navigation.is_some(),
                current_unix_millis_value().saturating_sub(trace_surface_started_at)
            ),
        );
        (live_context_budgets, surface_messages, subagent_navigation)
    };
    let trace_workbench_started_at = current_unix_millis_value();
    let workbench = workbench_from_runtime(
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        logs,
        &live_context_budgets,
    );
    webui_trace(
        "store_workbench_built",
        format!(
            "mode={subagent_navigation_mode:?} conversations={} messages={} elapsed_ms={}",
            workbench.conversations.len(),
            workbench.messages.len(),
            current_unix_millis_value().saturating_sub(trace_workbench_started_at)
        ),
    );
    let trace_commit_started_at = current_unix_millis_value();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    webui_trace(
        "store_workbench_committed",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_commit_started_at)
        ),
    );
    let trace_emit_started_at = current_unix_millis_value();
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    webui_trace(
        "store_workbench_emitted",
        format!(
            "mode={subagent_navigation_mode:?} elapsed_ms={} total_elapsed_ms={}",
            current_unix_millis_value().saturating_sub(trace_emit_started_at),
            current_unix_millis_value().saturating_sub(trace_started_at)
        ),
    );
    Ok(workbench)
}

/// Continues one WebUI prompt turn after the shared conversation precheck completes.
/// 在共享会话预检查完成后继续一个 WebUI 提示词回合。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained WebUI workspace snapshot.
/// `state` 持有 WebUI 保留的工作区快照。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` provides provider and hook runtime state.
/// `agent_runtime` 提供 Provider 与 hook 运行态。
/// `start` is the first-stage prompt turn prepared before precheck.
/// `start` 是 precheck 前准备好的第一阶段提示词回合。
/// `precheck_outcome` is the completed hook result.
/// `precheck_outcome` 是已完成的 hook 结果。
///
/// # Returns
/// 返回值
/// The execution request when the turn should continue, otherwise `None`.
/// 需要继续执行时返回执行请求，否则返回 `None`。
fn continue_webui_prompt_turn_after_precheck_from_state(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_outcome: ConversationPrecheckOutcome,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    let session_id = start.session_id;
    let continue_started_at = current_unix_millis_value();
    webui_trace(
        "precheck_continue_entered",
        format!("session={session_id} source={:?}", start.source),
    );
    let runs_user_prompt_precheck = start.source.runs_user_prompt_precheck();
    let snapshot_lock_started_at = current_unix_millis_value();
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    webui_trace(
        "precheck_continue_snapshot_loaded",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(snapshot_lock_started_at)
        ),
    );
    if workspace_snapshot.session_run_phases.get(&session_id) == Some(&SessionRunPhase::Stopped) {
        if let Ok(mut guard) = state.lock() {
            stop_webui_session_kernel_generation(&mut guard, session_id);
        }
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
            Some(String::from("precheck_stopped")),
        );
        return Ok(None);
    }
    ensure_webui_session_runtime_state(
        &mut workspace_snapshot,
        session_id,
        Some(start.precheck_request.turn_id()),
    );
    let ai_tool_configuration = effective_ai_tool_configuration_for_prompt_overrides(
        &agent_runtime.ai_tool_configuration,
        &start.disabled_ai_capability_kinds,
        &start.disabled_ai_capability_target_selectors,
        &start.ai_capability_target_order_overrides,
    );
    let project_configuration = load_project_configuration(&start.workspace_path);
    let runtime_reasoning_effort = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_reasoning_effort);
    let session_title_for_context = start.session_title.clone();
    let shared_continue_started_at = current_unix_millis_value();
    let continuation = continue_shared_prompt_turn_after_precheck(
        &mut workspace_snapshot,
        SharedPromptTurnContinueInput {
            start,
            precheck_outcome,
            ai_tool_configuration,
            project_configuration,
            runtime_reasoning_effort,
            provider_reasoning_effort: active_reasoning_selection_effort(
                &agent_runtime.provider_configuration,
            ),
            patch_visible_user_prompt_record: runs_user_prompt_precheck,
        },
    )?;
    webui_trace(
        "precheck_continue_request_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(shared_continue_started_at)
        ),
    );
    let vmm_snapshot_started_at = current_unix_millis_value();
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    webui_trace(
        "precheck_continue_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(vmm_snapshot_started_at)
        ),
    );
    if let Some(detail) = continuation.precheck_failure_detail.as_deref() {
        eprintln!("[tauri-service] precheck hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · Precheck hook 执行失败：{detail}"),
        ) {
            eprintln!("[tauri-service] record precheck warning failed: {error}");
        }
    }
    let mut request = continuation.request;
    request.task_session_store_root = owning_project_id_for_session(
        &workspace_snapshot,
        session_id,
    )
    .and_then(|project_id| {
        vulcan_workspace_store::workspace_store::find_segmented_session_directory_for_task_store(
            &controller_runtime.data_root,
            &project_id,
            &session_id.to_string(),
        )
        .ok()
        .flatten()
    });
    if runs_user_prompt_precheck {
        let context_persist_started_at = current_unix_millis_value();
        persist_webui_context_current_document(
            controller_runtime,
            &workspace_snapshot,
            session_id,
            &session_title_for_context,
        );
        webui_trace(
            "precheck_continue_context_document_persisted",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(context_persist_started_at)
            ),
        );
    }
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.updated_at = now.clone();
    }
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Running;
        session.focus = String::from("正在请求");
        session.last_updated_label = String::from("刚刚");
    }
    let store_started_at = current_unix_millis_value();
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot,
    )?;
    webui_trace(
        "precheck_continue_workbench_stored",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(store_started_at)
        ),
    );
    if runs_user_prompt_precheck {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[tauri-service] precheck continue persist snapshot failed: {error}");
                    return;
                }
            };
            spawn_workspace_session_persist_task(
                state_for_persist,
                controller_runtime_for_persist,
                snapshot,
                session_id,
                Some(String::from("precheck_continue")),
            );
        });
    }
    webui_trace(
        "precheck_continue_finished",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    Ok(Some(request))
}

/// Spawns the shared precheck continuation chain for one WebUI prompt turn.
/// 为一个 WebUI 提示词回合启动共享 precheck continuation 链路。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and is forwarded to the stream task.
/// `app` 发出工作台快照，并继续传递给流任务。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
fn spawn_webui_precheck_continue_task(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
) {
    let session_id = start.session_id;
    webui_trace("precheck_task_spawn", format!("session={session_id}"));
    tauri::async_runtime::spawn(async move {
        let precheck_started_at = current_unix_millis_value();
        webui_trace("precheck_task_run_start", format!("session={session_id}"));
        let precheck_outcome = start.precheck_request.clone().run().await;
        webui_trace(
            "precheck_task_run_finished",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(precheck_started_at)
            ),
        );
        let continue_started_at = current_unix_millis_value();
        match continue_webui_prompt_turn_after_precheck_from_state(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            start,
            precheck_outcome,
        ) {
            Ok(Some(request)) => {
                webui_trace(
                    "precheck_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=true elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
                spawn_agent_stream_task(app, state, controller_runtime, agent_runtime, request)
            }
            Ok(None) => {
                webui_trace(
                    "precheck_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=false elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
            }
            Err(error) => {
                eprintln!("[tauri-service] continue WebUI prompt after precheck failed: {error}");
                if let Err(cleanup_error) = mark_webui_precheck_continuation_failed(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    session_id,
                    &error,
                ) {
                    eprintln!(
                        "[tauri-service] cleanup failed WebUI precheck continuation failed: {cleanup_error}"
                    );
                }
            }
        }
    });
}

/// Starts precheck dispatch or immediately continues with a skipped precheck using shared semantics.
/// 使用共享语义启动 precheck 调度，或在需要跳过时立即继续。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and runtime stream events.
/// `app` 发出工作台快照与运行时流事件。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
/// `precheck_plan` decides whether handlers should run.
/// `precheck_plan` 决定是否需要运行处理器。
///
/// # Returns
/// 返回值
/// Whether any continuation or precheck task was started.
/// 是否已经启动 continuation 或 precheck 任务。
fn start_webui_precheck_or_continue(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_plan: SharedPromptPrecheckPlan,
) -> Result<bool, String> {
    let session_id = start.session_id;
    webui_trace(
        "precheck_dispatch_entered",
        format!(
            "session={} has_hooks={} skip={} labels={}",
            session_id,
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len()
        ),
    );
    if precheck_plan.should_skip_precheck || !precheck_plan.has_precheck_hooks {
        let skipped_continue_started_at = current_unix_millis_value();
        let precheck_outcome = start.precheck_request.clone().skip();
        if let Some(request) = continue_webui_prompt_turn_after_precheck_from_state(
            app,
            state,
            controller_runtime,
            agent_runtime,
            start,
            precheck_outcome,
        )? {
            spawn_agent_stream_task(
                app.clone(),
                state.clone(),
                controller_runtime.clone(),
                agent_runtime.clone(),
                request,
            );
            webui_trace(
                "precheck_dispatch_skipped_spawned_stream",
                format!(
                    "session={} elapsed_ms={}",
                    session_id,
                    current_unix_millis_value().saturating_sub(skipped_continue_started_at)
                ),
            );
            return Ok(true);
        }
        webui_trace(
            "precheck_dispatch_skipped_no_stream",
            format!(
                "session={} elapsed_ms={}",
                session_id,
                current_unix_millis_value().saturating_sub(skipped_continue_started_at)
            ),
        );
        return Ok(false);
    }
    spawn_webui_precheck_continue_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        start,
    );
    webui_trace(
        "precheck_dispatch_spawned_task",
        format!("session={session_id}"),
    );
    Ok(true)
}

/// Marks a WebUI prompt turn as stopped when precheck continuation cannot be applied.
/// 当 WebUI 提示词回合无法应用 precheck continuation 时，将其标记为已停止。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `controller_runtime` persists the cleaned session state.
/// `controller_runtime` 持久化清理后的会话状态。
/// `agent_runtime` provides projection configuration for the emitted workbench.
/// `agent_runtime` 提供发出工作台快照所需的投影配置。
/// `session_id` selects the failed prompt turn.
/// `session_id` 选择失败的提示词回合。
/// `error` describes the continuation failure.
/// `error` 描述 continuation 失败原因。
///
/// # Returns
/// 返回值
/// `Ok(())` after the failed state is projected and persisted.
/// 失败状态完成投影与持久化后返回 `Ok(())`。
fn mark_webui_precheck_continuation_failed(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    error: &str,
) -> Result<(), String> {
    if let Ok(mut guard) = state.lock() {
        stop_webui_session_kernel_generation(&mut guard, session_id);
    }
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    let now = current_unix_millis_text();
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.active_precheck_turn_id = None;
        append_runtime_event_projection_to_state(
            runtime_state,
            session_id,
            &now,
            &RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "Precheck 结果已忽略",
                error.to_string(),
            ),
        );
        runtime_state.updated_at = now.clone();
    }
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("Precheck 结果无法应用，已停止本轮请求。");
        session.last_updated_label = String::from("刚刚");
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot,
    )?;
    let state_for_persist = state.clone();
    let controller_runtime_for_persist = controller_runtime.clone();
    tauri::async_runtime::spawn(async move {
        let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[tauri-service] precheck failed persist snapshot failed: {error}");
                return;
            }
        };
        spawn_workspace_session_persist_task(
            state_for_persist,
            controller_runtime_for_persist,
            snapshot,
            session_id,
            Some(String::from("precheck_continue_failed")),
        );
    });
    Ok(())
}

/// Spawns one background task that forwards agent runtime stream events into the WebUI state.
/// 启动一个后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
fn spawn_agent_stream_task(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
) {
    let session_id = request.session_id;
    webui_trace("stream_task_spawn", format!("session={session_id}"));
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    let task_state = state.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let stream_task_started_at = current_unix_millis_value();
        webui_trace("stream_task_entered", format!("session={session_id}"));
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut reached_finished_event = false;
        let prompt_intervention = build_webui_prompt_intervention(
            app.clone(),
            task_state.clone(),
            agent_runtime.provider_configuration.clone(),
            agent_runtime.ai_tool_configuration.clone(),
        );
        let provider_stream_create_started_at = current_unix_millis_value();
        webui_trace(
            "stream_provider_create_start",
            format!("session={session_id}"),
        );
        let mut stream = Box::pin(stream_runtime_run_with_intervention(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            agent_runtime.mcp_runtime.clone(),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Arc::clone(&agent_runtime.token_budget_runtime),
            request,
            Some(prompt_intervention),
        ));

        webui_trace(
            "stream_provider_create_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_trace(
            "stream_task_poll_start",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        // These request-local timers isolate provider latency from UI persistence latency.
        // 这些请求局部计时器用于区分 provider 延迟和界面持久化延迟。
        let mut current_provider_request_started_at: Option<i128> = None;
        let mut first_event_after_request_logged = false;
        let mut first_visible_delta_logged = false;
        while let Some(event) = stream.next().await {
            if !webui_stream_event_matches_kernel_generation(
                &task_state,
                &event,
                expected_kernel_generation,
            ) {
                webui_trace(
                    "stream_event_skipped_stale_generation",
                    format!(
                        "session={} expected_generation={}",
                        execution_stream_event_session_id(&event),
                        expected_kernel_generation
                            .map(|generation| generation.to_string())
                            .unwrap_or_else(|| String::from("<none>"))
                    ),
                );
                break;
            }
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
                    webui_trace(
                        "stream_prepared",
                        format!(
                            "session={} provider={} model={} elapsed_ms={}",
                            preparation.session_id,
                            preparation.model_blueprint.provider_id,
                            preparation.model_blueprint.model_id,
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                    if let Err(error) = apply_execution_preparation(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &preparation,
                    ) {
                        eprintln!("[tauri-service] apply preparation failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestReady { session_id } => {
                    webui_trace(
                        "stream_provider_request_ready",
                        format!(
                            "session={session_id} elapsed_ms_since_task={}",
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                }
                ExecutionStreamEvent::WireRequestReady { session_id } => {
                    webui_trace(
                        "stream_wire_request_ready",
                        format!(
                            "session={session_id} elapsed_ms_since_request={} elapsed_ms_since_task={}",
                            current_provider_request_started_at
                                .map(|started_at| current_unix_millis_value()
                                    .saturating_sub(started_at)
                                    .to_string())
                                .unwrap_or_else(|| String::from("<unknown>")),
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    current_provider_request_started_at = Some(current_unix_millis_value());
                    first_event_after_request_logged = false;
                    first_visible_delta_logged = false;
                    webui_trace("stream_request_started", format!("session={session_id}"));
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[tauri-service] apply request-started failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    if !first_event_after_request_logged {
                        first_event_after_request_logged = true;
                        webui_trace(
                            "stream_first_event_after_request",
                            format!(
                                "session={session_id} event=thinking_delta elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                    }
                    if !first_visible_delta_logged {
                        first_visible_delta_logged = true;
                        webui_trace(
                            "stream_first_visible_delta",
                            format!(
                                "session={session_id} kind=thinking elapsed_ms_since_request={} chars={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                delta.chars().count()
                            ),
                        );
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids.thinking_record_id,
                        SessionMessageRecordKind::ThinkingOutput,
                        "思考",
                        &delta,
                        true,
                    ) {
                        eprintln!("[tauri-service] append thinking delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) =
                        collapse_streaming_thinking(&app, &task_state, session_id, &record_ids)
                    {
                        eprintln!("[tauri-service] complete thinking failed: {error}");
                    }
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[tauri-service] restore request phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    if !first_event_after_request_logged {
                        first_event_after_request_logged = true;
                        webui_trace(
                            "stream_first_event_after_request",
                            format!(
                                "session={session_id} event=output_delta elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                    }
                    if !first_visible_delta_logged {
                        first_visible_delta_logged = true;
                        webui_trace(
                            "stream_first_visible_delta",
                            format!(
                                "session={session_id} kind=output elapsed_ms_since_request={} chars={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                delta.chars().count()
                            ),
                        );
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids.output_record_id,
                        SessionMessageRecordKind::AssistantOutput,
                        "输出",
                        &delta,
                        false,
                    ) {
                        eprintln!("[tauri-service] append output delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    if current_provider_request_started_at.is_some()
                        && !first_event_after_request_logged
                    {
                        first_event_after_request_logged = true;
                        webui_trace(
                            "stream_first_event_after_request",
                            format!(
                                "session={session_id} event=runtime.{:?} elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                event.kind,
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                    }
                    if matches!(
                        event.kind,
                        RuntimeEventKind::ToolStarted
                            | RuntimeEventKind::ToolFinished
                            | RuntimeEventKind::DelegateStarted
                            | RuntimeEventKind::DelegateFinished
                    ) {
                        webui_trace(
                            "stream_runtime_event",
                            format!(
                                "session={session_id} kind={:?} {}",
                                event.kind,
                                runtime_event_trace_detail(&event)
                            ),
                        );
                    }
                    if is_provider_tool_boundary_runtime_event(&event) {
                        record_ids.thinking_record_id = None;
                        record_ids.output_record_id = None;
                    }
                    update_webui_wait_agent_runtime_state(&task_state, session_id, &event);
                    if matches!(event.kind, RuntimeEventKind::ToolStarted) {
                        if let Err(error) = apply_session_phase(
                            &app,
                            &task_state,
                            session_id,
                            SessionRunPhase::ToolRunning,
                        ) {
                            eprintln!("[tauri-service] apply tool phase failed: {error}");
                        }
                    }
                    if matches!(event.kind, RuntimeEventKind::ToolFinished) {
                        if let Err(error) = apply_session_phase(
                            &app,
                            &task_state,
                            session_id,
                            SessionRunPhase::Requesting,
                        ) {
                            eprintln!(
                                "[tauri-service] restore request phase after tool failed: {error}"
                            );
                        }
                    }
                    let event_is_main_session = task_state.lock().ok().is_some_and(|guard| {
                        guard.workspace_snapshot.as_ref().is_some_and(|workspace| {
                            workspace
                                .sessions
                                .iter()
                                .any(|candidate| candidate.id == session_id)
                        })
                    });
                    if event_is_main_session
                        && let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock()
                    {
                        hook_runtime.record_tool_runtime_event(session_id, &event);
                    }
                    if let Err(error) = maybe_start_webui_detached_subagent_task_listener(
                        app.clone(),
                        task_state.clone(),
                        controller_runtime.clone(),
                        agent_runtime.clone(),
                        session_id,
                        &event,
                    ) {
                        eprintln!(
                            "[tauri-service] start detached subagent listener failed: {error}"
                        );
                    }
                    let subagent_event_consumed = match ingest_webui_subagent_runtime_event(
                        &app,
                        &task_state,
                        &agent_runtime,
                        session_id,
                        &event,
                    ) {
                        Ok(consumed) => consumed,
                        Err(error) => {
                            eprintln!("[tauri-service] ingest subagent event failed: {error}");
                            false
                        }
                    };
                    if !subagent_event_consumed
                        && let Err(error) =
                            append_runtime_event_projection(&app, &task_state, session_id, &event)
                    {
                        eprintln!("[tauri-service] append runtime event failed: {error}");
                    }
                    let pending_after_runtime_event_started_at = current_unix_millis_value();
                    match maybe_prepare_next_webui_pending_prompt(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        session_id,
                        false,
                    )
                    .await
                    {
                        Ok(started) => {
                            webui_trace_if_slow(
                                "pending_prompt_after_runtime_event_slow",
                                pending_after_runtime_event_started_at,
                                format!(
                                    "session={session_id} kind={:?} started={started}",
                                    event.kind
                                ),
                            );
                            if started {
                                webui_trace(
                                    "pending_prompt_started_after_runtime_event",
                                    format!("session={session_id} kind={:?}", event.kind),
                                );
                            }
                        }
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] prepare pending prompt after runtime event failed: {error}"
                            );
                        }
                    }
                }
                ExecutionStreamEvent::ConversationCheckpoint {
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    provider_request_count,
                } => {
                    if let Err(error) = apply_conversation_checkpoint(
                        &app,
                        &task_state,
                        &controller_runtime,
                        session_id,
                        conversation_state,
                        last_successful_provider_usage,
                        provider_request_count,
                    ) {
                        eprintln!("[tauri-service] apply checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::Finished(report) => {
                    reached_finished_event = true;
                    let finished_session_id = report.session_id;
                    webui_trace(
                        "stream_finished",
                        format!(
                            "session={finished_session_id} provider_requests={} error={} output_chars={}",
                            report.provider_request_count,
                            report.error.is_some(),
                            report
                                .assistant_output
                                .as_deref()
                                .map(str::len)
                                .unwrap_or(0)
                        ),
                    );
                    if let Ok(mut guard) = task_state.lock() {
                        guard.active_stream_tasks.remove(&finished_session_id);
                        finish_webui_session_kernel_turn(
                            &mut guard,
                            finished_session_id,
                            if report.error.is_some() {
                                TurnTerminalStatus::Failed
                            } else {
                                TurnTerminalStatus::Completed
                            },
                        );
                    }
                    let terminal_lifecycle = report
                        .turn_completed_boundary_event(
                            u64::try_from(current_unix_millis_value()).unwrap_or_default(),
                        )
                        .map(|terminal_boundary_event| {
                            let final_boundary_events = report
                                .final_assistant_boundary_event(
                                    u64::try_from(current_unix_millis_value()).unwrap_or_default(),
                                )
                                .into_iter()
                                .collect::<Vec<_>>();
                            let session_title = report
                                .preparation
                                .as_ref()
                                .map(|preparation| preparation.session_title.clone())
                                .unwrap_or_else(|| String::from("Untitled"));
                            (
                                final_boundary_events,
                                terminal_boundary_event,
                                session_title,
                            )
                        });
                    // Measures terminal report projection, including turn stats and final session state.
                    // 衡量终态报告投影耗时，包括统计行与最终会话状态。
                    let final_report_apply_started_at = current_unix_millis_value();
                    let final_report_apply = match apply_execution_report_to_memory(
                        &app,
                        &task_state,
                        &agent_runtime,
                        report,
                    ) {
                        Ok(apply) => Some(apply),
                        Err(error) => {
                            eprintln!("[tauri-service] apply final report failed: {error}");
                            None
                        }
                    };
                    webui_trace_if_slow(
                        "stream_final_report_apply_slow",
                        final_report_apply_started_at,
                        format!(
                            "session={finished_session_id} applied={}",
                            final_report_apply.is_some()
                        ),
                    );
                    if let (
                        Some(apply),
                        Some((final_boundary_events, terminal_boundary_event, session_title)),
                    ) = (final_report_apply.as_ref(), terminal_lifecycle)
                        && let Some(project_id) = owning_project_id_for_session(
                            &apply.workspace_snapshot,
                            finished_session_id,
                        )
                    {
                        let controller_runtime_for_lifecycle = controller_runtime.clone();
                        tauri::async_runtime::spawn(async move {
                            let lifecycle_started_at = current_unix_millis_value();
                            if let Err(error) = finish_session_external_turn_lifecycle(
                                &controller_runtime_for_lifecycle,
                                &project_id,
                                finished_session_id,
                                &session_title,
                                final_boundary_events,
                                terminal_boundary_event,
                            ) {
                                eprintln!(
                                    "[tauri-service] append session terminal boundary failed: {error}"
                                );
                            }
                            webui_trace_if_slow(
                                "stream_terminal_lifecycle_finish_slow",
                                lifecycle_started_at,
                                format!("session={finished_session_id}"),
                            );
                        });
                    }
                    // Measures automatic mailbox-turn dispatch after a model run finishes.
                    // 衡量模型回合结束后自动 mailbox 回合分发耗时。
                    let mailbox_turn_check_started_at = current_unix_millis_value();
                    let started_mailbox_turn = match maybe_start_webui_mailbox_turn_if_ready(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        finished_session_id,
                    )
                    .await
                    {
                        Ok(started) => started,
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] prepare mailbox turn after stream failed: {error}"
                            );
                            false
                        }
                    };
                    webui_trace_if_slow(
                        "stream_mailbox_turn_check_slow",
                        mailbox_turn_check_started_at,
                        format!("session={finished_session_id} started={started_mailbox_turn}"),
                    );
                    // Measures visible or internal pending prompt dispatch after terminal report handling.
                    // 衡量终态报告处理后可见或内部待发送提示词分发耗时。
                    let next_prompt_check_started_at = current_unix_millis_value();
                    let started_next_prompt = if started_mailbox_turn {
                        true
                    } else {
                        match maybe_prepare_next_webui_pending_prompt(
                            &app,
                            &task_state,
                            &controller_runtime,
                            &agent_runtime,
                            finished_session_id,
                            false,
                        )
                        .await
                        {
                            Ok(started) => started,
                            Err(error) => {
                                eprintln!(
                                    "[tauri-service] prepare next pending prompt failed: {error}"
                                );
                                false
                            }
                        }
                    };
                    webui_trace_if_slow(
                        "stream_next_prompt_check_slow",
                        next_prompt_check_started_at,
                        format!("session={finished_session_id} started={started_next_prompt}"),
                    );
                    webui_trace(
                        "stream_finished_next_prompt",
                        format!(
                            "session={finished_session_id} started_next_prompt={started_next_prompt}"
                        ),
                    );
                    if let Some(completion_request) = final_report_apply
                        .as_ref()
                        .and_then(|apply| apply.completion_request.clone())
                    {
                        spawn_conversation_completion_hook_task(
                            app.clone(),
                            task_state.clone(),
                            controller_runtime.clone(),
                            agent_runtime.clone(),
                            completion_request,
                        );
                    }
                    if !started_next_prompt && let Some(apply) = final_report_apply {
                        // Measures final session persistence only when no chained turn starts immediately.
                        // 仅在没有立即链式启动下一回合时衡量最终会话持久化耗时。
                        let final_persist_started_at = current_unix_millis_value();
                        if let Err(error) = persist_workspace_session_update(
                            Some(&task_state),
                            controller_runtime.clone(),
                            &apply.workspace_snapshot,
                            finished_session_id,
                        )
                        .await
                        {
                            eprintln!("[tauri-service] persist final report failed: {error}");
                        }
                        webui_trace_if_slow(
                            "stream_final_persist_slow",
                            final_persist_started_at,
                            format!("session={finished_session_id}"),
                        );
                    }
                    break;
                }
            }
        }
        if !reached_finished_event && let Ok(mut guard) = task_state.lock() {
            webui_trace(
                "stream_task_ended_without_finished",
                format!("session={session_id}"),
            );
            guard.active_stream_tasks.remove(&session_id);
            finish_webui_session_kernel_turn(&mut guard, session_id, TurnTerminalStatus::Stopped);
        }
    });
    if let Ok(mut guard) = state.lock()
        && let Some(previous_task) = guard.active_stream_tasks.insert(session_id, handle)
    {
        webui_trace(
            "stream_task_abort_previous",
            format!("session={session_id}"),
        );
        previous_task.abort();
    }
}

/// Starts one detached child-task listener using the same task runtime stream as iced.
/// 使用与 iced 相同的任务运行时流启动单个分离子任务监听器。
fn maybe_start_webui_detached_subagent_task_listener(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if event.kind != RuntimeEventKind::DelegateStarted {
        return Ok(());
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(());
    };
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(());
    };
    let task_id = panel_state.task_id;
    webui_trace(
        "detached_listener_delegate_started",
        format!(
            "parent={parent_session_id} task={task_id} public_task_id={} label={}",
            panel_state.public_task_id, panel_state.subagent_label
        ),
    );
    let should_start = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let should_start = guard.detached_subagent_task_listeners.insert(task_id);
        if should_start {
            bind_webui_subagent_parent_turn(&mut guard, parent_session_id, task_id);
        }
        should_start
    };
    if !should_start {
        webui_trace(
            "detached_listener_already_running",
            format!("parent={parent_session_id} task={task_id}"),
        );
        return Ok(());
    }
    webui_trace(
        "detached_listener_spawn",
        format!("parent={parent_session_id} task={task_id}"),
    );
    tauri::async_runtime::spawn(async move {
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut stream = Box::pin(
            agent_runtime
                .task_runtime
                .subscribe_detached_task_events(task_id),
        );
        while let Some(event) = stream.next().await {
            webui_trace(
                "detached_listener_event",
                format!(
                    "task={task_id} kind={} {}",
                    detached_task_event_label(&event),
                    detached_task_event_trace_detail(&event)
                ),
            );
            if let Err(error) = handle_webui_detached_subagent_task_event(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                task_id,
                &mut record_ids,
                event,
            )
            .await
            {
                eprintln!("[tauri-service] detached subagent event failed: {error}");
            }
        }
    });
    Ok(())
}

/// Applies one detached subagent runtime event to the shared WebUI transcript projection.
/// 将单条分离子代理运行时事件应用到共享 WebUI 转录投影。
async fn handle_webui_detached_subagent_task_event(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    task_id: SessionId,
    record_ids: &mut SessionStreamingRecordIds,
    event: DetachedTaskRuntimeEvent,
) -> Result<(), String> {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(event) => {
            handle_webui_detached_child_execution_event(
                app,
                state,
                controller_runtime,
                agent_runtime,
                record_ids,
                event,
            )
            .await
        }
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id,
            event,
        } => {
            let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
            webui_trace(
                "detached_parent_event_received",
                format!(
                    "listener_task={task_id} parent={parent_session_id} kind={:?} is_finished={is_finished}",
                    event.kind
                ),
            );
            let parent_state_check_started_at = current_unix_millis_value();
            let (parent_is_regular_session, parent_is_stopped) = {
                let guard = state
                    .lock()
                    .map_err(|error| format!("failed to lock service state: {error}"))?;
                match guard.workspace_snapshot.as_ref() {
                    Some(workspace_snapshot) => (
                        workspace_snapshot_contains_regular_session(
                            workspace_snapshot,
                            parent_session_id,
                        ),
                        workspace_snapshot
                            .session_run_phases
                            .get(&parent_session_id)
                            == Some(&SessionRunPhase::Stopped),
                    ),
                    None => (false, false),
                }
            };
            webui_trace_if_slow(
                "detached_parent_event_state_check_slow",
                parent_state_check_started_at,
                format!(
                    "listener_task={task_id} parent={parent_session_id} kind={:?}",
                    event.kind
                ),
            );
            let consumed = match ingest_webui_subagent_runtime_event(
                app,
                state,
                agent_runtime,
                parent_session_id,
                &event,
            ) {
                Ok(consumed) => consumed,
                Err(error) => {
                    eprintln!("[tauri-service] ingest subagent parent event failed: {error}");
                    false
                }
            };
            webui_trace(
                "detached_parent_event_ingested",
                format!(
                    "listener_task={task_id} parent={parent_session_id} kind={:?} consumed={consumed}",
                    event.kind
                ),
            );
            if !consumed {
                append_runtime_event_projection(app, state, parent_session_id, &event)?;
            }
            if is_finished {
                {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    guard.detached_subagent_task_listeners.remove(&task_id);
                }
                if let Err(error) = enqueue_webui_session_mailbox_input_if_needed(
                    state,
                    parent_session_id,
                    &event,
                    parent_is_regular_session,
                    parent_is_stopped,
                ) {
                    eprintln!("[tauri-service] enqueue session mailbox input failed: {error}");
                }
                {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    clear_webui_subagent_parent_turn_binding(&mut guard, task_id);
                }
            }
            if webui_detached_parent_event_should_continue_pending_prompt(
                parent_is_regular_session,
                parent_is_stopped,
            ) {
                let started_mailbox_turn = maybe_start_webui_mailbox_turn_if_ready(
                    app,
                    state,
                    controller_runtime,
                    agent_runtime,
                    parent_session_id,
                )
                .await?;
                let started = if started_mailbox_turn {
                    true
                } else {
                    maybe_prepare_next_webui_pending_prompt(
                        app,
                        state,
                        controller_runtime,
                        agent_runtime,
                        parent_session_id,
                        false,
                    )
                    .await?
                };
                webui_trace(
                    "detached_parent_event_pending_result",
                    format!(
                        "parent={parent_session_id} task={task_id} started_next_prompt={started}"
                    ),
                );
            } else if parent_is_regular_session && parent_is_stopped {
                webui_trace(
                    "detached_parent_event_pending_skip_stopped",
                    format!("parent={parent_session_id} task={task_id}"),
                );
            }
            if webui_detached_parent_event_should_persist_terminal_projection(
                is_finished,
                parent_is_regular_session,
                parent_is_stopped,
            ) {
                refresh_webui_subagent_navigation_round_visibility(state, parent_session_id)?;
                webui_trace(
                    "detached_parent_event_persist_spawn",
                    format!("parent={parent_session_id} task={task_id}"),
                );
                let state_for_persist = state.clone();
                let controller_runtime_for_persist = controller_runtime.clone();
                tauri::async_runtime::spawn(async move {
                    let snapshot = match current_workspace_snapshot_from_shared_state(
                        &state_for_persist,
                    ) {
                        Ok(Some(snapshot)) => snapshot,
                        Ok(None) => return,
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] detached subagent persist snapshot failed: {error}"
                            );
                            return;
                        }
                    };
                    spawn_workspace_session_persist_task(
                        state_for_persist,
                        controller_runtime_for_persist,
                        snapshot,
                        parent_session_id,
                        Some(String::from("detached_subagent_parent_event")),
                    );
                });
            }
            Ok(())
        }
    }
}

/// Enqueues one completed subagent result into the parent kernel mailbox when no wait_agent owns it.
/// 当没有 wait_agent 接管时，将单个已完成子代理结果加入父级内核 mailbox。
fn enqueue_webui_session_mailbox_input_if_needed(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
    parent_is_regular_session: bool,
    parent_is_stopped: bool,
) -> Result<bool, String> {
    if event.kind != RuntimeEventKind::DelegateFinished
        || !parent_is_regular_session
        || parent_is_stopped
    {
        return Ok(false);
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(false);
    };
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(false);
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if guard
        .parent_sessions_waiting_for_subagents
        .contains(&parent_session_id)
    {
        webui_trace(
            "session_mailbox_skip_wait_agent",
            format!(
                "parent={parent_session_id} task={} public_task_id={}",
                panel_state.task_id, panel_state.public_task_id
            ),
        );
        return Ok(false);
    }
    if let Some(completed_selectors) = guard
        .parent_session_completed_wait_agent_selectors
        .get(&parent_session_id)
    {
        let candidate_selectors = delegate_finished_mailbox_selector_values(
            parent_session_id,
            panel_state.task_id,
            &panel_state.public_task_id,
            metadata,
        );
        if completed_selectors.is_empty()
            || candidate_selectors
                .iter()
                .any(|selector| completed_selectors.contains(selector))
        {
            webui_trace(
                "session_mailbox_skip_completed_wait_agent",
                format!(
                    "parent={parent_session_id} task={} public_task_id={} wait_selectors={} candidate_selectors={}",
                    panel_state.task_id,
                    panel_state.public_task_id,
                    completed_selectors.len(),
                    candidate_selectors.len()
                ),
            );
            return Ok(false);
        }
    }
    let mailbox_accepted = enqueue_webui_subagent_mailbox_input(
        &mut guard,
        parent_session_id,
        panel_state.task_id,
        metadata,
    );
    if !mailbox_accepted {
        return Ok(false);
    }
    webui_trace(
        "session_mailbox_input_enqueued",
        format!(
            "parent={parent_session_id} task={} public_task_id={} result_chars={}",
            panel_state.task_id,
            panel_state.public_task_id,
            panel_state.result_text.chars().count()
        ),
    );
    Ok(true)
}

/// Builds one provider input from the parent session kernel mailbox.
/// 从父会话内核 mailbox 构建一条 provider 输入。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns runtime kernels.
/// `state` 是持有运行时内核的共享服务状态。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` optionally selects one child task session.
/// `task_id` 可选指定一个子任务会话。
///
/// # Returns
/// 返回值
/// A structured mailbox prompt when matching mailbox input exists.
/// 当存在匹配 mailbox 输入时返回结构化 mailbox prompt。
fn webui_session_mailbox_input_from_kernel(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    task_id: Option<&str>,
) -> Result<Option<String>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(kernel) = guard.session_runtime_kernels.get(&parent_session_id) else {
        return Ok(None);
    };
    let matching_inputs = kernel
        .input_queue()
        .get_unconsumed_mailbox_input()
        .into_iter()
        .filter(|input| {
            input.kind == TurnInputKind::SubagentMessage
                && task_id.is_none_or(|task_id| {
                    input.payload.get("task_id").and_then(Value::as_str) == Some(task_id)
                })
        })
        .collect::<Vec<_>>();
    if matching_inputs.is_empty() {
        return Ok(None);
    };
    let events = matching_inputs
        .into_iter()
        .map(|input| {
            input
                .payload
                .get("mailbox_event")
                .cloned()
                .unwrap_or(input.payload)
        })
        .collect::<Vec<_>>();
    if events.len() == 1 {
        let event_text =
            serde_json::to_string_pretty(&events[0]).unwrap_or_else(|_| events[0].to_string());
        return Ok(Some(format!(
            "<subagent_mailbox_event>\n{event_text}\n</subagent_mailbox_event>"
        )));
    }
    let event_text =
        serde_json::to_string_pretty(&events).unwrap_or_else(|_| json!(events).to_string());
    Ok(Some(format!(
        "<subagent_mailbox_events>\n{event_text}\n</subagent_mailbox_events>"
    )))
}

/// Starts one mailbox-triggered parent model turn directly from kernel mailbox input.
/// 直接基于内核 mailbox 输入启动一个 mailbox 触发的父模型回合。
async fn maybe_start_webui_mailbox_turn_if_ready(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    let mailbox_prepare_started_at = current_unix_millis_value();
    let (
        has_active_run,
        has_active_kernel_turn,
        has_trigger_mailbox,
        preserved_workspace_run_state,
        retained_workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            Some(session_id),
        );
        let has_trigger_mailbox = guard
            .session_runtime_kernels
            .get(&session_id)
            .is_some_and(|kernel| kernel.input_queue().has_trigger_turn_mailbox_items());
        // Reuses the retained session context for mailbox-triggered follow-up turns to avoid disk reloads after child completion.
        // 对 mailbox 触发的后续回合复用保留会话上下文，避免子代理完成后重新从磁盘加载。
        let retained_workspace_snapshot = guard.workspace_snapshot.as_ref().and_then(|snapshot| {
            retained_submission_snapshot_has_session_context(snapshot, session_id)
                .then(|| snapshot.clone())
        });
        (
            webui_session_has_active_run(&guard, session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            has_trigger_mailbox,
            preserved_workspace_run_state,
            retained_workspace_snapshot,
        )
    };
    webui_trace_if_slow(
        "mailbox_turn_prepare_slow",
        mailbox_prepare_started_at,
        format!("session={session_id}"),
    );
    webui_trace(
        "mailbox_turn_prepare_check",
        format!(
            "session={session_id} has_active_run={has_active_run} has_active_kernel_turn={has_active_kernel_turn} has_trigger_mailbox={has_trigger_mailbox}"
        ),
    );
    if has_active_run || has_active_kernel_turn || !has_trigger_mailbox {
        return Ok(false);
    }
    let Some(prompt_text) = webui_session_mailbox_input_from_kernel(state, session_id, None)?
    else {
        return Ok(false);
    };
    let mailbox_workspace_started_at = current_unix_millis_value();
    let session_id_text = session_id.to_string();
    let (mut workspace_snapshot, workspace_source) =
        if let Some(workspace_snapshot) = retained_workspace_snapshot {
            (workspace_snapshot, "retained")
        } else {
            (
                load_workspace_for_submission(controller_runtime.clone(), Some(&session_id_text))
                    .await?,
                "disk",
            )
        };
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    webui_trace_if_slow(
        "mailbox_turn_workspace_ready_slow",
        mailbox_workspace_started_at,
        format!("session={session_id} source={workspace_source}"),
    );
    let Some(session_title) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
    else {
        return Ok(false);
    };
    let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
    let start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: format!("webui-mailbox-{}", SessionId::new_v4()),
                prompt: &prompt_text,
                attachments: Vec::new(),
                append_canonical_user_message: false,
                append_visible_user_prompt_record: false,
                source: PendingSessionPromptSource::SessionMailbox,
                execution_mode: String::from("build"),
                reasoning_effort: active_reasoning_selection_effort(
                    &agent_runtime.provider_configuration,
                ),
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
            },
        )
    }?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_turn(
            &mut guard,
            session_id,
            start.precheck_request.turn_id(),
            ActiveTurnKind::MailboxTriggered,
        )?;
        if let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) {
            let consumed = kernel.input_queue_mut().drain_current_turn_inputs().len();
            webui_trace(
                "mailbox_turn_inputs_consumed",
                format!("session={session_id} consumed={consumed}"),
            );
        }
    }
    apply_webui_prompt_begin_outer_steps(
        controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot,
    )?;
    let precheck_plan = SharedPromptPrecheckPlan {
        has_precheck_hooks: false,
        should_skip_precheck: true,
        progress_labels: Vec::new(),
    };
    start_webui_precheck_or_continue(
        app,
        state,
        controller_runtime,
        agent_runtime,
        start,
        precheck_plan,
    )?;
    webui_trace(
        "mailbox_turn_started",
        format!(
            "session={session_id} prompt_chars={}",
            prompt_text.chars().count()
        ),
    );
    Ok(true)
}

/// Returns whether a detached subagent parent event may dispatch the next queued prompt.
/// 返回分离子代理父级事件是否可以调度下一条排队提示词。
fn webui_detached_parent_event_should_continue_pending_prompt(
    parent_is_regular_session: bool,
    parent_is_stopped: bool,
) -> bool {
    parent_is_regular_session && !parent_is_stopped
}

/// Returns whether one detached terminal event may refresh and persist the parent projection.
/// 返回一条分离终态事件是否可以刷新并持久化父级投影。
fn webui_detached_parent_event_should_persist_terminal_projection(
    is_finished: bool,
    parent_is_regular_session: bool,
    parent_is_stopped: bool,
) -> bool {
    is_finished
        && webui_detached_parent_event_should_continue_pending_prompt(
            parent_is_regular_session,
            parent_is_stopped,
        )
}

/// Applies one child execution stream event using the same projection functions as root WebUI turns.
/// 使用与根 WebUI 回合相同的投影函数应用单条子执行流事件。
async fn handle_webui_detached_child_execution_event(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    record_ids: &mut SessionStreamingRecordIds,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::RequestReady { .. }
        | ExecutionStreamEvent::WireRequestReady { .. } => Ok(()),
        ExecutionStreamEvent::RequestStarted { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { session_id, delta } => append_stream_delta_record(
            app,
            state,
            session_id,
            &mut record_ids.thinking_record_id,
            SessionMessageRecordKind::ThinkingOutput,
            "思考",
            &delta,
            true,
        ),
        ExecutionStreamEvent::ThinkingCompleted { session_id } => {
            collapse_streaming_thinking(app, state, session_id, record_ids)?;
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::OutputDelta { session_id, delta } => append_stream_delta_record(
            app,
            state,
            session_id,
            &mut record_ids.output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            &delta,
            false,
        ),
        ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
            if is_provider_tool_boundary_runtime_event(&event) {
                record_ids.thinking_record_id = None;
                record_ids.output_record_id = None;
            }
            append_runtime_event_projection(app, state, session_id, &event)
        }
        ExecutionStreamEvent::ConversationCheckpoint {
            session_id,
            conversation_state,
            last_successful_provider_usage,
            provider_request_count,
        } => apply_conversation_checkpoint(
            app,
            state,
            controller_runtime,
            session_id,
            conversation_state,
            last_successful_provider_usage,
            provider_request_count,
        ),
        ExecutionStreamEvent::Finished(report) => {
            let _ = (app, agent_runtime);
            apply_task_session_execution_report_to_memory_in_place(state, &report).map(|_| ())
        }
    }
}

/// Builds the WebUI runtime intervention that injects guided prompts before the next provider request.
/// 构建 WebUI 运行时干预器，用于在下一次 Provider 请求前注入引导提示词。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit refreshed workbench snapshots.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` is the shared service bridge state that owns the active workspace snapshot.
/// `state` 是拥有当前工作区快照的共享服务桥接状态。
/// `provider_configuration` is the provider configuration used by the shared pending router.
/// `provider_configuration` 是共享待发送路由器使用的 Provider 配置。
///
/// # Returns
/// 返回值
/// A callback consumed by the host-controlled runtime stream.
/// 返回供宿主控制运行时流消费的回调。
fn build_webui_prompt_intervention(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    provider_configuration: ProviderConfigurationFile,
    ai_tool_configuration: PersistedAiToolConfiguration,
) -> RuntimePromptIntervention {
    Arc::new(move |session_id, controller: &mut VulcanCoreController| {
        match take_webui_internal_guidance_prompts(
            &app,
            &state,
            &provider_configuration,
            session_id,
        ) {
            Ok(entries) => {
                let pending_prompt_changed = entries
                    .iter()
                    .filter_map(|entry| {
                        let (prompt, attachments) = normalize_webui_interjected_prompt_for_provider(
                            &provider_configuration,
                            &ai_tool_configuration,
                            entry,
                        );
                        controller.append_interjected_user_turn_with_attachments(
                            prompt.as_str(),
                            attachments,
                        )
                    })
                    .count()
                    > 0;
                let mailbox_changed =
                    match take_webui_mailbox_intervention_prompt(&state, session_id) {
                        Ok(Some(prompt)) => {
                            let changed =
                                controller.append_interjected_user_turn(&prompt).is_some();
                            webui_trace(
                                "mailbox_intervention_appended",
                                format!(
                                    "session={session_id} changed={changed} prompt_chars={}",
                                    prompt.chars().count()
                                ),
                            );
                            changed
                        }
                        Ok(None) => false,
                        Err(error) => {
                            eprintln!(
                                "[tauri-service] mailbox intervention prompt failed: {error}"
                            );
                            false
                        }
                    };
                pending_prompt_changed || mailbox_changed
            }
            Err(error) => {
                eprintln!("[tauri-service] apply guided prompt intervention failed: {error}");
                false
            }
        }
    })
}

/// Takes one provider-loop mailbox intervention prompt and marks included mailbox inputs consumed.
/// 获取一条 Provider 循环 mailbox 干预提示词，并将包含的 mailbox 输入标记为已消费。
fn take_webui_mailbox_intervention_prompt(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    let Some(prompt) = webui_session_mailbox_input_from_kernel(state, session_id, None)? else {
        return Ok(None);
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) else {
        return Ok(None);
    };
    if kernel.has_wait_scopes() {
        return Ok(None);
    }
    let input_ids = kernel
        .input_queue()
        .get_unconsumed_mailbox_input()
        .into_iter()
        .filter(|input| input.kind == TurnInputKind::SubagentMessage)
        .map(|input| input.input_id)
        .collect::<Vec<_>>();
    if input_ids.is_empty() {
        return Ok(None);
    }
    let mut consumed = 0usize;
    for input_id in input_ids {
        if kernel.input_queue_mut().mark_input_consumed(input_id) {
            consumed = consumed.saturating_add(1);
        }
    }
    webui_trace(
        "mailbox_intervention_inputs_consumed",
        format!("session={session_id} consumed={consumed}"),
    );
    Ok(Some(prompt))
}

/// Updates the parent-session wait_agent guard from provider runtime tool events.
/// 根据 Provider 运行时工具事件更新父会话 wait_agent 等待闸门。
fn update_webui_wait_agent_runtime_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) {
    let Some(tool_name) = runtime_event_tool_name(event) else {
        return;
    };
    let Ok(mut guard) = state.lock() else {
        return;
    };
    match (tool_name, event.kind) {
        ("spawn_agent" | "followup_task", RuntimeEventKind::ToolStarted) => {
            guard
                .parent_session_completed_wait_agent_selectors
                .remove(&session_id);
        }
        ("wait_agent", RuntimeEventKind::ToolStarted) => {
            guard
                .parent_sessions_waiting_for_subagents
                .insert(session_id);
            guard
                .parent_session_completed_wait_agent_selectors
                .remove(&session_id);
            let scope_id = wait_agent_scope_id(event);
            let target_agent_ids = wait_agent_target_agent_ids_from_event(event);
            let kernel = session_runtime_kernel_mut(&mut guard, session_id);
            if !kernel.has_wait_scopes()
                && let Err(error) = kernel.register_wait_scope(
                    scope_id,
                    target_agent_ids,
                    current_unix_millis_u64(),
                )
            {
                webui_trace(
                    "wait_agent_scope_register_failed",
                    format!("session={session_id} error={}", error.message),
                );
            }
        }
        ("wait_agent", RuntimeEventKind::ToolFinished) => {
            guard
                .parent_sessions_waiting_for_subagents
                .remove(&session_id);
            if wait_agent_runtime_event_completed(event) {
                let consumed =
                    consume_wait_agent_mailbox_inputs_from_event(&mut guard, session_id, event);
                webui_trace(
                    "wait_agent_mailbox_inputs_consumed",
                    format!("session={session_id} consumed={consumed}"),
                );
                let completed_selectors = wait_agent_completed_agent_selectors(event);
                let selector_count = completed_selectors.len();
                guard
                    .parent_session_completed_wait_agent_selectors
                    .insert(session_id, completed_selectors);
                webui_trace(
                    "wait_agent_completed_selectors_recorded",
                    format!("session={session_id} selectors={selector_count}"),
                );
            }
            if let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) {
                let scope_id = wait_agent_scope_id(event);
                let _ = kernel.clear_wait_scope(&scope_id);
            }
        }
        _ => {}
    }
}

/// Returns the stable kernel wait-scope id for one wait_agent runtime event.
/// 返回单条 wait_agent 运行时事件对应的稳定内核等待范围标识。
fn wait_agent_scope_id(event: &RuntimeEvent) -> String {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_call_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|tool_call_id| format!("wait-agent-{tool_call_id}"))
        .unwrap_or_else(|| String::from("wait-agent-current"))
}

/// Extracts requested child-agent selectors from wait_agent tool-start metadata.
/// 从 wait_agent 工具开始事件元数据中提取请求的子代理选择器。
fn wait_agent_target_agent_ids_from_event(event: &RuntimeEvent) -> Vec<String> {
    let Some(arguments) = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("arguments"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    [
        "task_name",
        "task_id",
        "agent_id",
        "child_thread_id",
        "name",
    ]
    .iter()
    .filter_map(|key| arguments.get(*key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToString::to_string)
    .collect()
}

/// Marks matching parent mailbox inputs as consumed by a completed wait_agent result.
/// 将已完成 wait_agent 结果匹配到的父级 mailbox 输入标记为已消费。
fn consume_wait_agent_mailbox_inputs_from_event(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> usize {
    let selectors = wait_agent_completed_agent_selectors(event);
    let scope_id = wait_agent_scope_id(event);
    let Some(kernel) = state.session_runtime_kernels.get_mut(&session_id) else {
        return 0;
    };
    if !kernel.has_wait_scopes() {
        let _ = kernel.register_wait_scope(
            scope_id.clone(),
            Vec::<String>::new(),
            current_unix_millis_u64(),
        );
    }
    let input_ids = kernel
        .input_queue()
        .get_unconsumed_mailbox_input()
        .into_iter()
        .filter(|input| {
            input.kind == TurnInputKind::SubagentMessage
                && (selectors.is_empty() || mailbox_input_matches_any_selector(input, &selectors))
        })
        .map(|input| input.input_id)
        .collect::<Vec<_>>();
    let mut consumed = 0usize;
    for input_id in input_ids {
        if kernel.consume_mailbox_input_for_wait_scope(&scope_id, &input_id) {
            consumed = consumed.saturating_add(1);
        }
    }
    consumed
}

/// Extracts completed child-agent selectors from a wait_agent tool-finished metadata payload.
/// 从 wait_agent 工具完成元数据中提取已完成子代理选择器。
fn wait_agent_completed_agent_selectors(event: &RuntimeEvent) -> BTreeSet<String> {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| host_result.get("payload"))
        .and_then(|payload| payload.get("agents"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|agent| {
            [
                agent.get("task_name").and_then(Value::as_str),
                agent.get("task_session_id").and_then(Value::as_str),
                agent.get("agent_id").and_then(Value::as_str),
                agent.get("child_thread_id").and_then(Value::as_str),
                agent.get("name").and_then(Value::as_str),
            ]
        })
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Returns whether one mailbox input belongs to any wait_agent-completed selector.
/// 返回一条 mailbox 输入是否属于任一 wait_agent 已完成选择器。
fn mailbox_input_matches_any_selector(input: &TurnInput, selectors: &BTreeSet<String>) -> bool {
    mailbox_input_selector_values(input)
        .iter()
        .any(|value| selectors.contains(value))
}

/// Collects stable selector values carried by one subagent mailbox input.
/// 收集单条子代理 mailbox 输入携带的稳定选择器值。
fn mailbox_input_selector_values(input: &TurnInput) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    push_json_string_value(&mut values, input.payload.get("task_id"));
    push_json_string_value(&mut values, input.payload.get("parent_session_id"));
    if let Some(mailbox_event) = input.payload.get("mailbox_event") {
        push_json_string_value(&mut values, mailbox_event.get("child_thread_id"));
        push_json_string_value(&mut values, mailbox_event.get("parent_thread_id"));
        if let Some(payload) = mailbox_event.get("payload") {
            for key in [
                "task_name",
                "task_id",
                "task_session_id",
                "public_id",
                "child_thread_id",
                "agent_id",
                "name",
            ] {
                push_json_string_value(&mut values, payload.get(key));
            }
        }
    }
    values
}

/// Collects selector values from one delegate-finished event before it is enqueued into the mailbox.
/// 在委派完成事件进入 mailbox 前收集其选择器值。
fn delegate_finished_mailbox_selector_values(
    parent_session_id: SessionId,
    task_id: SessionId,
    public_task_id: &str,
    metadata: &Value,
) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    values.insert(parent_session_id.to_string());
    values.insert(task_id.to_string());
    if !public_task_id.trim().is_empty() {
        values.insert(public_task_id.trim().to_string());
    }
    if let Some(payload) = metadata.get("payload") {
        for key in [
            "task_name",
            "task_id",
            "task_session_id",
            "public_id",
            "child_thread_id",
            "agent_id",
            "name",
        ] {
            push_json_string_value(&mut values, payload.get(key));
        }
        if let Some(mailbox_event) = payload.get("subagent_mailbox_event") {
            push_json_string_value(&mut values, mailbox_event.get("child_thread_id"));
            push_json_string_value(&mut values, mailbox_event.get("parent_thread_id"));
            if let Some(mailbox_payload) = mailbox_event.get("payload") {
                for key in [
                    "task_name",
                    "task_id",
                    "task_session_id",
                    "public_id",
                    "child_thread_id",
                    "agent_id",
                    "name",
                ] {
                    push_json_string_value(&mut values, mailbox_payload.get(key));
                }
            }
        }
    }
    values
}

/// Inserts a non-empty JSON string into a selector set.
/// 将非空 JSON 字符串插入选择器集合。
fn push_json_string_value(values: &mut BTreeSet<String>, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str).map(str::trim)
        && !value.is_empty()
    {
        values.insert(value.to_string());
    }
}

/// Returns whether one wait_agent tool-finished event collected all selected child agents.
/// 返回一次 wait_agent 工具完成事件是否已经收集了所有选中的子代理。
fn wait_agent_runtime_event_completed(event: &RuntimeEvent) -> bool {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| host_result.get("payload"))
        .and_then(|payload| payload.get("completed"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Extracts the canonical tool name carried by one runtime event metadata payload.
/// 提取单条运行时事件元数据中携带的规范工具名称。
fn runtime_event_tool_name(event: &RuntimeEvent) -> Option<&str> {
    event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

/// Normalizes one dynamically interjected WebUI prompt through the same provider/model capability gate used by request bootstrap.
/// 通过与请求 bootstrap 相同的 Provider/模型能力门禁归一化一条 WebUI 动态引导提示词。
///
/// # Parameters
/// 参数
/// `provider_configuration` is the active provider configuration captured by the running stream.
/// `provider_configuration` 是当前运行流捕获的活跃 Provider 配置。
/// `ai_tool_configuration` is the active AI tool routing configuration captured by the running stream.
/// `ai_tool_configuration` 是当前运行流捕获的活跃 AI 工具路由配置。
/// `entry` is the internal queued prompt selected for runtime guidance.
/// `entry` 是被选中用于运行时引导的内部排队提示词。
///
/// # Returns
/// 返回值
/// Provider-safe prompt text and attachments after unsupported modalities have been downgraded.
/// 返回不支持模态被降级后的 Provider 安全提示词文本与附件集合。
fn normalize_webui_interjected_prompt_for_provider(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    entry: &PendingSessionPromptEntry,
) -> (String, Vec<ConversationAttachment>) {
    let scoped_provider_configuration = provider_configuration_for_model_reference(
        provider_configuration,
        entry.model_reference.as_ref(),
    );
    let Some((provider_id, model_id)) = scoped_provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
    else {
        return (entry.prompt.clone(), entry.attachments.clone());
    };
    let Some(definition) = provider_config::supported_provider_by_id(provider_id) else {
        return (entry.prompt.clone(), entry.attachments.clone());
    };
    let effective_ai_tool_configuration = effective_ai_tool_configuration_for_prompt_overrides(
        ai_tool_configuration,
        &entry.runtime_options.disabled_ai_capability_kinds,
        &BTreeSet::new(),
        &BTreeMap::new(),
    );
    let active_model =
        provider_config::effective_model_choices(provider_id, &scoped_provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    let capabilities = resolve_effective_model_capabilities(
        definition,
        active_model.as_ref(),
        &effective_ai_tool_configuration,
    );
    let api_family = definition
        .effective_runtime_for_model(model_id)
        .api_family();
    let normalized = normalize_provider_user_input_for_model(
        api_family,
        Some(&capabilities),
        &entry.prompt,
        &entry.attachments,
    );
    (normalized.text, normalized.attachments)
}

/// Takes already-guided WebUI prompts from the shared internal queue.
/// 从共享内部队列取出已经引导的 WebUI 提示词集合。
///
/// # Parameters
/// 参数
/// `app` is used by the workspace mutation helper to emit the updated queue view.
/// `app` 由工作区变更辅助函数用于发出更新后的队列视图。
/// `state` contains the current workspace snapshot and live runtime handles.
/// `state` 包含当前工作区快照与实时运行时句柄。
/// `provider_configuration` lets the shared router preserve iced Anthropic dispatch preference.
/// `provider_configuration` 用于让共享路由器保留 iced 的 Anthropic 分发优先级。
/// `session_id` identifies the session whose stream reached a safe tool boundary.
/// `session_id` 标识流已经到达安全工具边界的会话。
///
/// # Returns
/// 返回值
/// Guided prompt entries waiting in the internal queue, preserving queue order.
/// 按队列顺序返回内部队列中等待的引导条目集合。
fn take_webui_internal_guidance_prompts(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
    session_id: SessionId,
) -> Result<Vec<PendingSessionPromptEntry>, String> {
    let mut guided_entries = Vec::new();
    with_workspace_mutation(app, state, |workspace| {
        let dispatch_context = webui_pending_prompt_dispatch_context(
            workspace,
            provider_configuration,
            session_id,
            false,
            false,
        );
        let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
            return;
        };
        guided_entries = take_runtime_intervention_guided_pending_prompt_entries(
            &mut runtime_state.ui_state,
            dispatch_context,
        );
        if guided_entries.is_empty() {
            return;
        }
        runtime_state.updated_at = current_unix_millis_text();
    })?;
    Ok(guided_entries)
}

/// Clears stale session-mailbox entries from the old WebUI pending-prompt queue.
/// 从旧 WebUI 待发送队列中清理过时的会话 mailbox 条目。
///
/// # Parameters
/// 参数
/// `app` is used by the workspace mutation helper to emit the updated queue view.
/// `app` 由工作区变更辅助函数用于发出更新后的队列视图。
/// `state` owns the current workspace snapshot.
/// `state` 持有当前工作区快照。
/// `session_id` identifies the parent session whose queue should be cleaned.
/// `session_id` 标识需要清理队列的父会话。
///
/// # Returns
/// 返回值
/// Number of removed stale mailbox entries.
/// 被移除的过时 mailbox 条目数量。
fn clear_stale_webui_session_mailbox_pending_prompts(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<usize, String> {
    let mut removed = 0usize;
    with_workspace_mutation(app, state, |workspace| {
        let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
            return;
        };
        removed = remove_internal_pending_prompt_entries_by_source(
            &mut runtime_state.ui_state,
            PendingSessionPromptSource::SessionMailbox,
        );
        if removed > 0 {
            runtime_state.updated_at = current_unix_millis_text();
        }
    })?;
    Ok(removed)
}

/// Prepares the next visible queued prompt after one WebUI-owned session run reaches a clean idle point.
/// 在 WebUI 拥有的单次会话运行到达干净空闲点后准备下一条可见排队提示词。
///
/// `allow_from_stopped_phase` lets manual insert match iced while automatic continuation still respects stop.
/// `allow_from_stopped_phase` 让手动插入对齐 iced，同时自动续跑仍尊重停止状态。
async fn maybe_prepare_next_webui_pending_prompt(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    allow_from_stopped_phase: bool,
) -> Result<bool, String> {
    let (
        mut workspace_snapshot,
        has_active_stream_task,
        has_active_kernel_turn,
        has_active_session_run,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        (
            workspace_snapshot,
            guard.active_stream_tasks.contains_key(&session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            webui_session_has_active_run(&guard, session_id),
        )
    };
    let dispatch_context = webui_pending_prompt_dispatch_context(
        &workspace_snapshot,
        &agent_runtime.provider_configuration,
        session_id,
        has_active_session_run,
        false,
    );
    let priority_override = false;
    let (internal_queue_len, visible_queue_len, run_phase) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| {
            (
                runtime_state.ui_state.pending_prompt_queue.len(),
                runtime_state.ui_state.pending_prompt_panel_queue.len(),
                workspace_snapshot
                    .session_run_phases
                    .get(&session_id)
                    .copied(),
            )
        })
        .unwrap_or((0, 0, None));
    webui_trace(
        "pending_prompt_prepare_check",
        format!(
            "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_running_subagents=false priority_override={priority_override} internal_queue_len={internal_queue_len} visible_queue_len={visible_queue_len} run_phase={run_phase:?} allow_from_stopped_phase={allow_from_stopped_phase}"
        ),
    );
    let Some(next_source) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| {
            next_pending_prompt_dispatch_source(&runtime_state.ui_state, priority_override)
        })
    else {
        webui_trace(
            "pending_prompt_prepare_none",
            format!("session={session_id}"),
        );
        return Ok(false);
    };
    if next_source == PendingSessionPromptSource::SessionMailbox {
        let removed = clear_stale_webui_session_mailbox_pending_prompts(app, state, session_id)?;
        webui_trace(
            "pending_prompt_stale_session_mailbox_cleared",
            format!("session={session_id} removed={removed}"),
        );
        return Ok(false);
    }
    let block_reason = pending_prompt_dispatch_block_reason_for_source(
        dispatch_context,
        allow_from_stopped_phase,
        Some(next_source),
    )
    .is_some();
    if let Some(reason) = pending_prompt_dispatch_block_reason_for_source(
        dispatch_context,
        allow_from_stopped_phase,
        Some(next_source),
    ) {
        webui_trace(
            "pending_prompt_prepare_blocked",
            format!("session={session_id} next_source={next_source:?} reason={reason:?}"),
        );
        return Ok(false);
    }
    let Some(session_title) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
    else {
        webui_trace(
            "pending_prompt_prepare_missing_session_title",
            format!("session={session_id} next_source={next_source:?} block_reason={block_reason}"),
        );
        return Ok(false);
    };
    let Some(entry) =
        take_next_webui_pending_prompt(&mut workspace_snapshot, session_id, priority_override)
    else {
        webui_trace(
            "pending_prompt_prepare_take_none",
            format!(
                "session={session_id} next_source={next_source:?} priority_override={priority_override}"
            ),
        );
        return Ok(false);
    };
    webui_trace(
        "pending_prompt_prepare_start",
        format!(
            "session={session_id} entry_id={} source={:?} task_id={} prompt_chars={} attachments={}",
            entry.id,
            entry.source,
            entry.task_id.as_deref().unwrap_or("<none>"),
            entry.prompt.chars().count(),
            entry.attachments.len()
        ),
    );
    if entry.source.starts_user_parent_turn() {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.session_subagent_navigation_rounds.remove(&session_id);
        if guard
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            guard.active_subagent_panel = None;
        }
    }
    let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
    let runtime_options = entry.runtime_options.clone();
    let entry_agent_runtime = agent_runtime_for_pending_prompt(agent_runtime, &entry);
    let panel_entry_id = entry.panel_entry_id.clone();
    let disabled_ai_capability_kinds = runtime_options
        .disabled_ai_capability_kinds
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if entry.source.runs_user_prompt_precheck() {
        snapshot_webui_context_before_user_prompt(
            controller_runtime,
            &workspace_snapshot,
            session_id,
            entry.source,
        )?;
    }
    let prompt_text = entry.prompt.clone();
    let precheck_start = {
        let mut hook_runtime = entry_agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: entry.id.clone(),
                prompt: &prompt_text,
                attachments: entry.attachments,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: entry.source.starts_user_parent_turn(),
                source: entry.source,
                execution_mode: String::from("build"),
                reasoning_effort: Some(runtime_options.reasoning_effort),
                disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
            },
        )
    }?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_turn(
            &mut guard,
            session_id,
            precheck_start.precheck_request.turn_id(),
            ActiveTurnKind::UserPrompt,
        )?;
    }
    apply_webui_prompt_begin_outer_steps(
        controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    entry_agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &entry_agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    let precheck_plan = if entry.source.runs_user_prompt_precheck() {
        webui_shared_prompt_precheck_plan(
            &entry_agent_runtime,
            &workspace_snapshot,
            session_id,
            &workspace_path,
        )?
    } else {
        SharedPromptPrecheckPlan {
            has_precheck_hooks: false,
            should_skip_precheck: true,
            progress_labels: Vec::new(),
        }
    };
    if let Some(panel_entry_id) = panel_entry_id.as_deref()
        && let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
    {
        let _ = remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, panel_entry_id);
        runtime_state.updated_at = current_unix_millis_text();
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        workspace_snapshot,
    )?;
    if entry.source.runs_user_prompt_precheck() {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        tauri::async_runtime::spawn(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[tauri-service] queued begin persist snapshot failed: {error}");
                    return;
                }
            };
            spawn_workspace_session_runtime_state_persist_task(
                state_for_persist,
                controller_runtime_for_persist,
                snapshot,
                session_id,
                Some(String::from("queued_begin")),
            );
        });
    }
    start_webui_precheck_or_continue(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        precheck_start,
        precheck_plan,
    )?;
    webui_trace(
        "pending_prompt_prepare_started",
        format!("session={session_id}"),
    );
    Ok(true)
}

/// Removes the next dispatchable queued prompt from the internal or visible WebUI queues.
/// 从 WebUI 内部或可见队列中移除下一条可分发排队提示词。
fn take_next_webui_pending_prompt(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    priority_override: bool,
) -> Option<PendingSessionPromptEntry> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)?;
    let created_at = current_unix_millis_text();
    let internal_entry_id = format!("webui-pending-internal-{}", SessionId::new_v4());
    let entry = take_next_pending_prompt_entry(
        &mut runtime_state.ui_state,
        priority_override,
        internal_entry_id,
        created_at,
    )?;
    runtime_state.updated_at = current_unix_millis_text();
    Some(entry)
}

/// Rebuilds and emits the retained workbench snapshot after a service-side queue mutation.
/// 在 service 侧队列变更后重建并发出保留的工作台快照。
fn store_current_workbench_snapshot(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: WorkspaceSnapshot,
) -> Result<WorkbenchSnapshot, String> {
    // Measures this retained-workbench rebuild entry used by precheck and mailbox turns.
    // 衡量 precheck 与 mailbox 回合共用的保留工作台重建入口。
    let trace_started_at = current_unix_millis_value();
    // Measures runtime catalog extraction before surface projection.
    // 衡量界面投影前运行时目录提取耗时。
    let catalog_started_at = current_unix_millis_value();
    let (luaskills_catalog, token_budget_runtime, mcp_catalog, logs) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        (
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            guard.startup.recent_logs.clone(),
        )
    };
    webui_trace_if_slow(
        "store_current_workbench_catalog_slow",
        catalog_started_at,
        format!("selected_session={}", workspace_snapshot.selected_session),
    );
    // Measures active surface and subagent navigation projection.
    // 衡量活动界面与子代理导航投影耗时。
    let surface_started_at = current_unix_millis_value();
    let (live_context_budgets, surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.live_context_budgets.clone(),
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
        )
    };
    webui_trace_if_slow(
        "store_current_workbench_surface_slow",
        surface_started_at,
        format!(
            "selected_session={} surface_messages={} has_subagent_navigation={}",
            workspace_snapshot.selected_session,
            surface_messages.len(),
            subagent_navigation.is_some()
        ),
    );
    // Measures final DTO construction after runtime projection inputs are ready.
    // 衡量运行时投影输入就绪后的最终 DTO 构建耗时。
    let workbench_started_at = current_unix_millis_value();
    let workbench = workbench_from_runtime(
        controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    webui_trace_if_slow(
        "store_current_workbench_build_slow",
        workbench_started_at,
        format!("selected_session={}", workspace_snapshot.selected_session),
    );
    // Measures retained state commit for rebuilt workbench snapshots.
    // 衡量重建工作台快照写回保留状态的耗时。
    let commit_started_at = current_unix_millis_value();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    webui_trace_if_slow(
        "store_current_workbench_commit_slow",
        commit_started_at,
        format!(
            "selected_conversation={}",
            workbench
                .selected_conversation_id
                .as_deref()
                .unwrap_or("<none>")
        ),
    );
    // Measures frontend event emission cost for rebuilt workbench snapshots.
    // 衡量重建工作台快照前端事件发出的耗时。
    let emit_started_at = current_unix_millis_value();
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    webui_trace_if_slow(
        "store_current_workbench_emit_slow",
        emit_started_at,
        format!(
            "selected_conversation={}",
            workbench
                .selected_conversation_id
                .as_deref()
                .unwrap_or("<none>")
        ),
    );
    webui_trace_if_slow(
        "store_current_workbench_total_slow",
        trace_started_at,
        format!(
            "selected_conversation={}",
            workbench
                .selected_conversation_id
                .as_deref()
                .unwrap_or("<none>")
        ),
    );
    Ok(workbench)
}

/// Emits a display-only workbench snapshot after a subagent route change.
/// 在子代理显示路由变更后发出仅显示用途的工作台快照。
fn emit_subagent_display_route_workbench_snapshot(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    workspace_snapshot: WorkspaceSnapshot,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workbench =
            build_subagent_display_route_workbench_snapshot_locked(&mut guard, &workspace_snapshot);
        guard.workspace_snapshot = Some(workspace_snapshot);
        workbench
    };
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Builds one display-only workbench snapshot for a subagent route without cloning the full workspace snapshot.
/// 构建一份子代理路由专用的显示快照，避免克隆完整工作区快照。
fn build_subagent_display_route_workbench_snapshot_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> WorkbenchSnapshot {
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(guard, workspace_snapshot);
    let active_session_id =
        preserved_active_surface_session_id(workspace_snapshot, subagent_navigation.as_ref());
    let mut surface_messages = surface_messages_for_session_without_subagent_navigation_refresh(
        guard,
        workspace_snapshot,
        active_session_id,
    );
    repair_task_mirror_statuses_for_surface_messages_locked(
        guard,
        active_session_id,
        surface_messages.as_mut_slice(),
    );
    let mut workbench = guard.workbench.clone();
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.clone())
        .or_else(|| {
            (workspace_snapshot.selected_session != SessionId::nil())
                .then(|| workspace_snapshot.selected_session.to_string())
        });
    workbench.messages = active_conversation_id
        .as_deref()
        .map(|conversation_id| {
            conversation_messages_from_session(conversation_id, surface_messages.as_slice())
        })
        .unwrap_or_default();
    workbench.subagent_navigation = subagent_navigation;
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench = workbench.clone();
    workbench
}

/// Applies execution preparation identity fields to the target session runtime state.
/// 将执行准备得到的身份字段应用到目标会话运行时状态。
fn apply_execution_preparation(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    if let Some(context_budget) = preparation.context_budget.clone() {
        state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .live_context_budgets
            .insert(preparation.session_id, context_budget);
    }
    let mut boundary_project_id: Option<String> = None;
    with_workspace_mutation(app, state, |workspace| {
        let runtime_state = workspace
            .session_runtime_state
            .entry(preparation.session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.active_agent_id = Some(preparation.agent_id.clone());
        runtime_state.active_provider_id = Some(preparation.model_blueprint.provider_id.clone());
        runtime_state.active_model_id = Some(preparation.model_blueprint.model_id.clone());
        runtime_state.active_openrouter_endpoint_route = preparation
            .model_blueprint
            .openrouter_endpoint_route
            .clone();
        runtime_state.active_reasoning_effort =
            Some(preparation.model_blueprint.reasoning.requested_effort);
        runtime_state.active_temperature = preparation.temperature;
        runtime_state.live_context_budget_fingerprint = preparation
            .context_budget
            .as_ref()
            .map(|budget| budget.fingerprint.clone());
        runtime_state.live_context_assembly_fingerprint = preparation
            .context_baseline
            .as_ref()
            .map(|baseline| baseline.fingerprint.clone());
        runtime_state.updated_at = current_unix_millis_text();
        boundary_project_id = owning_project_id_for_session(workspace, preparation.session_id);
        workspace
            .session_run_phases
            .insert(preparation.session_id, SessionRunPhase::Preparing);
    })
    .map(|_| ())?;
    if let Some(project_id) = boundary_project_id {
        begin_session_external_turn_lifecycle(controller_runtime, &project_id, preparation)?;
    }
    Ok(())
}

/// Begins the session lifecycle for one WebUI provider-stream turn.
/// 开始单个 WebUI provider 流式轮次的 会话生命周期。
fn begin_session_external_turn_lifecycle(
    controller_runtime: &ControllerRuntimeHandle,
    project_id: &str,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
    let Some(turn_id) = preparation
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.clone())
    else {
        return Ok(());
    };
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        &controller_runtime.data_root,
        project_id,
        preparation.session_id,
        &preparation.session_title,
    );
    let runner = SessionLifecycleRunner::new(store, WebuiSessionLifecycleHook);
    runner.begin_turn(SessionLifecycleBeginRequest {
        thread_id: SessionNodeId::from(preparation.session_id),
        turn_id,
        accepted_events: preparation.boundary_events.clone(),
    })
}

/// Finishes the session lifecycle for one WebUI provider-stream turn.
/// 结束单个 WebUI provider 流式轮次的 会话生命周期。
fn finish_session_external_turn_lifecycle(
    controller_runtime: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
    session_title: &str,
    final_events: Vec<SessionBoundaryEvent>,
    terminal_event: SessionBoundaryEvent,
) -> Result<(), String> {
    let Some(turn_id) = terminal_event.turn_id.clone() else {
        return Ok(());
    };
    let terminal_status = terminal_event
        .payload
        .get("status")
        .and_then(Value::as_str)
        .map(|status| {
            if status == "completed" {
                SessionTurnTerminalStatus::Completed
            } else {
                SessionTurnTerminalStatus::Failed
            }
        })
        .unwrap_or(SessionTurnTerminalStatus::Completed);
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        &controller_runtime.data_root,
        project_id,
        session_id,
        session_title,
    );
    let runner = SessionLifecycleRunner::new(store, WebuiSessionLifecycleHook);
    runner.finish_turn(SessionLifecycleFinishRequest {
        thread_id: SessionNodeId::from(session_id),
        turn_id,
        terminal_status,
        final_events,
        terminal_event: Some(terminal_event),
    })
}

/// Updates one session phase in the retained workspace snapshot.
/// 更新保留工作区快照中的单个会话阶段。
fn apply_session_phase(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    phase: SessionRunPhase,
) -> Result<(), String> {
    with_workspace_mutation(app, state, |workspace| {
        workspace.session_run_phases.insert(session_id, phase);
    })
    .map(|_| ())
}

/// Appends or extends one streaming transcript record.
/// 追加或扩展单条流式转录记录。
fn append_stream_delta_record(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_id_slot: &mut Option<String>,
    record_kind: SessionMessageRecordKind,
    title: &str,
    delta: &str,
    collapsed_by_default: bool,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        append_stream_delta_record_to_state(
            runtime_state,
            session_id,
            &now,
            record_id_slot,
            record_kind,
            title,
            delta,
            collapsed_by_default,
        );
        workspace.session_messages.insert(
            session_id,
            conversation_state_to_session_messages(&runtime_state.conversation_state),
        );
        workspace.session_run_phases.insert(
            session_id,
            match record_kind {
                SessionMessageRecordKind::ThinkingOutput => SessionRunPhase::Thinking,
                _ => SessionRunPhase::Requesting,
            },
        );
        workspace.hydrated_session_ids.insert(session_id);
    })
    .map(|_| ())
}

/// Keeps the thinking row collapsed after its streaming phase completes.
/// 在思考流阶段完成后保持思考行折叠。
fn collapse_streaming_thinking(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record_ids: &SessionStreamingRecordIds,
) -> Result<(), String> {
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        collapse_streaming_thinking_in_state(runtime_state, &now, record_ids);
    })
    .map(|_| ())
}

/// Projects runtime events that have user-facing transcript cards into the retained session state.
/// 将具备用户可见转录卡片的运行时事件投影到保留的会话状态中。
fn append_runtime_event_projection(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) {
        return Ok(());
    }
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        if append_runtime_event_projection_to_state(runtime_state, session_id, &now, event) {
            workspace.session_messages.insert(
                session_id,
                conversation_state_to_session_messages(&runtime_state.conversation_state),
            );
            workspace.hydrated_session_ids.insert(session_id);
        }
    })
    .map(|_| ())
}

/// Consumes one delegate runtime event into the WebUI subagent cache and child transcript state.
/// 将一条委派运行时事件吸收到 WebUI 子代理缓存与子会话转录状态中。
fn ingest_webui_subagent_runtime_event(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    _agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<bool, String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::DelegateStarted | RuntimeEventKind::DelegateFinished
    ) {
        return Ok(false);
    }
    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(true);
    };
    let prompt = parse_subagent_task_prompt_metadata(metadata);
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(true);
    };
    webui_trace(
        "subagent_runtime_event_ingest",
        format!(
            "parent={parent_session_id} task={} kind={:?} is_finished={is_finished} public_task_id={} label={}",
            panel_state.task_id, event.kind, panel_state.public_task_id, panel_state.subagent_label
        ),
    );
    let normalized_state = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref();
        let normalized = if is_finished {
            normalize_webui_subagent_task_state(workspace_snapshot, &panel_state, None, false)
        } else {
            panel_state.clone()
        };
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id);
        if is_finished {
            guard
                .detached_subagent_task_listeners
                .remove(&normalized.task_id);
        }
        guard
            .session_subagent_tasks
            .entry(parent_session_id)
            .or_default()
            .insert(normalized.task_id, normalized.clone());
        normalized
    };
    webui_trace(
        "subagent_runtime_event_normalized",
        format!(
            "parent={parent_session_id} task={} run_state={} has_conversation_state={} result_chars={}",
            normalized_state.task_id,
            subagent_run_state_key(normalized_state.run_state),
            normalized_state.conversation_state.is_some(),
            normalized_state.result_text.chars().count()
        ),
    );

    with_workspace_mutation(app, state, |workspace| {
        attach_webui_task_session_metadata_to_parent_tool_message(
            workspace,
            parent_session_id,
            metadata,
        );
        if let Some(prompt) = prompt.as_deref() {
            seed_webui_subagent_task_prompt_if_needed(workspace, normalized_state.task_id, prompt);
        }
        if let Some(conversation_state) = normalized_state.conversation_state.clone() {
            replace_webui_subagent_task_conversation_state(
                workspace,
                normalized_state.task_id,
                conversation_state,
            );
        }
    })?;
    if is_finished {
        webui_trace(
            "subagent_runtime_event_projection_finished",
            format!(
                "parent={parent_session_id} task={}",
                normalized_state.task_id
            ),
        );
    }
    Ok(true)
}

/// Seeds one child task transcript with its initial prompt when no child transcript exists yet.
/// 当子任务尚无转录时，使用初始提示词播种子任务转录。
fn seed_webui_subagent_task_prompt_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    prompt: &str,
) {
    if prompt.trim().is_empty()
        || workspace_snapshot
            .session_messages
            .get(&task_id)
            .is_some_and(|messages| !messages.is_empty())
    {
        return;
    }
    let now = current_unix_millis_text();
    workspace_snapshot.session_messages.insert(
        task_id,
        vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", prompt)
                .with_runtime_metadata(format!("session-{task_id}-subagent-prompt-{now}"), now),
        ],
    );
    workspace_snapshot.hydrated_session_ids.insert(task_id);
}

/// Appends the canonical stopped notice to a child task transcript when it is missing.
/// 当子任务转录缺少停止提示时追加规范停止提示。
fn append_webui_subagent_stop_notice_if_needed(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
) {
    let messages = workspace_snapshot
        .session_messages
        .entry(task_id)
        .or_default();
    if messages.iter().any(|message| {
        message.title == "执行已停止" && message.body.trim() == SUBAGENT_TASK_STOPPED_SUMMARY_TEXT
    }) {
        return;
    }
    let now = current_unix_millis_text();
    messages.push(
        SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "执行已停止",
            SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        )
        .with_runtime_metadata(format!("session-{task_id}-subagent-stopped-{now}"), now),
    );
    workspace_snapshot.hydrated_session_ids.insert(task_id);
}

/// Links one delegate task-session payload back onto the visible WebUI parent task card.
/// 将一份委派 task-session 载荷反挂回 WebUI 可见父级 task 卡片。
fn attach_webui_task_session_metadata_to_parent_tool_message(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    metadata: &Value,
) {
    let Some(payload) = task_session_payload_from_tool_metadata(metadata) else {
        return;
    };
    let message_ids = find_webui_task_session_tool_message_ids_for_payload(
        workspace_snapshot,
        parent_session_id,
        payload,
    );
    if message_ids.is_empty() {
        return;
    }
    let execution_status =
        task_session_execution_status_from_tool_metadata(metadata).unwrap_or("running");
    let metadata_update =
        attach_tool_display_state_to_metadata(Some(metadata.clone()), execution_status)
            .unwrap_or_else(|| metadata.clone());
    for message_id in message_ids {
        let _ = merge_webui_task_session_tool_message_metadata(
            workspace_snapshot,
            parent_session_id,
            &message_id,
            metadata_update.clone(),
            execution_status,
        );
    }
}

/// Shallow-merges task-session metadata into one WebUI parent tool-card record and projection.
/// 将 task-session 元数据浅合并进一条 WebUI 父级工具卡片记录与投影。
fn merge_webui_task_session_tool_message_metadata(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    message_id: &str,
    metadata_update: Value,
    execution_status: &str,
) -> bool {
    let current_metadata =
        webui_task_session_tool_message_metadata(workspace_snapshot, parent_session_id, message_id);
    let Some(updated_metadata) = merge_task_mirror_tool_display_metadata(
        current_metadata,
        task_session_display_metadata_update(metadata_update, execution_status),
        execution_status == "running",
    ) else {
        return false;
    };
    let mut updated = false;
    let updated_at = current_unix_millis_text();
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&parent_session_id)
    {
        for record in &mut runtime_state.conversation_state.records {
            if record.id == message_id {
                record.metadata = Some(updated_metadata.clone());
                repair_task_session_display_title(
                    &mut record.title,
                    record.record_kind,
                    execution_status,
                );
                runtime_state.updated_at = updated_at.clone();
                updated = true;
                break;
            }
        }
    }
    if let Some(messages) = workspace_snapshot
        .session_messages
        .get_mut(&parent_session_id)
    {
        for message in messages {
            if message.id == message_id {
                message.metadata = Some(updated_metadata.clone());
                repair_task_session_display_title(
                    &mut message.title,
                    message.record_kind,
                    execution_status,
                );
                updated = true;
                break;
            }
        }
    }
    updated
}

/// Applies lightweight persisted child-task mirror states to parent spawn-agent tool cards.
/// 将持久化轻量子任务镜像状态应用到父级 spawn_agent 工具卡片。
///
/// # Parameters
/// 参数
/// `controller_runtime` provides the data root used to read parent session metadata.
/// `controller_runtime` 提供读取父会话元数据所需的数据根目录。
/// `workspace_snapshot` owns the already hydrated parent transcript window.
/// `workspace_snapshot` 持有已经水合的父会话转录窗口。
/// `parent_session_id` identifies the parent session whose tool cards should be repaired.
/// `parent_session_id` 标识需要修正工具卡片的父会话。
/// `is_task_live_running` reports whether the task still has a real in-process runtime.
/// `is_task_live_running` 表示该任务是否仍有真实进程内运行态。
///
/// # Returns
/// 返回值
/// `Ok(())` after best-effort state repair; missing mirrors are treated as no-op.
/// 尽力修复状态后返回 `Ok(())`；缺少镜像时视为无操作。
fn apply_persisted_task_mirror_statuses_to_workspace<F>(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    mut is_task_live_running: F,
) -> Result<(), String>
where
    F: FnMut(SessionId) -> bool,
{
    let task_mirrors =
        vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors(
            &controller_runtime.data_root,
            parent_session_id,
        )?;
    if task_mirrors.is_empty() {
        return Ok(());
    }
    let Some(messages) = workspace_snapshot
        .session_messages
        .get_mut(&parent_session_id)
    else {
        return Ok(());
    };
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&parent_session_id);
    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        runtime_state,
        task_mirrors.as_slice(),
        |task_id| is_task_live_running(task_id),
    );
    Ok(())
}

/// Applies task mirror status rows to already loaded parent transcript messages.
/// 将任务镜像状态行应用到已加载的父级转录消息。
///
/// # Parameters
/// 参数
/// `messages` is the mutable projected parent transcript window.
/// `messages` 是可变的父级投影转录窗口。
/// `runtime_state` optionally carries canonical parent conversation records to repair in parallel.
/// `runtime_state` 可选携带需要同步修复的父级规范会话记录。
/// `task_mirrors` are lightweight parent metadata rows and never include child transcript bodies.
/// `task_mirrors` 是父元数据中的轻量行，不包含子任务转录正文。
/// `is_task_live_running` reports whether a task is truly running in the current process.
/// `is_task_live_running` 表示任务在当前进程中是否真实运行。
fn apply_task_mirror_statuses_to_parent_messages<F>(
    messages: &mut [SessionMessageEntry],
    mut runtime_state: Option<&mut SessionRuntimeStateEntry>,
    task_mirrors: &[SessionTaskMirrorSnapshot],
    mut is_task_live_running: F,
) where
    F: FnMut(SessionId) -> bool,
{
    for task_mirror in task_mirrors {
        let execution_status = normalized_task_mirror_execution_status(
            task_mirror.status.as_str(),
            is_task_live_running(task_mirror.task_session_id),
        );
        let payload = json!({
            "task_id": task_mirror.task_session_id.to_string(),
            "task_session_id": task_mirror.task_session_id.to_string(),
            "execution_status": execution_status,
            "line_count": task_mirror.line_count,
            "updated_at": task_mirror.updated_at,
        });
        let message_ids = find_task_session_tool_message_ids_for_payload(
            messages.iter(),
            runtime_state
                .as_deref()
                .map(|state| state.conversation_state.records.iter())
                .into_iter()
                .flatten(),
            &payload,
        );
        if message_ids.is_empty() {
            continue;
        }
        let metadata_update = task_session_display_metadata_update(
            json!({
                "tool_name": "spawn_agent",
                "kind": "task_session",
                "payload": payload,
            }),
            execution_status.as_str(),
        );
        for message_id in message_ids {
            for message in messages.iter_mut() {
                if message.id == message_id {
                    let Some(updated_metadata) = merge_task_mirror_tool_display_metadata(
                        message.metadata.clone(),
                        metadata_update.clone(),
                        execution_status == "running",
                    ) else {
                        continue;
                    };
                    message.metadata = Some(updated_metadata);
                    repair_task_session_display_title(
                        &mut message.title,
                        message.record_kind,
                        execution_status.as_str(),
                    );
                }
            }
            if let Some(runtime_state) = runtime_state.as_deref_mut() {
                for record in &mut runtime_state.conversation_state.records {
                    if record.id == message_id {
                        let Some(updated_metadata) = merge_task_mirror_tool_display_metadata(
                            record.metadata.clone(),
                            metadata_update.clone(),
                            execution_status == "running",
                        ) else {
                            continue;
                        };
                        record.metadata = Some(updated_metadata);
                        repair_task_session_display_title(
                            &mut record.title,
                            record.record_kind,
                            execution_status.as_str(),
                        );
                    }
                }
            }
        }
    }
}

/// Repairs one display-only surface window with lightweight parent task mirrors.
/// 使用轻量父级任务镜像修复一个仅显示用途的消息窗口。
///
/// # Parameters
/// 参数
/// `guard` provides controller and live task-runtime state.
/// `guard` 提供控制器与实时任务运行态。
/// `session_id` identifies the surface session whose messages are being displayed.
/// `session_id` 标识当前正在显示的界面会话。
/// `messages` is the projected message window to repair in place.
/// `messages` 是需要原地修复的投影消息窗口。
fn repair_task_mirror_statuses_for_surface_messages_locked(
    guard: &ServiceBridgeState,
    session_id: SessionId,
    messages: &mut [SessionMessageEntry],
) {
    if messages.is_empty() {
        return;
    }
    let Some(controller_runtime) = guard.controller_runtime.as_ref() else {
        return;
    };
    let Ok(task_mirrors) =
        vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors(
            &controller_runtime.data_root,
            session_id,
        )
    else {
        return;
    };
    if task_mirrors.is_empty() {
        return;
    }
    apply_task_mirror_statuses_to_parent_messages(
        messages,
        None,
        task_mirrors.as_slice(),
        |task_id| {
            webui_subagent_task_has_live_run(
                guard
                    .agent_runtime
                    .as_ref()
                    .map(|runtime| &runtime.task_runtime),
                guard.detached_subagent_task_listeners.contains(&task_id),
                task_id,
            )
        },
    );
}

/// Finds every parent task-card message id that belongs to one task-session payload.
/// 查找属于同一 task-session 载荷的所有父级任务卡片消息标识。
///
/// # Parameters
/// 参数
/// `messages` yields projected parent transcript messages.
/// `messages` 产出父级转录投影消息。
/// `records` yields canonical parent conversation records.
/// `records` 产出父级规范会话记录。
/// `payload` is the task-session payload used for matching.
/// `payload` 是用于匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// Stable set of every matching task-card message id.
/// 返回所有匹配任务卡片消息标识的稳定集合。
fn find_task_session_tool_message_ids_for_payload<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    records: impl Iterator<Item = &'a SessionConversationRecord>,
    payload: &Value,
) -> BTreeSet<String> {
    let mut message_ids = BTreeSet::new();
    for message in messages {
        if !task_session_record_kind_can_display_status(message.record_kind) {
            continue;
        }
        if message
            .metadata
            .as_ref()
            .is_some_and(|metadata| task_session_message_match_score(metadata, payload) > 0)
        {
            message_ids.insert(message.id.clone());
        }
    }
    for record in records {
        if !task_session_record_kind_can_display_status(record.record_kind) {
            continue;
        }
        if record
            .metadata
            .as_ref()
            .is_some_and(|metadata| task_session_message_match_score(metadata, payload) > 0)
        {
            message_ids.insert(record.id.clone());
        }
    }
    message_ids
}

/// Returns whether one record kind can be a visible task-session tool row.
/// 返回某记录类型是否可能是可见的 task-session 工具行。
fn task_session_record_kind_can_display_status(record_kind: SessionMessageRecordKind) -> bool {
    matches!(
        record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    )
}

/// Builds display metadata with every status alias kept in sync.
/// 构建显示元数据，并保持所有状态别名同步。
///
/// # Parameters
/// 参数
/// `metadata_update` is the base task-session metadata update.
/// `metadata_update` 是基础 task-session 元数据更新。
/// `execution_status` is the canonical runtime-derived display state.
/// `execution_status` 是运行时推导出的规范显示状态。
///
/// # Returns
/// 返回值
/// Metadata update with top-level and payload-level display states aligned.
/// 返回顶层与 payload 层显示状态已对齐的元数据更新。
fn task_session_display_metadata_update(
    mut metadata_update: Value,
    execution_status: &str,
) -> Value {
    let Some(object) = metadata_update.as_object_mut() else {
        return json!({
            "tool_name": "spawn_agent",
            "kind": "task_session",
            "display_state": execution_status,
            "tool_display_state": execution_status,
        });
    };
    object.insert(
        String::from("display_state"),
        Value::String(execution_status.to_string()),
    );
    object.insert(
        String::from("tool_display_state"),
        Value::String(execution_status.to_string()),
    );
    if let Some(payload) = object.get_mut("payload").and_then(Value::as_object_mut) {
        payload.insert(
            String::from("execution_status"),
            Value::String(execution_status.to_string()),
        );
    }
    if let Some(host_payload) = object
        .get_mut("host_result")
        .and_then(Value::as_object_mut)
        .and_then(|host_result| host_result.get_mut("payload"))
        .and_then(Value::as_object_mut)
    {
        host_payload.insert(
            String::from("execution_status"),
            Value::String(execution_status.to_string()),
        );
    }
    metadata_update
}

/// Repairs stale localized task-session titles without changing unrelated titles.
/// 修复陈旧的本地化 task-session 标题，同时不改动无关标题。
fn repair_task_session_display_title(
    title: &mut String,
    record_kind: SessionMessageRecordKind,
    execution_status: &str,
) {
    if !task_session_record_kind_can_display_status(record_kind) || !title.contains("spawn_agent") {
        return;
    }
    let status_label = match execution_status {
        "failed" => "失败",
        "stopped" => "已停止",
        "completed" => "已完成",
        _ => "进行中",
    };
    *title = format!("调用 spawn_agent 工具 {status_label}");
}

/// Merges task-mirror display metadata while allowing live running state to override stale terminal payloads.
/// 合并任务镜像展示元数据，并允许实时运行态覆盖陈旧终态载荷。
///
/// # Parameters
/// 参数
/// `current_metadata` is the metadata already attached to the parent tool card.
/// `current_metadata` 是父级工具卡片已有的元数据。
/// `metadata_update` is the task-mirror-derived display metadata.
/// `metadata_update` 是从任务镜像推导出的展示元数据。
/// `live_running_override` indicates that the host runtime still owns active task work.
/// `live_running_override` 表示宿主运行时仍持有活动任务。
///
/// # Returns
/// 返回值
/// Merged metadata for the parent tool card.
/// 返回父级工具卡片的合并后元数据。
fn merge_task_mirror_tool_display_metadata(
    current_metadata: Option<Value>,
    metadata_update: Value,
    live_running_override: bool,
) -> Option<Value> {
    if !live_running_override {
        return merge_tool_display_metadata(current_metadata, Some(metadata_update));
    }
    match (current_metadata, metadata_update) {
        (Some(Value::Object(mut current_object)), Value::Object(update_object)) => {
            current_object.extend(update_object);
            Some(Value::Object(current_object))
        }
        (_, update) => Some(update),
    }
}

/// Resolves display status for one task mirror, letting live runtime state win over stale metadata.
/// 为单条任务镜像解析展示状态，并让实时运行态覆盖陈旧元数据。
///
/// # Parameters
/// 参数
/// `status` is the status text mirrored in parent metadata.
/// `status` 是父级元数据镜像中的状态文本。
/// `is_live_running` tells whether the current process still owns active task work.
/// `is_live_running` 表示当前进程是否仍持有活动任务。
///
/// # Returns
/// 返回值
/// A canonical display execution status.
/// 规范化后的展示执行状态。
fn normalized_task_mirror_execution_status(status: &str, is_live_running: bool) -> String {
    if is_live_running {
        return String::from("running");
    }
    match status.trim().to_ascii_lowercase().as_str() {
        "failed" | "error" | "errored" => String::from("failed"),
        "stopped" | "cancelled" | "canceled" | "aborted" => String::from("stopped"),
        "completed" | "complete" | "success" | "succeeded" => String::from("completed"),
        "running" | "created" | "pending" | "in_progress" => String::from("completed"),
        _ => String::from("completed"),
    }
}

/// Returns a cloned metadata object for one WebUI task-session parent message.
/// 返回一条 WebUI task-session 父级消息的元数据克隆。
fn webui_task_session_tool_message_metadata(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    message_id: &str,
) -> Option<Value> {
    workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .and_then(|messages| messages.iter().find(|message| message.id == message_id))
        .and_then(|message| message.metadata.clone())
        .or_else(|| {
            workspace_snapshot
                .session_runtime_state
                .get(&parent_session_id)
                .and_then(|runtime_state| {
                    runtime_state
                        .conversation_state
                        .records
                        .iter()
                        .find(|record| record.id == message_id)
                        .and_then(|record| record.metadata.clone())
                })
        })
}

/// Marks one WebUI subagent task as stopped in the child transcript and parent task card.
/// 在子转录与父级 task 卡片中将单个 WebUI 子代理任务标记为已停止。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the retained WebUI workspace snapshot to mutate.
/// `workspace_snapshot` 是需要变更的 WebUI 保留工作区快照。
/// `parent_session_id` owns the visible task card.
/// `parent_session_id` 拥有可见 task 卡片。
/// `task_id` is the child task session identifier.
/// `task_id` 是子任务会话标识。
/// `state_snapshot` is the latest cached subagent state when available.
/// `state_snapshot` 是可用时最近缓存的子代理状态。
fn mark_webui_subagent_task_stopped_in_workspace(
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_id: SessionId,
    state_snapshot: Option<&SubagentTaskPanelState>,
) {
    append_webui_subagent_stop_notice_if_needed(workspace_snapshot, task_id);
    let payload = json!({
        "task_id": task_id.to_string(),
        "task_session_id": task_id.to_string(),
    });
    let Some(message_id) = find_webui_task_session_tool_message_id_for_payload(
        workspace_snapshot,
        parent_session_id,
        &payload,
    ) else {
        return;
    };
    let current_metadata = webui_task_session_tool_message_metadata(
        workspace_snapshot,
        parent_session_id,
        &message_id,
    );
    let updated_at = current_unix_millis_text();
    let updated_metadata = build_stopped_task_session_metadata(
        current_metadata.as_ref(),
        state_snapshot,
        task_id,
        SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        &updated_at,
    );
    let _ = merge_webui_task_session_tool_message_metadata(
        workspace_snapshot,
        parent_session_id,
        &message_id,
        updated_metadata,
        "stopped",
    );
}

/// Finds the best parent task-card message id for a WebUI task-session payload.
/// 为 WebUI task-session 载荷查找最匹配的父级 task 卡片消息标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides projected messages and canonical runtime records.
/// `workspace_snapshot` 提供投影消息与规范运行时记录。
/// `parent_session_id` selects the parent session to scan.
/// `parent_session_id` 选择需要扫描的父会话。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// The best matching message id when one can be found.
/// 找到匹配项时返回最佳消息标识。
fn find_webui_task_session_tool_message_id_for_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    payload: &Value,
) -> Option<String> {
    let messages = workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|messages| messages.iter().rev());
    let records = workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|runtime_state| runtime_state.conversation_state.records.iter().rev());
    find_task_session_tool_message_id_for_payload(messages, records, payload)
}

/// Finds every WebUI parent task-card message id for one task-session payload.
/// 为单个 task-session 载荷查找所有 WebUI 父级任务卡片消息标识。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` provides projected messages and canonical runtime records.
/// `workspace_snapshot` 提供投影消息与规范运行时记录。
/// `parent_session_id` selects the parent session to scan.
/// `parent_session_id` 选择需要扫描的父会话。
/// `payload` is the task-session payload to match.
/// `payload` 是需要匹配的 task-session 载荷。
///
/// # Returns
/// 返回值
/// Stable set of every matching WebUI parent task-card message id.
/// 返回所有匹配 WebUI 父级任务卡片消息标识的稳定集合。
fn find_webui_task_session_tool_message_ids_for_payload(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    payload: &Value,
) -> BTreeSet<String> {
    let messages = workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|messages| messages.iter().rev());
    let records = workspace_snapshot
        .session_runtime_state
        .get(&parent_session_id)
        .into_iter()
        .flat_map(|runtime_state| runtime_state.conversation_state.records.iter().rev());
    find_task_session_tool_message_ids_for_payload(messages, records, payload)
}

/// Replaces one child task transcript with the canonical conversation returned by the subagent.
/// 使用子代理返回的规范会话替换单个子任务转录。
fn replace_webui_subagent_task_conversation_state(
    workspace_snapshot: &mut WorkspaceSnapshot,
    task_id: SessionId,
    conversation_state: vulcan_agent_runtime::conversation_protocol::SessionConversationState,
) {
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(task_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let preserved_records = runtime_state.conversation_state.records.clone();
    runtime_state.conversation_state = conversation_state;
    if !preserved_records.is_empty() {
        runtime_state.conversation_state.records = preserved_records;
    }
    runtime_state.updated_at = now;
    workspace_snapshot.session_messages.insert(
        task_id,
        conversation_state_to_session_messages(&runtime_state.conversation_state),
    );
    workspace_snapshot.hydrated_session_ids.insert(task_id);
}

/// Hides the selected parent subagent navigation round once iced-equivalent blocking work has settled.
/// 当与 iced 等价的阻塞工作全部收敛后，隐藏该轮子代理导航。
fn refresh_webui_subagent_navigation_round_visibility(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let has_running =
        !running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, parent_session_id)
            .is_empty();
    let has_active_stream_task = guard.active_stream_tasks.contains_key(&parent_session_id);
    let has_active_kernel_turn = webui_session_kernel_has_active_turn(&guard, parent_session_id);
    let has_active_phase = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace_snapshot| {
            workspace_snapshot
                .session_run_phases
                .get(&parent_session_id)
                .copied()
        })
        .is_some_and(is_active_session_run_phase);
    let should_remain_visible = subagent_navigation_round_should_remain_visible(
        has_active_stream_task || has_active_kernel_turn || has_active_phase,
        has_running,
        false,
    );
    webui_trace(
        "subagent_navigation_round_visibility",
        format!(
            "parent={parent_session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_phase={has_active_phase} has_running={has_running} should_remain_visible={should_remain_visible}"
        ),
    );
    if !should_remain_visible {
        guard
            .session_subagent_navigation_rounds
            .remove(&parent_session_id);
    }
    Ok(())
}

/// Applies one safe canonical conversation checkpoint and persists it off the stream path.
/// 应用一份安全规范会话检查点，并在流式关键路径外持久化。
fn apply_conversation_checkpoint(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: vulcan_agent_runtime::conversation_protocol::SessionConversationState,
    last_successful_provider_usage: Option<vulcan_core::types::ProviderUsageSnapshot>,
    provider_request_count: usize,
) -> Result<(), String> {
    let mut is_main_session = false;
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let now = current_unix_millis_text();
            // Only main-session checkpoints may persist through workspace-store; TaskSession checkpoints stay in memory here.
            // 只有主会话 checkpoint 可以通过 workspace-store 持久化；TaskSession checkpoint 在这里仅更新内存显示态。
            is_main_session = workspace
                .sessions
                .iter()
                .any(|candidate| candidate.id == session_id);
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            apply_checkpoint_conversation_state(runtime_state, conversation_state, &now);
            if let Some(usage) = last_successful_provider_usage {
                runtime_state.last_provider_usage = Some(usage.clone());
                runtime_state.last_successful_provider_usage = Some(usage);
            }
            if provider_request_count > 0 {
                runtime_state.last_safe_provider_request_count = Some(provider_request_count);
            }
            workspace.session_messages.insert(
                session_id,
                conversation_state_to_session_messages(&runtime_state.conversation_state),
            );
            workspace.hydrated_session_ids.insert(session_id);
        })?;
    if is_main_session {
        spawn_workspace_session_runtime_state_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
            Some(String::from("conversation_checkpoint")),
        );
    }
    Ok(())
}

/// Result produced after applying one terminal report to retained memory.
/// 将一份终态报告应用到保留内存后产生的结果。
struct TerminalReportMemoryApply {
    /// Updated workspace snapshot after applying the terminal report.
    /// 应用终态报告后的更新工作区快照。
    workspace_snapshot: WorkspaceSnapshot,
    /// Completed-turn hook request that should run asynchronously.
    /// 需要异步执行的完成回合 hook 请求。
    completion_request: Option<ConversationCompletionRequest>,
}

/// Applies a TaskSession terminal report without rebuilding the full WebUI workbench.
/// 在不重建完整 WebUI workbench 的情况下应用 TaskSession 终态报告。
///
/// # Parameters
/// 参数
/// `state` is the shared Tauri service bridge state that owns the in-memory workspace snapshot.
/// `state` 是持有内存工作区快照的共享 Tauri 服务桥状态。
/// `report` is the completed child execution report.
/// `report` 是已完成的子执行报告。
///
/// # Returns
/// 返回值
/// Returns the updated workspace snapshot and no hook completion request.
/// 返回更新后的工作区快照，且不产生 hook 完成请求。
fn apply_task_session_execution_report_to_memory(
    state: &SharedServiceBridgeState,
    report: ExecutionRunReport,
) -> Result<TerminalReportMemoryApply, String> {
    let (session_id, is_success) =
        apply_task_session_execution_report_to_memory_in_place(state, &report)?;
    let workspace_snapshot = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    webui_trace(
        "execution_report_applied_task_session_fast",
        format!(
            "session={session_id} error={} provider_requests={} output_chars={}",
            !is_success,
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok(TerminalReportMemoryApply {
        workspace_snapshot,
        completion_request: None,
    })
}

/// Applies a TaskSession terminal report in place without rebuilding the full workspace snapshot.
/// 原地应用 TaskSession 终态报告，不重建完整工作区快照。
fn apply_task_session_execution_report_to_memory_in_place(
    state: &SharedServiceBridgeState,
    report: &ExecutionRunReport,
) -> Result<(SessionId, bool), String> {
    let session_id = report.session_id;
    let is_success = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let now = current_unix_millis_text();
        let (is_success, conversation_state) = {
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            let apply_result =
                apply_terminal_execution_report_to_state(runtime_state, session_id, &now, &report);
            (
                apply_result.is_success,
                runtime_state.conversation_state.clone(),
            )
        };
        workspace.session_messages.insert(
            session_id,
            conversation_state_to_session_messages(&conversation_state),
        );
        workspace.session_run_phases.insert(
            session_id,
            if is_success {
                SessionRunPhase::Ready
            } else {
                SessionRunPhase::Failed
            },
        );
        workspace.hydrated_session_ids.insert(session_id);
        is_success
    };
    webui_trace(
        "execution_report_applied_task_session_in_place",
        format!(
            "session={session_id} error={} provider_requests={} output_chars={}",
            !is_success,
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok((session_id, is_success))
}

/// Applies the terminal execution report to retained memory and emits the updated workbench view.
/// 将终态执行报告应用到保留内存中，并发出更新后的工作台视图。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit the refreshed workbench snapshot.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot mutated by the stream task.
/// `state` 持有由流任务修改的保留工作区快照。
/// `agent_runtime` provides conversation hooks and VMM synchronization state.
/// `agent_runtime` 提供会话 hook 与 VMM 同步状态。
/// `report` is the terminal execution report produced by the agent runtime.
/// `report` 是 Agent 运行时产出的终态执行报告。
///
/// # Returns
/// 返回值
/// The updated workspace snapshot after the terminal report has been applied in memory.
/// 返回终态报告已经应用到内存后的更新工作区快照。
fn apply_execution_report_to_memory(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    report: ExecutionRunReport,
) -> Result<TerminalReportMemoryApply, String> {
    let session_id = report.session_id;
    let report_is_regular_session = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|workspace_snapshot| {
                workspace_snapshot_contains_regular_session(workspace_snapshot, session_id)
            })
    };
    if !report_is_regular_session {
        return apply_task_session_execution_report_to_memory(state, report);
    }
    let prompt_purpose = report
        .preparation
        .as_ref()
        .map(|preparation| preparation.prompt_purpose)
        .unwrap_or(ExecutionPromptPurpose::UserTurn);
    let runs_completed_turn_hooks = prompt_purpose.runs_completed_turn_hooks();
    // Measures completion-gate inspection for active hooks and descendant subagents.
    // 衡量完成闸门对活跃 hook 与后代子代理的检查耗时。
    let completion_gate_started_at = current_unix_millis_value();
    let completion_gate = webui_turn_completion_gate_from_state(state, agent_runtime, session_id)?;
    webui_trace_if_slow(
        "execution_report_completion_gate_slow",
        completion_gate_started_at,
        format!("session={session_id}"),
    );
    let mut completion_request = None;
    // Measures in-memory workspace mutation for the terminal execution report.
    // 衡量终态执行报告写入内存工作区的耗时。
    let workspace_apply_started_at = current_unix_millis_value();
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let now = current_unix_millis_text();
            // Main-session membership decides whether completion hooks and VMM side effects may run.
            // 主会话成员关系决定完成 hook 与 VMM 副作用是否允许执行。
            let is_main_session = workspace
                .sessions
                .iter()
                .any(|candidate| candidate.id == session_id);
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            let apply_result =
                apply_terminal_execution_report_to_state(runtime_state, session_id, &now, &report);
            let project_id = workspace
                .sessions
                .iter()
                .find(|candidate| candidate.id == session_id)
                .and_then(|session| session.project_id.clone());
            let conversation_state = runtime_state.conversation_state.clone();
            let should_dispatch_completion_hook = is_main_session
                && apply_result.is_success
                && (runs_completed_turn_hooks || completion_gate.has_active_hook_turn);
            let should_terminate_completion_hook = is_main_session
                && !apply_result.is_success
                && (runs_completed_turn_hooks || completion_gate.has_active_hook_turn);
            let completion_status = completion_gate.completion_status();
            if should_dispatch_completion_hook {
                let hook_turn_id =
                    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
                        let (hook_turn_id, request) = hook_runtime
                        .prepare_turn_completion_dispatch_with_status(
                        session_id,
                        project_id.clone(),
                        report.assistant_output.as_deref().unwrap_or(
                            "The model completed the run but did not return visible text output.",
                        ),
                        report.provider_request_count,
                        &conversation_state,
                        completion_status,
                        completion_gate.pending_subagent_count,
                    );
                        completion_request = Some(request);
                        hook_turn_id
                    } else {
                        String::new()
                    };
                if completion_status.is_final() {
                    runtime_state
                        .context_injection_state
                        .consume_turn(&hook_turn_id, &now);
                    runtime_state.vmm_profile_sync.record_completed_turn();
                    let profile_sync = runtime_state.vmm_profile_sync.clone();
                    agent_runtime
                        .vmm_hook_runtime
                        .store_profile_sync_state(session_id, profile_sync);
                }
            } else if should_terminate_completion_hook
                && let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock()
            {
                hook_runtime.terminate_turn(
                    session_id,
                    project_id,
                    "execution_failed",
                    report.provider_request_count,
                );
            }
            workspace.session_messages.insert(
                session_id,
                conversation_state_to_session_messages(&runtime_state.conversation_state),
            );
            if let Some(session) = workspace
                .sessions
                .iter_mut()
                .find(|candidate| candidate.id == session_id)
            {
                session.status = if apply_result.is_success {
                    SessionState::Completed
                } else {
                    SessionState::Blocked
                };
                session.last_updated_label = String::from("刚刚");
            }
            if workspace.selected_session != session_id {
                runtime_state.ui_state.unread = true;
            }
            workspace.session_run_phases.insert(
                session_id,
                if apply_result.is_success {
                    SessionRunPhase::Ready
                } else {
                    SessionRunPhase::Failed
                },
            );
            workspace.hydrated_session_ids.insert(session_id);
            if is_main_session {
                agent_runtime
                    .vmm_hook_runtime
                    .replace_snapshot(build_vmm_hook_config_snapshot(
                        &agent_runtime.provider_configuration,
                        workspace,
                    ));
            }
        })?;
    webui_trace_if_slow(
        "execution_report_workspace_apply_slow",
        workspace_apply_started_at,
        format!("session={session_id}"),
    );
    webui_trace(
        "execution_report_applied",
        format!(
            "session={session_id} is_main_session={} prompt_purpose={} runs_completed_turn_hooks={} completion_status={} pending_subagents={} has_active_hook_turn={} error={} provider_requests={} output_chars={}",
            workspace_snapshot
                .sessions
                .iter()
                .any(|candidate| candidate.id == session_id),
            prompt_purpose.diagnostics_label(),
            runs_completed_turn_hooks,
            completion_gate.completion_status().as_str(),
            completion_gate.pending_subagent_count,
            completion_gate.has_active_hook_turn,
            report.error.is_some(),
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok(TerminalReportMemoryApply {
        workspace_snapshot,
        completion_request,
    })
}

/// Spawns the completed-turn hook without blocking queued prompt dispatch.
/// 启动完成回合 hook，避免阻塞排队提示词分发。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit any later VMM outbox updates.
/// `app` 是用于发出后续 VMM outbox 更新的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot that receives hook side effects.
/// `state` 持有接收 hook 副作用的保留工作区快照。
/// `controller_runtime` persists VMM outbox changes after the hook finishes.
/// `controller_runtime` 在 hook 完成后持久化 VMM outbox 变更。
/// `agent_runtime` provides VMM hook snapshot refresh state.
/// `agent_runtime` 提供 VMM hook 快照刷新状态。
/// `completion_request` is the prepared completed-turn hook dispatch request.
/// `completion_request` 是已准备的完成回合 hook 调度请求。
fn spawn_conversation_completion_hook_task(
    app: tauri::AppHandle,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    completion_request: ConversationCompletionRequest,
) {
    tauri::async_runtime::spawn(async move {
        let outcome = completion_request.run().await;
        let session_id = outcome.session_id;
        if let Err(error) = outcome.report.as_ref() {
            eprintln!(
                "[tauri-service] completed-turn hook failed for {}: {}",
                outcome.turn_id, error
            );
        }
        if outcome.vmm_writeback_effects.is_empty() {
            return;
        }
        let workspace_snapshot = match with_workspace_mutation(&app, &state, |workspace| {
            let now = current_unix_millis_text();
            if let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) {
                apply_vmm_writeback_effects_to_runtime_state(
                    runtime_state,
                    outcome.vmm_writeback_effects,
                    &now,
                );
                runtime_state.updated_at = now;
            }
            agent_runtime
                .vmm_hook_runtime
                .replace_snapshot(build_vmm_hook_config_snapshot(
                    &agent_runtime.provider_configuration,
                    workspace,
                ));
        }) {
            Ok(workspace_snapshot) => workspace_snapshot,
            Err(error) => {
                eprintln!("[tauri-service] apply completed-turn hook effects failed: {error}");
                return;
            }
        };
        if let Err(error) = persist_workspace_session_update(
            Some(&state),
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            eprintln!("[tauri-service] persist completed-turn hook effects failed: {error}");
        }
    });
}

/// Applies VMM writeback hook effects to one session runtime state.
/// 将 VMM 写回 hook 效果应用到单个会话运行态。
fn apply_vmm_writeback_effects_to_runtime_state(
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

/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot.
/// Controls whether one workbench refresh rebuilds structural subagent navigation.
/// 控制一次工作台刷新是否重建结构性的子代理导航。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubagentNavigationRefreshMode {
    /// Rebuild subagent navigation from runtime and segmented state.
    /// 从运行时和分段状态重建子代理导航。
    Rebuild,
    /// Preserve the current subagent navigation while refreshing transcript content.
    /// 刷新转录内容时保留当前子代理导航。
    Preserve,
}

/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot.
/// 修改保留的工作区快照并发出重建后的工作台快照。
fn with_workspace_mutation<F>(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    with_workspace_mutation_with_subagent_navigation_mode(
        app,
        state,
        SubagentNavigationRefreshMode::Rebuild,
        mutate,
    )
}

/// Mutates the retained workspace snapshot without rebuilding subagent task navigation.
/// 修改保留的工作区快照但不重建子代理任务导航。
fn with_workspace_mutation_preserving_subagent_navigation<F>(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    with_workspace_mutation_with_subagent_navigation_mode(
        app,
        state,
        SubagentNavigationRefreshMode::Preserve,
        mutate,
    )
}

/// Mutates the retained workspace snapshot and emits a rebuilt workbench snapshot using the requested navigation mode.
/// 按指定导航模式修改保留的工作区快照并发出重建后的工作台快照。
fn with_workspace_mutation_with_subagent_navigation_mode<F>(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    subagent_navigation_refresh_mode: SubagentNavigationRefreshMode,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    let trace_started_at = current_unix_millis_value();
    let (
        workspace_snapshot,
        rebuild_generation,
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        surface_messages,
        subagent_navigation,
        logs,
        live_context_budgets,
    ) = {
        let trace_lock_wait_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        webui_trace_if_slow(
            "workspace_mutation_lock_wait",
            trace_lock_wait_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_prepare_started_at = current_unix_millis_value();
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let mut workspace_snapshot = guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        webui_trace_if_slow(
            "workspace_mutation_prepare",
            trace_prepare_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_mutate_started_at = current_unix_millis_value();
        mutate(&mut workspace_snapshot);
        webui_trace_if_slow(
            "workspace_mutation_apply",
            trace_mutate_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_surface_started_at = current_unix_millis_value();
        let (surface_messages, subagent_navigation) = match subagent_navigation_refresh_mode {
            SubagentNavigationRefreshMode::Rebuild => (
                active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
                subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            ),
            SubagentNavigationRefreshMode::Preserve => {
                let subagent_navigation = guard.workbench.subagent_navigation.clone();
                let active_session_id = preserved_active_surface_session_id(
                    &workspace_snapshot,
                    subagent_navigation.as_ref(),
                );
                (
                    surface_messages_for_session_without_subagent_navigation_refresh(
                        &guard,
                        &workspace_snapshot,
                        active_session_id,
                    ),
                    subagent_navigation,
                )
            }
        };
        webui_trace_if_slow(
            "workspace_mutation_surface",
            trace_surface_started_at,
            format!("mode={subagent_navigation_refresh_mode:?}"),
        );
        let trace_workbench_started_at = current_unix_millis_value();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let rebuild_generation = guard.workbench_generation;
        let returned_workspace_snapshot = workspace_snapshot.clone();
        guard.workspace_snapshot = Some(workspace_snapshot);
        webui_trace_if_slow(
            "workspace_mutation_commit_workspace",
            trace_workbench_started_at,
            format!("mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation}"),
        );
        (
            returned_workspace_snapshot,
            rebuild_generation,
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            provider_configuration,
            ai_tool_configuration,
            surface_messages,
            subagent_navigation,
            logs,
            live_context_budgets,
        )
    };
    let trace_workbench_started_at = current_unix_millis_value();
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    webui_trace_if_slow(
        "workspace_mutation_workbench",
        trace_workbench_started_at,
        format!("mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation}"),
    );
    let should_emit = {
        let trace_commit_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let is_current_generation = guard.workbench_generation == rebuild_generation;
        if is_current_generation {
            guard.workbench = workbench.clone();
        }
        webui_trace_if_slow(
            "workspace_mutation_commit_workbench",
            trace_commit_started_at,
            format!(
                "mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation} current={is_current_generation}"
            ),
        );
        is_current_generation
    };
    if should_emit {
        let trace_emit_started_at = current_unix_millis_value();
        let _ = app.emit(
            "vulcan://service-event",
            ServiceEvent::Workbench(Box::new(workbench)),
        );
        webui_trace_if_slow(
            "workspace_mutation_emit",
            trace_emit_started_at,
            format!("mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation}"),
        );
    } else {
        webui_trace(
            "workspace_mutation_emit_skipped",
            format!(
                "mode={subagent_navigation_refresh_mode:?} generation={rebuild_generation} reason=stale_generation"
            ),
        );
    }
    webui_trace_if_slow(
        "workspace_mutation_total",
        trace_started_at,
        format!("mode={subagent_navigation_refresh_mode:?}"),
    );
    Ok(workspace_snapshot)
}

/// Returns the selected session messages from runtime state before falling back to persisted rows.
/// 优先从运行时状态返回当前选中会话消息，缺失时回退到持久化行。
fn selected_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<Vec<SessionMessageEntry>> {
    in_memory_session_messages_from_workspace(
        workspace_snapshot,
        workspace_snapshot.selected_session,
    )
}

/// Normalizes one subagent task state against the shared task runtime without disk fallback scans.
/// 根据共享任务运行时归一化单条子代理任务状态，且不执行磁盘回退扫描。
fn normalize_webui_subagent_task_state(
    _workspace_snapshot: Option<&WorkspaceSnapshot>,
    state: &SubagentTaskPanelState,
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
) -> SubagentTaskPanelState {
    let is_live_running =
        webui_subagent_task_has_live_run(task_runtime, has_live_listener, state.task_id);
    normalize_subagent_task_panel_state(state, is_live_running, None)
}

/// Returns whether one child task still has a live WebUI listener or shared runtime abort handle.
/// 返回单个子任务是否仍拥有 WebUI 实时监听器或共享运行时中止句柄。
fn webui_subagent_task_has_live_run(
    task_runtime: Option<&Arc<TaskRuntime>>,
    has_live_listener: bool,
    task_id: SessionId,
) -> bool {
    has_live_listener
        || task_runtime.is_some_and(|runtime| runtime.is_task_running(task_id).unwrap_or(false))
}

/// Builds the shared segmented TaskSession store for one direct WebUI parent when its selected root session belongs to a project.
/// 当 WebUI 直接父级所属的选中根会话属于项目时构建共享分段 TaskSession 存储。
fn webui_task_session_store_for_parent_session(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
) -> Option<SegmentedTaskSessionStore> {
    let (root_session_id, project_id) = if let Some(project_id) =
        owning_project_id_for_session(workspace_snapshot, parent_session_id)
    {
        (parent_session_id, project_id)
    } else {
        let root_session_id = workspace_snapshot.selected_session;
        if root_session_id == SessionId::nil() {
            return None;
        }
        let project_id = owning_project_id_for_session(workspace_snapshot, root_session_id)?;
        (root_session_id, project_id)
    };
    let session_dir =
        vulcan_workspace_store::workspace_store::find_segmented_session_directory_for_task_store(
            &vulcan_workspace_store::data_plane::application_data_root(),
            &project_id,
            &root_session_id.to_string(),
        )
        .ok()
        .flatten()?;
    Some(SegmentedTaskSessionStore::new(session_dir))
}

/// Loads one WebUI subagent task detail from the shared segmented store without scanning sibling task details.
/// 从共享分段存储加载单个 WebUI 子代理详情，避免扫描兄弟任务详情。
fn segmented_webui_subagent_task_panel_state_for_task(
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Option<SubagentTaskPanelState> {
    // Measures single-task segmented detail lookup without sibling scans.
    // 衡量不扫描兄弟任务的单任务分段详情读取耗时。
    let detail_lookup_started_at = current_unix_millis_value();
    let Some(store) =
        webui_task_session_store_for_parent_session(workspace_snapshot, parent_session_id)
    else {
        webui_trace_if_slow(
            "subagent_detail_lookup_slow",
            detail_lookup_started_at,
            format!("parent={parent_session_id} task={task_session_id} found=false no_store=true"),
        );
        return None;
    };
    let result = subagent_task_panel_state_detail_from_segmented_task_session_store(
        &store,
        SessionNodeId::from(task_session_id),
    )
    .filter(|state| state.parent_session_id == parent_session_id);
    webui_trace_if_slow(
        "subagent_detail_lookup_slow",
        detail_lookup_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} found={}",
            result.is_some()
        ),
    );
    result
}

/// Finds one cached WebUI subagent task state by child task identifier across direct-parent buckets.
/// 跨直接父级缓存桶按子任务标识查找一条 WebUI 子代理任务状态。
fn webui_subagent_task_state_by_task_id(
    guard: &ServiceBridgeState,
    task_id: SessionId,
) -> Option<&SubagentTaskPanelState> {
    guard
        .session_subagent_tasks
        .values()
        .find_map(|tasks| tasks.get(&task_id))
}

/// Merges preloaded segmented task states into the in-memory WebUI subagent cache.
/// 将预加载的分段任务状态合并到 WebUI 内存子代理缓存。
fn merge_webui_subagent_task_cache_for_session(
    guard: &mut ServiceBridgeState,
    parent_session_id: SessionId,
    segmented_states: Vec<SubagentTaskPanelState>,
) {
    if parent_session_id == SessionId::nil() {
        return;
    }
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
    let previous_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .cloned()
        .unwrap_or_default();
    let merged_states =
        merge_subagent_task_panel_cache(&previous_states, segmented_states, |task_id| {
            webui_subagent_task_has_live_run(
                task_runtime,
                live_listener_task_ids.contains(&task_id),
                task_id,
            )
        });
    if merged_states.is_empty() {
        guard.session_subagent_tasks.remove(&parent_session_id);
    } else {
        guard
            .session_subagent_tasks
            .insert(parent_session_id, merged_states);
    }
    if let Some(selection) = guard.active_subagent_panel
        && selection.parent_session_id == parent_session_id
        && !guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .is_some_and(|tasks| tasks.contains_key(&selection.task_id))
    {
        guard.active_subagent_panel = None;
    }
}

/// Stores one loaded subagent detail in the service cache and mirrors its messages into the workspace snapshot.
/// 将单个已加载子代理详情写入服务缓存，并把它的消息镜像进工作区快照。
fn cache_webui_subagent_surface_detail_for_task(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_state: SubagentTaskPanelState,
) {
    let now = current_unix_millis_value();
    prune_webui_subagent_surface_detail_cache(guard, now);
    if let Some(conversation_state) = task_state.conversation_state.clone() {
        replace_webui_subagent_task_conversation_state(
            workspace_snapshot,
            task_state.task_id,
            conversation_state,
        );
    }
    let messages =
        in_memory_session_messages_from_workspace(workspace_snapshot, task_state.task_id)
            .unwrap_or_else(|| {
                task_state
                    .conversation_state
                    .as_ref()
                    .map(conversation_state_to_session_messages)
                    .unwrap_or_default()
            });
    guard.subagent_surface_detail_cache.insert(
        task_state.task_id,
        WebuiSubagentSurfaceDetailCacheEntry {
            parent_session_id,
            task_state,
            messages,
            last_access_ms: now,
        },
    );
    prune_webui_subagent_surface_detail_cache(guard, now);
}

/// Restores one loaded subagent detail cache entry into the workspace and task-panel caches.
/// 将单个已加载子代理详情缓存恢复进工作区与任务面板缓存。
fn restore_webui_subagent_surface_detail_cache_for_task(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &mut WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> bool {
    let now = current_unix_millis_value();
    prune_webui_subagent_surface_detail_cache(guard, now);
    let Some(entry) = guard
        .subagent_surface_detail_cache
        .get_mut(&task_session_id)
        .filter(|entry| entry.parent_session_id == parent_session_id)
    else {
        return false;
    };
    entry.last_access_ms = now;
    if let Some(conversation_state) = entry.task_state.conversation_state.clone() {
        replace_webui_subagent_task_conversation_state(
            workspace_snapshot,
            task_session_id,
            conversation_state,
        );
    } else if !entry.messages.is_empty() {
        workspace_snapshot
            .session_messages
            .insert(task_session_id, entry.messages.clone());
        workspace_snapshot
            .hydrated_session_ids
            .insert(task_session_id);
    }
    guard
        .session_subagent_tasks
        .entry(parent_session_id)
        .or_default()
        .insert(task_session_id, entry.task_state.clone());
    true
}

/// Removes stale or excessive loaded subagent detail cache entries.
/// 移除过期或超量的已加载子代理详情缓存项。
fn prune_webui_subagent_surface_detail_cache(guard: &mut ServiceBridgeState, now_ms: i128) {
    guard.subagent_surface_detail_cache.retain(|_, entry| {
        now_ms.saturating_sub(entry.last_access_ms) <= SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS
    });
    if guard.subagent_surface_detail_cache.len() <= SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT {
        return;
    }
    let mut entries_by_age = guard
        .subagent_surface_detail_cache
        .iter()
        .map(|(task_id, entry)| (*task_id, entry.last_access_ms))
        .collect::<Vec<_>>();
    entries_by_age.sort_by_key(|(_, last_access_ms)| *last_access_ms);
    let overflow = guard
        .subagent_surface_detail_cache
        .len()
        .saturating_sub(SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT);
    for (task_id, _) in entries_by_age.into_iter().take(overflow) {
        guard.subagent_surface_detail_cache.remove(&task_id);
    }
}

/// Returns the currently active transcript surface session id for the WebUI.
/// 返回 WebUI 当前活动转录界面的会话标识。
fn active_surface_session_id_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> SessionId {
    let root_session_id = workspace_snapshot.selected_session;
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id != root_session_id)
    {
        guard.active_subagent_panel = None;
    }
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&root_session_id);
    let has_active_panel = guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == root_session_id);
    if !has_visible_round && !has_active_panel {
        return root_session_id;
    }
    if let Some(selection) = guard.active_subagent_panel
        && let Some(state) = guard
            .session_subagent_tasks
            .get(&selection.parent_session_id)
            .and_then(|tasks| tasks.get(&selection.task_id))
    {
        let task_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| &runtime.task_runtime);
        let has_live_listener = guard
            .detached_subagent_task_listeners
            .contains(&state.task_id);
        let normalized = normalize_webui_subagent_task_state(
            Some(workspace_snapshot),
            state,
            task_runtime,
            has_live_listener,
        );
        if subagent_task_panel_is_openable(
            &normalized,
            webui_subagent_panel_has_recoverable_detail(workspace_snapshot, &normalized),
        ) {
            return selection.task_id;
        }
    }
    guard.active_subagent_panel = None;
    root_session_id
}

/// Returns transcript messages for the currently active WebUI surface.
/// 返回 WebUI 当前活动界面的转录消息。
fn active_surface_messages_from_workspace_and_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Vec<SessionMessageEntry> {
    let active_session_id = active_surface_session_id_from_state(guard, workspace_snapshot);
    if active_session_id != workspace_snapshot.selected_session
        && let Some(messages) =
            in_memory_session_messages_from_workspace(workspace_snapshot, active_session_id)
        && !messages.is_empty()
    {
        return messages;
    }
    if active_session_id != workspace_snapshot.selected_session
        && let Some(state) = webui_subagent_task_state_by_task_id(guard, active_session_id)
        && let Some(conversation_state) = state.conversation_state.as_ref()
    {
        return conversation_state_to_session_messages(conversation_state);
    }
    in_memory_session_messages_from_workspace(workspace_snapshot, active_session_id)
        .unwrap_or_default()
}

/// Returns the active surface id from preserved navigation without recomputing task lists.
/// 不重算任务列表，直接从已保留导航中返回活动界面标识。
fn preserved_active_surface_session_id(
    workspace_snapshot: &WorkspaceSnapshot,
    subagent_navigation: Option<&SubagentNavigationSnapshot>,
) -> SessionId {
    subagent_navigation
        .and_then(|navigation| navigation.active_conversation_id.parse::<SessionId>().ok())
        .unwrap_or(workspace_snapshot.selected_session)
}

/// Returns messages for one active surface without refreshing subagent navigation.
/// 不刷新子代理导航，返回指定活动界面的消息。
fn surface_messages_for_session_without_subagent_navigation_refresh(
    guard: &ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    active_session_id: SessionId,
) -> Vec<SessionMessageEntry> {
    if active_session_id != workspace_snapshot.selected_session
        && let Some(messages) =
            in_memory_session_messages_from_workspace(workspace_snapshot, active_session_id)
        && !messages.is_empty()
    {
        return messages;
    }
    if active_session_id != workspace_snapshot.selected_session
        && let Some(state) = webui_subagent_task_state_by_task_id(guard, active_session_id)
        && let Some(conversation_state) = state.conversation_state.as_ref()
    {
        return conversation_state_to_session_messages(conversation_state);
    }
    in_memory_session_messages_from_workspace(workspace_snapshot, active_session_id)
        .unwrap_or_default()
}

/// Returns whether one WebUI subagent panel has enough persisted transcript detail for review.
/// 返回 WebUI 子代理面板是否拥有足够的持久转录详情可供复看。
fn webui_subagent_panel_has_recoverable_detail(
    workspace_snapshot: &WorkspaceSnapshot,
    state: &SubagentTaskPanelState,
) -> bool {
    in_memory_session_messages_from_workspace(workspace_snapshot, state.task_id)
        .is_some_and(|messages| !messages.is_empty())
        || subagent_task_panel_state_has_recoverable_conversation(state)
}

/// Builds the subagent navigation snapshot for the selected parent conversation.
/// 为当前选中父会话构建子代理导航快照。
fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    let root_session_id = workspace_snapshot.selected_session;
    if root_session_id == SessionId::nil() {
        return None;
    }
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id != root_session_id)
    {
        guard.active_subagent_panel = None;
    }
    let parent_session_id = root_session_id;
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&parent_session_id);
    let has_active_panel = guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == parent_session_id);
    if !has_visible_round && !has_active_panel {
        return None;
    }
    let requested_active_task_id = guard.active_subagent_panel.and_then(|selection| {
        (selection.parent_session_id == parent_session_id).then_some(selection.task_id)
    });
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
    let normalized_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .map(|entries| {
            entries
                .values()
                .map(|state| {
                    normalize_webui_subagent_task_state(
                        Some(workspace_snapshot),
                        state,
                        task_runtime,
                        live_listener_task_ids.contains(&state.task_id),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_running_subagents = normalized_states
        .iter()
        .any(|state| state.run_state.is_running());
    if !subagent_navigation_surface_is_visible(
        has_visible_round,
        has_running_subagents,
        has_active_panel,
    ) {
        return None;
    }
    let (navigation_states, active_task_id) = select_subagent_navigation_panel_states(
        &normalized_states,
        requested_active_task_id,
        |state| webui_subagent_panel_has_recoverable_detail(workspace_snapshot, state),
    );
    if navigation_states.is_empty() {
        return None;
    }
    let tasks = navigation_states
        .iter()
        .map(|state| subagent_task_summary_from_state(state, active_task_id))
        .collect::<Vec<_>>();
    let active_conversation_id = active_task_id.unwrap_or(parent_session_id).to_string();
    Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id,
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    })
}

/// Builds the subagent navigation snapshot from the current cache without refreshing segmented storage.
/// 仅从当前缓存构建子代理导航快照，不刷新分段存储。
fn subagent_navigation_snapshot_from_cached_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    let parent_session_id = workspace_snapshot.selected_session;
    if parent_session_id == SessionId::nil() {
        return None;
    }
    if guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id != parent_session_id)
    {
        guard.active_subagent_panel = None;
    }
    let has_visible_round = guard
        .session_subagent_navigation_rounds
        .contains(&parent_session_id);
    let has_active_panel = guard
        .active_subagent_panel
        .is_some_and(|selection| selection.parent_session_id == parent_session_id);
    if !has_visible_round && !has_active_panel {
        return None;
    }
    let requested_active_task_id = guard.active_subagent_panel.and_then(|selection| {
        (selection.parent_session_id == parent_session_id).then_some(selection.task_id)
    });
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let live_listener_task_ids = guard.detached_subagent_task_listeners.clone();
    let normalized_states = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .map(|entries| {
            entries
                .values()
                .map(|state| {
                    let is_live_running = webui_subagent_task_has_live_run(
                        task_runtime,
                        live_listener_task_ids.contains(&state.task_id),
                        state.task_id,
                    );
                    normalize_subagent_task_panel_state(state, is_live_running, None)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_running_subagents = normalized_states
        .iter()
        .any(|state| state.run_state.is_running());
    if !subagent_navigation_surface_is_visible(
        has_visible_round,
        has_running_subagents,
        has_active_panel,
    ) {
        return None;
    }
    let (navigation_states, active_task_id) = select_subagent_navigation_panel_states(
        &normalized_states,
        requested_active_task_id,
        |state| webui_subagent_panel_has_recoverable_detail(workspace_snapshot, state),
    );
    if navigation_states.is_empty() {
        return None;
    }
    let tasks = navigation_states
        .iter()
        .map(|state| subagent_task_summary_from_state(state, active_task_id))
        .collect::<Vec<_>>();
    Some(SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id: active_task_id.unwrap_or(parent_session_id).to_string(),
        active_task_id: active_task_id.map(|task_id| task_id.to_string()),
        tasks,
    })
}

/// Converts one shared subagent panel state into a service API summary.
/// 将一条共享子代理面板状态转换为 service API 摘要。
fn subagent_task_summary_from_state(
    state: &SubagentTaskPanelState,
    active_task_id: Option<SessionId>,
) -> SubagentTaskSummary {
    SubagentTaskSummary {
        task_id: state.task_id.to_string(),
        public_task_id: state.public_task_id.clone(),
        label: subagent_navigation_task_label(state),
        description: state.description.clone(),
        run_state: subagent_run_state_key(state.run_state).to_string(),
        is_running: state.run_state.is_running(),
        is_active: active_task_id == Some(state.task_id),
    }
}

/// Returns the service API run-state key for one shared subagent state.
/// 返回单个共享子代理状态对应的 service API 运行状态键。
fn subagent_run_state_key(run_state: SubagentTaskRunState) -> &'static str {
    match run_state {
        SubagentTaskRunState::Running => "running",
        SubagentTaskRunState::Completed => "completed",
        SubagentTaskRunState::Stopped => "stopped",
        SubagentTaskRunState::Failed => "failed",
    }
}

/// Refreshes the WebUI workbench after a focused workspace metadata update.
/// 在一次聚焦的工作区元数据更新后刷新 WebUI 工作台。
async fn refresh_workbench_snapshot(
    state: tauri::State<'_, SharedServiceBridgeState>,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        preserved_workspace_run_state,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        agent_runtime_for_status,
        live_listener_task_ids,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = cached_luaskills_catalog_from_state(&guard);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = cached_mcp_catalog_from_state(&guard);
        let logs = guard.startup.recent_logs.clone();
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let live_context_budgets = guard.live_context_budgets.clone();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            preserved_workspace_run_state,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            guard.agent_runtime.clone(),
            guard.detached_subagent_task_listeners.clone(),
        )
    };

    let mut refreshed_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );
    let selected_messages =
        if let Some(messages) = selected_session_messages_from_workspace(&refreshed_snapshot) {
            messages
        } else {
            let hydration_snapshot = match load_selected_session_hydration(
                controller_runtime.clone(),
                refreshed_snapshot.selected_session,
            )
            .await
            {
                Ok(snapshot) => snapshot,
                Err(error) if is_missing_segmented_session_file_error(&error) => None,
                Err(error) => return Err(error),
            };
            hydration_snapshot
                .map(|snapshot| snapshot.messages)
                .unwrap_or_default()
        };
    let selected_session_id = refreshed_snapshot.selected_session;
    if selected_session_id != SessionId::nil() && !selected_messages.is_empty() {
        refreshed_snapshot
            .session_messages
            .insert(selected_session_id, selected_messages);
        apply_persisted_task_mirror_statuses_to_workspace(
            &controller_runtime,
            &mut refreshed_snapshot,
            selected_session_id,
            |task_id| {
                webui_subagent_task_has_live_run(
                    agent_runtime_for_status
                        .as_ref()
                        .map(|runtime| &runtime.task_runtime),
                    live_listener_task_ids.contains(&task_id),
                    task_id,
                )
            },
        )?;
    }
    let (surface_messages, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &refreshed_snapshot),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &refreshed_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        &logs,
        &live_context_budgets,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(refreshed_snapshot);
    }
    Ok(workbench)
}

/// Captures the minimal in-memory state needed to repair one refreshed workspace snapshot.
/// 捕获修复一个刷新后工作区快照所需的最小内存状态。
fn capture_preserved_workspace_run_state(
    previous_snapshot: Option<&WorkspaceSnapshot>,
    active_kernel_session_ids: &BTreeSet<SessionId>,
    submitted_session_id: Option<SessionId>,
) -> Option<PreservedWorkspaceRunState> {
    let Some(previous_snapshot) = previous_snapshot else {
        return None;
    };
    let mut sessions = Vec::new();
    for (previous_index, previous_session) in previous_snapshot.sessions.iter().enumerate() {
        let runtime_state = previous_snapshot
            .session_runtime_state
            .get(&previous_session.id);
        let has_pending_prompt = runtime_state.is_some_and(|runtime_state| {
            !runtime_state.ui_state.pending_prompt_queue.is_empty()
                || !runtime_state.ui_state.pending_prompt_panel_queue.is_empty()
        });
        let is_submitted_session = submitted_session_id == Some(previous_session.id);
        let had_active_run = active_kernel_session_ids.contains(&previous_session.id);
        if !had_active_run && !has_pending_prompt && !is_submitted_session {
            continue;
        }
        let previous_phase = previous_snapshot
            .session_run_phases
            .get(&previous_session.id)
            .copied();
        let previous_project_index =
            previous_session
                .project_id
                .as_deref()
                .and_then(|project_id| {
                    previous_snapshot
                        .task_projects
                        .iter()
                        .find(|project| project.id == project_id)
                        .and_then(|project| {
                            project
                                .session_ids
                                .iter()
                                .position(|session_id| *session_id == previous_session.id)
                        })
                });
        sessions.push(PreservedSessionRunState {
            previous_index,
            previous_project_index,
            session: previous_session.clone(),
            previous_phase,
            had_active_run,
            runtime_state: runtime_state.cloned(),
            messages: previous_snapshot
                .session_messages
                .get(&previous_session.id)
                .cloned(),
        });
    }
    let selected_session = (previous_snapshot.selected_session != SessionId::nil())
        .then_some(previous_snapshot.selected_session);
    (selected_session.is_some() || !sessions.is_empty()).then_some(PreservedWorkspaceRunState {
        selected_session,
        sessions,
    })
}

/// Preserves in-memory running session markers across database-backed snapshot refreshes.
/// 在基于数据库重载快照时保留内存中的会话运行中标记。
fn preserve_active_session_run_state(
    preserved_state: Option<&PreservedWorkspaceRunState>,
    refreshed_snapshot: &mut WorkspaceSnapshot,
) {
    let Some(preserved_state) = preserved_state else {
        return;
    };
    if let Some(selected_session) = preserved_state.selected_session
        && refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == selected_session)
    {
        refreshed_snapshot.selected_session = selected_session;
    }
    for preserved_session in &preserved_state.sessions {
        let previous_session = &preserved_session.session;
        if !refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == previous_session.id)
        {
            preserve_missing_visible_session_summary(
                preserved_state,
                refreshed_snapshot,
                preserved_session,
            );
        }
        if let Some(refreshed_session) = refreshed_snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == previous_session.id)
        {
            refreshed_session.status = if preserved_session.had_active_run {
                SessionState::Running
            } else if previous_session.status == SessionState::Running {
                refreshed_session.status
            } else {
                previous_session.status
            };
            refreshed_session.last_updated_label = previous_session.last_updated_label.clone();
        }
        if let Some(previous_phase) = preserved_session.previous_phase
            && (preserved_session.had_active_run || !is_active_session_run_phase(previous_phase))
        {
            let preserved_phase = if preserved_session.had_active_run
                && !is_active_session_run_phase(previous_phase)
            {
                SessionRunPhase::Preparing
            } else {
                previous_phase
            };
            refreshed_snapshot
                .session_run_phases
                .insert(previous_session.id, preserved_phase);
        }
        if let Some(mut runtime_state) = preserved_session.runtime_state.clone() {
            if let Some(refreshed_runtime_state) = refreshed_snapshot
                .session_runtime_state
                .get(&previous_session.id)
                && !runtime_state_has_concrete_conversation_context(&runtime_state)
                && runtime_state_has_concrete_conversation_context(refreshed_runtime_state)
            {
                runtime_state.conversation_state =
                    refreshed_runtime_state.conversation_state.clone();
            }
            refreshed_snapshot
                .session_runtime_state
                .insert(previous_session.id, runtime_state);
        }
        if let Some(messages) = preserved_session.messages.clone() {
            refreshed_snapshot
                .session_messages
                .insert(previous_session.id, messages);
        }
    }
}

/// Preserves one in-memory visible session that has not reached the database snapshot yet.
/// 保留一个尚未进入数据库快照的内存可见会话。
///
/// # Parameters
/// 参数
/// `preserved_state` is the retained minimal state before database refresh.
/// `preserved_state` 是数据库刷新前保留的最小状态。
/// `refreshed_snapshot` is the database-backed snapshot being repaired.
/// `refreshed_snapshot` 是正在修复的数据库来源快照。
/// `preserved_session` is the active, queued, or submitted session to preserve.
/// `preserved_session` 是需要保留的运行中、排队中或已提交会话。
fn preserve_missing_visible_session_summary(
    preserved_state: &PreservedWorkspaceRunState,
    refreshed_snapshot: &mut WorkspaceSnapshot,
    preserved_session: &PreservedSessionRunState,
) {
    let previous_session = &preserved_session.session;
    let insert_index = preserved_session
        .previous_index
        .min(refreshed_snapshot.sessions.len());
    refreshed_snapshot
        .sessions
        .insert(insert_index, previous_session.clone());
    if preserved_state.selected_session == Some(previous_session.id) {
        refreshed_snapshot.selected_session = previous_session.id;
    }
    preserve_visible_project_session_membership(refreshed_snapshot, preserved_session);
}

/// Preserves the project sidebar membership for one in-memory session.
/// 保留一个内存会话在项目侧栏中的归属关系。
///
/// # Parameters
/// 参数
/// `refreshed_snapshot` is the database-backed snapshot being repaired.
/// `refreshed_snapshot` 是正在修复的数据库来源快照。
/// `preserved_session` carries the session and its retained project index.
/// `preserved_session` 携带会话及其保留的项目位置。
fn preserve_visible_project_session_membership(
    refreshed_snapshot: &mut WorkspaceSnapshot,
    preserved_session: &PreservedSessionRunState,
) {
    let previous_session = &preserved_session.session;
    let Some(project_id) = previous_session.project_id.as_deref() else {
        return;
    };
    let previous_index = preserved_session.previous_project_index.unwrap_or_default();
    if let Some(refreshed_project) = refreshed_snapshot
        .task_projects
        .iter_mut()
        .find(|project| project.id == project_id)
    {
        refreshed_project
            .session_ids
            .retain(|session_id| *session_id != previous_session.id);
        let insert_index = previous_index.min(refreshed_project.session_ids.len());
        refreshed_project
            .session_ids
            .insert(insert_index, previous_session.id);
    }
}

/// Returns whether one session phase still represents active work.
/// 判断单个会话阶段是否仍代表活跃执行。
fn is_active_session_run_phase(phase: SessionRunPhase) -> bool {
    matches!(
        phase,
        SessionRunPhase::Preparing
            | SessionRunPhase::MemoryRecall
            | SessionRunPhase::Requesting
            | SessionRunPhase::Thinking
            | SessionRunPhase::ToolRunning
    )
}

/// Returns whether a runtime event closes the current streamed thinking segment.
/// 判断运行时事件是否会关闭当前流式思考片段。
fn is_provider_tool_boundary_runtime_event(event: &RuntimeEvent) -> bool {
    matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) && (event.title.starts_with("工具调用 · ") || event.title.starts_with("工具返回 · "))
}

/// Loads the selected session transcript window when a selected session exists.
/// 在存在选中会话时加载其转录窗口。
async fn load_selected_session_hydration(
    controller_runtime: ControllerRuntimeHandle,
    selected_session_id: SessionId,
) -> Result<Option<SessionHydrationSnapshot>, String> {
    if selected_session_id == SessionId::nil() {
        return Ok(None);
    }
    vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
        controller_runtime,
        selected_session_id,
        SELECTED_SESSION_TURN_LOAD_LIMIT,
    )
    .await
    .map(Some)
}

/// Returns whether a hydration failure is caused by missing segmented session files.
/// 判断水合失败是否源自缺失分段会话文件。
fn is_missing_segmented_session_file_error(error: &str) -> bool {
    (error.contains("metadata.json")
        || error.contains("state.json")
        || error.contains("s001.jsonl")
        || error.contains("s000001.jsonl"))
        && (error.contains("os error 3")
            || error.contains("os error 2")
            || error.contains("系统找不到指定的路径")
            || error.contains("The system cannot find the path specified")
            || error.contains("No such file or directory"))
}

/// Returns the current Unix timestamp as millisecond text for SQLite metadata rows.
/// 返回当前 Unix 毫秒时间戳文本，用于 SQLite 元数据行。
fn current_unix_millis_text() -> String {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
        .to_string()
}

/// Returns the current Unix timestamp as a millisecond integer for hot-path timing.
/// 返回当前 Unix 毫秒整数，用于热路径计时。
fn current_unix_millis_value() -> i128 {
    OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .checked_div(1_000_000)
        .unwrap_or_default()
}

/// Returns a UTC timestamp suitable for human-readable diagnostic traces.
/// 返回适合人工阅读诊断日志的 UTC 时间戳。
fn current_webui_trace_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

/// Returns whether WebUI execution-chain diagnostic traces are enabled.
/// 返回 WebUI 执行链诊断日志是否已启用。
fn webui_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Emits one WebUI execution-chain diagnostic trace line when tracing is enabled.
/// 当诊断开关启用时输出一条 WebUI 执行链诊断日志。
fn webui_trace(stage: &str, detail: impl AsRef<str>) {
    if !webui_trace_enabled() {
        return;
    }
    eprintln!(
        "[tauri-service][webui-trace] timestamp={} unix_ms={} stage={} {}",
        current_webui_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}

/// Emits one WebUI timing trace when a supposedly hot path exceeds the 20ms budget.
/// 当本应为热路径的步骤超过 20ms 预算时输出 WebUI 计时日志。
fn webui_trace_if_slow(stage: &str, started_at_ms: i128, detail: impl AsRef<str>) {
    let elapsed_ms = current_unix_millis_value().saturating_sub(started_at_ms);
    if elapsed_ms > 20 {
        webui_trace(
            stage,
            format!("elapsed_ms={elapsed_ms} {}", detail.as_ref()),
        );
    }
}

/// Builds compact RuntimeEvent metadata for WebUI trace logs.
/// 为 WebUI 跟踪日志构建紧凑的 RuntimeEvent 元数据。
fn runtime_event_trace_detail(event: &RuntimeEvent) -> String {
    let metadata = event.metadata.as_ref();
    let tool_call_id = metadata
        .and_then(|value| value.get("tool_call_id"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let tool_name = metadata
        .and_then(|value| value.get("tool_name"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    let display_state = metadata
        .and_then(|value| value.get("display_state"))
        .and_then(Value::as_str)
        .unwrap_or("<none>");
    format!(
        "tool_call_id={tool_call_id} tool_name={tool_name} display_state={display_state} title_chars={} detail_chars={}",
        event.title.chars().count(),
        event.detail.chars().count()
    )
}

/// Builds compact detached-task metadata for WebUI trace logs.
/// 为 WebUI 跟踪日志构建紧凑的分离任务元数据。
fn detached_task_event_trace_detail(event: &DetachedTaskRuntimeEvent) -> String {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
            event,
            ..
        }) => runtime_event_trace_detail(event),
        DetachedTaskRuntimeEvent::ParentRuntime { event, .. } => runtime_event_trace_detail(event),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingDelta {
            session_id,
            delta,
        }) => format!("session={session_id} delta_chars={}", delta.chars().count()),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::OutputDelta {
            session_id,
            delta,
        }) => format!("session={session_id} delta_chars={}", delta.chars().count()),
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ConversationCheckpoint {
                session_id,
                conversation_state,
                provider_request_count,
                ..
            },
        ) => format!(
            "session={session_id} records={} messages={} provider_requests={provider_request_count}",
            conversation_state.records.len(),
            conversation_state.messages.len()
        ),
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(report)) => {
            format!(
                "provider_requests={} output_chars={} error={}",
                report.provider_request_count,
                report
                    .assistant_output
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count(),
                report.error.is_some()
            )
        }
        _ => String::from("detail=<none>"),
    }
}

/// Returns a compact label for one detached task event without dumping large payloads.
/// 返回单条分离任务事件的紧凑标签，避免输出大型载荷。
fn detached_task_event_label(event: &DetachedTaskRuntimeEvent) -> &'static str {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Prepared(_)) => {
            "child.prepared"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RequestReady { .. }) => {
            "child.request_ready"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::WireRequestReady {
            ..
        }) => "child.wire_request_ready",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RequestStarted {
            ..
        }) => "child.request_started",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingDelta {
            ..
        }) => "child.thinking_delta",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::ThinkingCompleted {
            ..
        }) => "child.thinking_completed",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::OutputDelta { .. }) => {
            "child.output_delta"
        }
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
            event,
            ..
        }) => match event.kind {
            RuntimeEventKind::DelegateStarted => "child.runtime.delegate_started",
            RuntimeEventKind::DelegateFinished => "child.runtime.delegate_finished",
            RuntimeEventKind::ToolStarted => "child.runtime.tool_started",
            RuntimeEventKind::ToolFinished => "child.runtime.tool_finished",
            _ => "child.runtime.event",
        },
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ConversationCheckpoint { .. },
        ) => "child.checkpoint",
        DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::Finished(_)) => {
            "child.finished"
        }
        DetachedTaskRuntimeEvent::ParentRuntime { event, .. } => match event.kind {
            RuntimeEventKind::DelegateStarted => "parent.delegate_started",
            RuntimeEventKind::DelegateFinished => "parent.delegate_finished",
            _ => "parent.runtime_event",
        },
    }
}

/// Emits initial service events after the WebView is created.
/// 在 WebView 创建后发出初始服务事件。
fn emit_initial_service_events(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(guard) = state.lock()
    {
        emit_service_snapshots(app, &guard.startup, &guard.workbench);
    }
}

/// Starts the real service bootstrap on a background thread.
/// 在后台线程启动真实服务引导流程。
fn spawn_service_bootstrap(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name(String::from("vulcan-tauri-service-bootstrap"))
        .spawn(move || {
            if let Err(error) = run_service_bootstrap(&app) {
                update_service_failure(&app, error);
            }
        })
        .expect("failed to spawn Vulcan service bootstrap thread");
}

/// Startup step-scoped error used to keep bootstrap diagnostics attached to the real failed phase.
/// 带启动阶段的错误，用于把引导诊断绑定到真实失败阶段。
#[derive(Debug, Clone)]
struct ServiceBootstrapError {
    /// Stable startup step identifier shown in the boot overlay.
    /// 启动遮罩中展示的稳定步骤标识。
    step_id: &'static str,
    /// Human-readable startup step label used in logs.
    /// 日志中使用的人类可读启动步骤标签。
    step_label: &'static str,
    /// Concrete failure message produced by the failed subsystem.
    /// 失败子系统产出的具体失败信息。
    message: String,
}

impl ServiceBootstrapError {
    /// Builds one startup step-scoped error.
    /// 构建一条带启动阶段的错误。
    fn new(step_id: &'static str, step_label: &'static str, message: impl Into<String>) -> Self {
        Self {
            step_id,
            step_label,
            message: message.into(),
        }
    }
}

/// Wraps a subsystem result with one startup step scope.
/// 为子系统结果附加一个启动阶段作用域。
fn bootstrap_step<T>(
    result: Result<T, String>,
    step_id: &'static str,
    step_label: &'static str,
) -> Result<T, ServiceBootstrapError> {
    result.map_err(|error| ServiceBootstrapError::new(step_id, step_label, error))
}

/// Loads the same standalone AI capability configuration used by the iced workbench.
/// 加载与 iced 工作台一致的独立 AI 能力配置。
fn load_service_ai_tool_configuration(
    provider_configuration: &ProviderConfigurationFile,
    logs: &mut Vec<String>,
) -> PersistedAiToolConfiguration {
    match vulcan_agent_runtime::ai_tool_configuration::load_ai_tool_configuration_file(
        provider_configuration,
    ) {
        Ok(loaded) => {
            if loaded.normalized {
                if let Err(error) =
                    vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    )
                {
                    logs.push(format!(
                        "[tauri-service] ai capability configuration normalization save failed: {error}"
                    ));
                }
            }
            match loaded.source {
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::DedicatedFile => logs.push(String::from(
                    "[tauri-service] ai capability configuration loaded from ~/.vulcan/vulcan-ai-capabilities.json",
                )),
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::LegacyProviderFile => {
                    match vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    ) {
                        Ok(()) => logs.push(String::from(
                            "[tauri-service] legacy embedded ai capability configuration migrated to ~/.vulcan/vulcan-ai-capabilities.json",
                        )),
                        Err(error) => logs.push(format!(
                            "[tauri-service] legacy ai capability configuration migration failed: {error}"
                        )),
                    }
                }
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::Default => logs.push(String::from(
                    "[tauri-service] ai capability configuration uses built-in defaults",
                )),
            }
            loaded.configuration
        }
        Err(error) => {
            logs.push(format!(
                "[tauri-service] ai capability configuration load failed, falling back: {error}"
            ));
            if provider_configuration.ai_tools.is_default() {
                PersistedAiToolConfiguration::default()
            } else {
                provider_configuration.ai_tools.clone()
            }
        }
    }
}

/// Runs the real startup sequence shared with the legacy GUI.
/// 运行与旧版 GUI 共享的真实启动顺序。
fn run_service_bootstrap(app: &tauri::AppHandle) -> Result<(), ServiceBootstrapError> {
    let mut logs = vec![format!(
        "[tauri-service] workspace root: {}",
        workspace_root().display()
    )];
    let controller_configuration =
        vulcan_provider_config::config::load_controller_configuration_file().map_err(|error| {
            ServiceBootstrapError::new("service-process", "Core Service", error)
        })?;
    let provider_configuration = vulcan_provider_config::config::load_configuration_file()
        .map_err(|error| ServiceBootstrapError::new("service-process", "Core Service", error))?;
    let ai_tool_configuration =
        load_service_ai_tool_configuration(&provider_configuration, &mut logs);

    update_startup(app, "bootstrapping-vldb", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Running,
                    "正在初始化 VLDB/controller 数据平面",
                ),
                ("luaskills", StartupStepStatus::Pending, "等待 VLDB 就绪"),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "等待 LuaSkills 就绪",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                ServiceBootstrapError::new(
                    "service-process",
                    "Core Service",
                    format!("创建 Tauri service tokio runtime 失败：{error}"),
                )
            })?,
    );

    let controller_report = bootstrap_step(
        runtime.block_on(
            vulcan_workspace_store::data_plane::bootstrap_application_data_plane(
                controller_configuration.clone(),
            ),
        ),
        "vldb",
        "VLDB",
    )?;
    logs.push(format!(
        "[tauri-service] vldb ready: {}",
        controller_report.handle.sqlite_path.display()
    ));
    let controller_runtime = controller_report.handle.clone();

    update_startup(app, "bootstrapping-luaskills", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Running,
                    "正在初始化 LuaSkills 引擎与技能根",
                ),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "等待 LuaSkills 就绪",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let luaskills_runtime = bootstrap_step(
        crate::luaskills_bridge::bootstrap_luaskills(controller_configuration.clone(), |message| {
            update_luaskills_bootstrap_progress(app, &logs, message)
        }),
        "luaskills",
        "LuaSkills",
    )?;
    let luaskills_catalog = bootstrap_step(
        crate::luaskills_bridge::load_luaskills_catalog(&luaskills_runtime),
        "luaskills",
        "LuaSkills",
    )?;
    logs.push(format!(
        "[tauri-service] luaskills ready: root={} skills={} entries={}",
        luaskills_catalog.runtime_root.display(),
        luaskills_catalog.skills.len(),
        luaskills_catalog.entry_count
    ));

    update_startup(app, "bootstrapping-token-budget", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    StartupStepStatus::Running,
                    "正在初始化 Token 预算运行时",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let token_budget_runtime = crate::token_budget_bridge::TauriTokenBudgetRuntime::initialize();
    logs.push(format!(
        "[tauri-service] token budget: {}",
        token_budget_runtime.status_row()
    ));

    update_startup(app, "bootstrapping-mcp", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Running, "正在初始化 MCP Runtime"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });

    let mcp_runtime = runtime
        .block_on(crate::mcp_bridge::bootstrap_mcp_runtime(
            provider_configuration.clone(),
        ))
        .map_err(|error| {
            ServiceBootstrapError::new(
                "mcp",
                "MCP Runtime",
                format!("MCP Runtime 初始化失败：{error}"),
            )
        })?;
    let mcp_catalog = mcp_runtime.catalog();
    logs.push(format!(
        "[tauri-service] mcp ready: servers={}/{} connected={} tools={} prompts={} resources={}",
        mcp_catalog.enabled_server_count,
        mcp_catalog.configured_server_count,
        mcp_catalog.connected_server_count,
        mcp_catalog.tool_count,
        mcp_catalog.prompt_count,
        mcp_catalog.resource_count
    ));
    let agent_profile_catalog = load_agent_profile_catalog()
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    // Cache LuaSkills tool entries once during startup so provider-run preparation never enumerates the engine on the submit hot path.
    // 启动时缓存一次 LuaSkills 工具入口，避免 Provider 运行准备阶段在发送热路径枚举引擎。
    let agent_luaskills_tool_entries = {
        let engine = luaskills_runtime.engine.lock().map_err(|_| {
            ServiceBootstrapError::new(
                "luaskills",
                "LuaSkills",
                "LuaSkills 引擎锁已损坏，无法构建工具入口缓存。",
            )
        })?;
        engine.list_entries_for_authority(::luaskills::SkillManagementAuthority::System)
    };
    let agent_luaskills_runtime = LuaSkillsRuntimeHandle {
        engine: Arc::clone(&luaskills_runtime.engine),
        runtime_root: luaskills_runtime.runtime_root.clone(),
        system_lua_lib_dir: luaskills_runtime.runtime_root.join("system_lua_lib"),
        skill_roots: luaskills_runtime.skill_roots.clone(),
        entry_count: luaskills_catalog.entry_count,
        tool_entries: Arc::new(Mutex::new(agent_luaskills_tool_entries)),
        operation_progress_events: Arc::new(Mutex::new(VecDeque::new())),
    };
    let agent_mcp_runtime = runtime
        .block_on(bootstrap_mcp_runtime_access(provider_configuration.clone()))
        .map_err(|error| {
            ServiceBootstrapError::new(
                "mcp",
                "MCP Runtime",
                format!("Agent MCP Runtime 初始化失败：{error}"),
            )
        })?;
    let agent_token_budget_runtime = Arc::new(
        TokenBudgetRuntime::initialize_from_resource_directory_with_backend(
            token_budget_runtime.resource_directory().to_path_buf(),
            token_budget_runtime.backend(),
        ),
    );
    let vmm_hook_runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot::default());
    let mut conversation_hook_runtime = ConversationHookRuntime::new();
    register_builtin_vmm_hooks(
        conversation_hook_runtime.dispatcher_mut(),
        vmm_hook_runtime.clone(),
    )
    .map_err(|error| {
        ServiceBootstrapError::new(
            "workspace",
            "Workspace",
            format!("VMM hook 注册失败：{error}"),
        )
    })?;
    let agent_runtime = AgentRuntimeBridge {
        provider_configuration: provider_configuration.clone(),
        ai_tool_configuration: ai_tool_configuration.clone(),
        agent_profile_catalog,
        luaskills_runtime: Some(agent_luaskills_runtime),
        mcp_runtime: Some(agent_mcp_runtime),
        task_runtime: Arc::new(TaskRuntime::new()),
        question_runtime: Arc::new(QuestionRuntime::new()),
        command_session_runtime: Arc::new(CommandSessionRuntime::new()),
        token_budget_runtime: agent_token_budget_runtime,
        conversation_hook_runtime: Arc::new(Mutex::new(conversation_hook_runtime)),
        vmm_hook_runtime,
    };
    logs.push(String::from(
        "[tauri-service] agent runtime bridge ready: provider/LuaSkills/MCP/token-budget handles are mounted",
    ));

    update_startup(app, "loading-workspace", &logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                (
                    "workspace",
                    StartupStepStatus::Running,
                    "正在载入项目、会话与当前会话正文",
                ),
            ],
        )
    });

    let workspace_snapshot = bootstrap_step(
        runtime.block_on(
            vulcan_workspace_store::workspace_store::load_or_seed_workspace(
                controller_runtime.clone(),
            ),
        ),
        "workspace",
        "Workspace",
    )?;
    let vmm_hook_snapshot =
        build_vmm_hook_config_snapshot(&provider_configuration, &workspace_snapshot);
    agent_runtime
        .vmm_hook_runtime
        .seed_profile_cache(vmm_hook_snapshot.profile_sync.clone());
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(vmm_hook_snapshot);
    let selected_session_id = workspace_snapshot.selected_session;
    let hydration_snapshot = bootstrap_step(
        runtime.block_on(load_selected_session_hydration(
            controller_runtime.clone(),
            selected_session_id,
        )),
        "workspace",
        "Workspace",
    )?;
    logs.push(format!(
        "[tauri-service] workspace loaded: projects={} sessions={} selected={}",
        workspace_snapshot.task_projects.len(),
        workspace_snapshot.sessions.len(),
        selected_session_id
    ));

    let startup = startup_snapshot_with_phase(
        "ready",
        startup_steps_with_status(
            OffsetDateTime::now_utc(),
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                (
                    "luaskills",
                    StartupStepStatus::Ready,
                    "LuaSkills 引擎与技能目录已就绪",
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                (
                    "workspace",
                    StartupStepStatus::Ready,
                    "项目、会话与当前会话正文已载入",
                ),
            ],
        ),
        &logs,
    );
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        hydration_snapshot
            .as_ref()
            .map(|snapshot| snapshot.messages.as_slice())
            .unwrap_or(&[]),
        None,
        &logs,
        &BTreeMap::new(),
    );

    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.agent_runtime = Some(agent_runtime);
        guard.service_runtime = Some(runtime);
        guard.controller_runtime = Some(controller_runtime);
        guard.luaskills_runtime = Some(luaskills_runtime);
        guard.cached_luaskills_catalog = Some(luaskills_catalog);
        guard.token_budget_runtime = Some(token_budget_runtime);
        guard.mcp_runtime = Some(mcp_runtime);
        guard.cached_mcp_catalog = Some(mcp_catalog);
    }
    emit_service_snapshots(app, &startup, &workbench);
    Ok(())
}

/// Updates startup state while a bootstrap step is running.
/// 在引导步骤运行中更新启动状态。
fn update_startup<F>(app: &tauri::AppHandle, phase: &str, logs: &[String], build_steps: F)
where
    F: FnOnce(OffsetDateTime) -> Vec<StartupStepSnapshot>,
{
    let startup = startup_snapshot_with_phase(phase, build_steps(OffsetDateTime::now_utc()), logs);
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
    }
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Startup(Box::new(startup)),
    );
}

/// Emits a LuaSkills-specific startup progress snapshot.
/// 发出 LuaSkills 专用启动进度快照。
fn update_luaskills_bootstrap_progress(app: &tauri::AppHandle, logs: &[String], message: &str) {
    update_startup(app, "bootstrapping-luaskills", logs, |now| {
        startup_steps_with_status(
            now,
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                (
                    "vldb",
                    StartupStepStatus::Ready,
                    "VLDB/controller 数据平面已就绪",
                ),
                ("luaskills", StartupStepStatus::Running, message),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "等待 LuaSkills 就绪",
                ),
                ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "等待工作区数据库载入",
                ),
            ],
        )
    });
}

/// Updates the shared service state after a startup failure.
/// 在启动失败后更新共享服务状态。
fn update_service_failure(app: &tauri::AppHandle, error: ServiceBootstrapError) {
    let logs = vec![
        format!(
            "[tauri-service] startup failed at {}: {}",
            error.step_label, error.message
        ),
        format!(
            "[tauri-service] workspace root: {}",
            workspace_root().display()
        ),
    ];
    let startup = startup_snapshot_with_phase(
        "failed",
        startup_failure_steps(OffsetDateTime::now_utc(), &error),
        &logs,
    );
    let workbench = fallback_workbench_snapshot(&logs);
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench.clone();
    }
    emit_service_snapshots(app, &startup, &workbench);
}

/// Builds ordered startup steps for one scoped bootstrap failure.
/// 为单个带阶段的引导失败构建有序启动步骤。
fn startup_failure_steps(
    updated_at: OffsetDateTime,
    error: &ServiceBootstrapError,
) -> Vec<StartupStepSnapshot> {
    let step_order = [
        "service-process",
        "vldb",
        "luaskills",
        "token-budget",
        "mcp",
        "workspace",
    ];
    let failed_index = step_order
        .iter()
        .position(|step_id| *step_id == error.step_id)
        .unwrap_or(0);
    step_order
        .iter()
        .enumerate()
        .map(|(index, step_id)| {
            let (status, message) = if *step_id == error.step_id {
                (StartupStepStatus::Error, error.message.clone())
            } else if index < failed_index {
                (StartupStepStatus::Ready, startup_ready_message(step_id))
            } else {
                (
                    StartupStepStatus::Pending,
                    String::from("上游启动失败，未继续"),
                )
            };
            StartupStepSnapshot {
                id: (*step_id).to_string(),
                label: startup_step_label(step_id),
                status,
                message: Some(message),
                updated_at: Some(updated_at),
            }
        })
        .collect()
}

/// Returns the stable ready message for one startup step.
/// 返回单个启动步骤的稳定就绪文案。
fn startup_ready_message(step_id: &str) -> String {
    match step_id {
        "service-process" => "Tauri 本地服务桥接已启动",
        "vldb" => "VLDB/controller 数据平面已就绪",
        "luaskills" => "LuaSkills 引擎与技能目录已就绪",
        "token-budget" => "Token 预算运行时已初始化",
        "mcp" => "MCP Runtime 已初始化",
        "workspace" => "项目、会话与当前会话正文已载入",
        _ => "已就绪",
    }
    .to_string()
}

/// Emits startup and workbench snapshots to subscribed WebUI clients.
/// 向已订阅的 WebUI 客户端发送启动与工作台快照。
fn emit_service_snapshots(
    app: &tauri::AppHandle,
    startup: &StartupSnapshot,
    workbench: &WorkbenchSnapshot,
) {
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Startup(Box::new(startup.clone())),
    );
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
}

/// Builds a startup snapshot for one phase.
/// 为单个阶段构建启动快照。
fn startup_snapshot_with_phase(
    phase: &str,
    steps: Vec<StartupStepSnapshot>,
    logs: &[String],
) -> StartupSnapshot {
    StartupSnapshot {
        ready: phase == "ready",
        phase: phase.to_string(),
        steps: if steps.is_empty() {
            startup_steps_with_status(
                OffsetDateTime::now_utc(),
                &[
                    (
                        "service-process",
                        StartupStepStatus::Pending,
                        "等待 Tauri 本地服务桥接启动",
                    ),
                    ("vldb", StartupStepStatus::Pending, "等待初始化"),
                    ("luaskills", StartupStepStatus::Pending, "等待初始化"),
                    ("token-budget", StartupStepStatus::Pending, "等待初始化"),
                    ("mcp", StartupStepStatus::Pending, "等待初始化"),
                    ("workspace", StartupStepStatus::Pending, "等待初始化"),
                ],
            )
        } else {
            steps
        },
        recent_logs: logs.to_vec(),
    }
}

/// Builds ordered startup steps from compact tuples.
/// 从紧凑元组构建有序启动步骤。
fn startup_steps_with_status(
    updated_at: OffsetDateTime,
    compact_steps: &[(&str, StartupStepStatus, &str)],
) -> Vec<StartupStepSnapshot> {
    compact_steps
        .iter()
        .map(|(id, status, message)| StartupStepSnapshot {
            id: (*id).to_string(),
            label: startup_step_label(id),
            status: status.clone(),
            message: Some((*message).to_string()),
            updated_at: Some(updated_at),
        })
        .collect()
}

/// Resolves the user-facing label for one startup step id.
/// 解析单个启动步骤标识对应的用户可见名称。
fn startup_step_label(id: &str) -> String {
    match id {
        "service-process" => "Core Service",
        "vldb" => "VLDB",
        "luaskills" => "LuaSkills",
        "token-budget" => "Token Budget",
        "mcp" => "MCP Runtime",
        "workspace" => "Workspace",
        _ => id,
    }
    .to_string()
}

/// Converts runtime state into a WebUI workbench snapshot.
/// 将运行时状态转换为 WebUI 工作台快照。
fn workbench_from_runtime(
    controller: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    surface_messages: &[SessionMessageEntry],
    subagent_navigation: Option<SubagentNavigationSnapshot>,
    logs: &[String],
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
) -> WorkbenchSnapshot {
    let selected_conversation_id = if workspace.selected_session == SessionId::nil() {
        None
    } else {
        Some(workspace.selected_session.to_string())
    };
    let active_conversation_id = subagent_navigation
        .as_ref()
        .map(|navigation| navigation.active_conversation_id.as_str())
        .or(selected_conversation_id.as_deref());
    let messages = active_conversation_id
        .map(|conversation_id| {
            conversation_messages_from_session(conversation_id, surface_messages)
        })
        .unwrap_or_default();
    let selected_runtime_state = workspace
        .session_runtime_state
        .get(&workspace.selected_session);
    WorkbenchSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        modules: runtime_modules(
            controller,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
        ),
        projects: workspace
            .task_projects
            .iter()
            .map(|project| ProjectSummary {
                id: project.id.clone(),
                name: project.title.clone(),
                path: project.path.clone(),
                pinned: project.pinned,
                icon_pack: project.icon_pack.clone(),
                icon_key: project.icon_key.clone(),
            })
            .collect(),
        expanded_project_ids: workspace.expanded_task_projects.iter().cloned().collect(),
        conversations: workspace
            .sessions
            .iter()
            .filter(|session| session_segmented_metadata_exists(session))
            .map(|session| {
                let run_phase = workspace
                    .session_run_phases
                    .get(&session.id)
                    .copied()
                    .unwrap_or(SessionRunPhase::Idle);
                let effective_run_phase = run_phase;
                let is_running = session.status == SessionState::Running
                    || is_active_session_run_phase(run_phase);
                ConversationSummary {
                    id: session.id.to_string(),
                    title: session.title.clone(),
                    status: if is_running {
                        SessionState::Running.label()
                    } else {
                        session.status.label()
                    }
                    .to_string(),
                    run_phase: effective_run_phase.to_string(),
                    last_updated_label: session.last_updated_label.clone(),
                    last_updated_at: session.updated_at.clone(),
                    unread: workspace
                        .session_runtime_state
                        .get(&session.id)
                        .is_some_and(|runtime_state| runtime_state.ui_state.unread),
                    project_id: session.project_id.clone(),
                }
            })
            .collect(),
        selected_conversation_id,
        messages,
        subagent_navigation,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_luaskills(luaskills_catalog),
        settings: settings(),
        providers: providers(provider_configuration),
        reasoning_selection: reasoning_selection(provider_configuration),
        ai_capabilities: ai_capabilities(ai_tool_configuration),
        composer_attachment_capabilities: composer_attachment_capabilities(
            provider_configuration,
            ai_tool_configuration,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: selected_runtime_state
            .map(|runtime_state| {
                runtime_state
                    .ui_state
                    .pending_prompt_panel_queue
                    .iter()
                    .map(pending_prompt_summary_from_panel_entry)
                    .collect()
            })
            .unwrap_or_default(),
        memories: memories(workspace),
        command_sessions: command_sessions(controller),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("startup-{index}"), "info", "service", message)
            })
            .chain(
                mcp_catalog
                    .summary_rows
                    .iter()
                    .enumerate()
                    .map(|(index, message)| {
                        log_entry(&format!("mcp-{index}"), "info", "mcp", message)
                    }),
            )
            .collect(),
        context_injection: context_injection_snapshot_from_runtime_state(selected_runtime_state),
        context_usage: context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            selected_runtime_state,
            live_context_budgets,
        ),
    }
}

/// Converts one persisted visible pending-prompt entry into a WebUI summary.
/// 将单条持久化可见待发送提示词转换为 WebUI 摘要。
fn pending_prompt_summary_from_panel_entry(
    entry: &PendingPromptPanelEntry,
) -> PendingPromptSummary {
    PendingPromptSummary {
        id: entry.id.clone(),
        prompt: entry.prompt.clone(),
        preview: entry.preview.clone(),
        attachments: entry
            .attachments
            .iter()
            .map(composer_attachment_summary_from_core)
            .collect(),
        state: match entry.state {
            PendingPromptPanelEntryState::Ready => String::from("ready"),
            PendingPromptPanelEntryState::Inserted => String::from("inserted"),
        },
        created_at: entry.created_at.clone(),
    }
}

/// Builds the selected-session context injection side-panel snapshot.
/// 构建当前选中会话的上下文注入侧面板快照。
fn context_injection_snapshot_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<ContextInjectionSnapshot> {
    let runtime_state = runtime_state?;
    let dynamic_cards = context_injection_cards_to_summary(context_injection_panel_cards_for_kind(
        &runtime_state.context_injection_state,
        Some(InjectionKind::Transient),
    ));
    let mut static_cards = Vec::new();
    if let Some(profile_card) = static_vmm_profile_panel_card(
        runtime_state
            .vmm_profile_sync
            .base_profile_content
            .as_deref(),
        runtime_state.vmm_profile_sync.updated_at.as_deref(),
    ) {
        static_cards.push(profile_card);
    }
    static_cards.extend(context_injection_cards_to_summary(
        context_injection_panel_cards_for_kind(
            &runtime_state.context_injection_state,
            Some(InjectionKind::Persistent),
        ),
    ));

    Some(ContextInjectionSnapshot {
        dynamic_cards,
        static_cards,
    })
}

/// Converts shared injection panel cards into service API DTOs.
/// 将共享注入面板卡片转换为 service API DTO。
fn context_injection_cards_to_summary(
    cards: Vec<InjectionPanelCard>,
) -> Vec<ContextInjectionCardSummary> {
    cards
        .into_iter()
        .map(|card| ContextInjectionCardSummary {
            tag: card.tag,
            description: card.description,
            items: card
                .items
                .into_iter()
                .map(|item| ContextInjectionItemSummary {
                    content: item.content,
                    detail: item.detail,
                    turn_label: item.turn_label,
                    ttl_label: item.ttl_label,
                    source_display: item.source_display,
                })
                .collect(),
        })
        .collect()
}

/// Builds the static VMM profile card aligned with the iced side panel.
/// 构建与 iced 侧面板一致的静态 VMM 画像卡片。
fn static_vmm_profile_panel_card(
    content: Option<&str>,
    updated_at: Option<&str>,
) -> Option<ContextInjectionCardSummary> {
    let content = content
        .map(str::trim)
        .filter(|content| !content.is_empty())?;
    let mut detail_lines = Vec::new();
    if let Some(updated_at) = updated_at.map(str::trim).filter(|value| !value.is_empty()) {
        detail_lines.push(format!("更新时间：{updated_at}"));
    }
    detail_lines.push(String::from("来源：VMM profile system hook"));
    detail_lines.push(String::from("内容："));
    detail_lines.push(content.to_string());

    Some(ContextInjectionCardSummary {
        tag: String::from("vmm_profile"),
        description: String::from(
            "VMM 画像基点。该静态上下文由系统提示词 hook 持续复用，面板仅用于查看。",
        ),
        items: vec![ContextInjectionItemSummary {
            content: compact_panel_preview(content),
            detail: detail_lines.join("\n"),
            turn_label: None,
            ttl_label: String::from("静态"),
            source_display: Some(String::from("VMM profile")),
        }],
    })
}

/// Builds the same compact multi-line preview used by the iced panel.
/// 构建与 iced 面板一致的多行紧凑预览。
fn compact_panel_preview(content: &str) -> String {
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(7)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > 6 {
        lines.truncate(6);
        lines.push(String::from("..."));
    }
    lines.join("\n")
}

/// Builds the selected-session context usage orb snapshot.
/// 构建当前选中会话的上下文占用圆盘快照。
fn context_usage_snapshot_from_runtime_state(
    session_id: SessionId,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
) -> Option<ContextUsageSnapshot> {
    let runtime_state = runtime_state?;
    let context_budget = live_context_budgets.get(&session_id);
    let mut tooltip_rows = Vec::new();
    if let Some(context_budget) = context_budget {
        tooltip_rows.push(format!(
            "输入占用：{:.0}% · {}",
            context_budget.usage_percent(),
            context_budget_severity_label(context_budget.severity),
        ));
        tooltip_rows.push(format!(
            "输入窗口（API实测/本地估算）：{}",
            context_budget.usage_label()
        ));
        tooltip_rows.extend(context_budget_breakdown_rows(context_budget));
        tooltip_rows.push(format!(
            "预留：输出 {} · 推理 {} · 压缩 {}",
            context_budget.reserved_output_tokens,
            context_budget.reserved_reasoning_tokens,
            context_budget.reserved_compaction_tokens
        ));
        if !context_budget.baseline_matches_active_runtime {
            let reason = context_budget
                .baseline_mismatch_reason
                .as_deref()
                .unwrap_or("当前运行时上下文与压缩锚点不一致");
            tooltip_rows.push(format!("恢复：基线失配（{reason}），已回退完整上下文。"));
        }
        if let Some(usage) = context_budget.observed_usage.as_ref() {
            tooltip_rows.push(format!("最近 API：{}", provider_usage_label(usage)));
        }
    } else {
        tooltip_rows.push(String::from("请求拆分：等待预算引擎计算"));
    }

    tooltip_rows.push(if runtime_state.last_compaction_id.is_some() {
        String::from("压缩状态：已使用摘要前文")
    } else {
        String::from("压缩状态：尚未触发")
    });

    Some(ContextUsageSnapshot {
        percent: context_budget
            .map(|budget| budget.usage_percent().round().clamp(0.0, 100.0) as u8)
            .unwrap_or(0),
        severity: context_budget
            .map(context_budget_severity_key)
            .unwrap_or_else(|| String::from("unknown")),
        baseline_matches_active_runtime: context_budget
            .map(|budget| budget.baseline_matches_active_runtime)
            .unwrap_or(true),
        tooltip_title: String::from("上下文占用"),
        tooltip_rows,
    })
}

/// Builds the context-budget token breakdown rows used by the tooltip.
/// 构建提示浮层使用的上下文预算 Token 拆分行。
fn context_budget_breakdown_rows(context_budget: &ContextBudgetSnapshot) -> Vec<String> {
    let raw_parts = [
        ("系统", context_budget.estimated_system_tokens),
        ("历史", context_budget.estimated_history_tokens),
        ("当前", context_budget.estimated_prompt_tokens),
        ("工具", context_budget.estimated_tool_tokens),
    ];
    let raw_total: usize = raw_parts.iter().map(|(_, value)| *value).sum();
    let prediction_row = format!(
        "预测拆分（预算器）：{}",
        raw_parts
            .iter()
            .map(|(label, value)| format!("{label} {value}"))
            .collect::<Vec<_>>()
            .join(" / ")
    );

    let Some(observed_input) =
        vulcan_context::observed_input_tokens_from_usage(context_budget.observed_usage.as_ref())
    else {
        return vec![prediction_row];
    };
    if raw_total == 0 {
        return vec![prediction_row];
    }
    let calibrated = raw_parts
        .iter()
        .map(|(label, value)| {
            let scaled = ((*value as f64 / raw_total as f64) * observed_input as f64).round();
            format!("{label} {}", scaled as usize)
        })
        .collect::<Vec<_>>()
        .join(" / ");
    vec![
        String::from("说明：API实测为准；拆分已按实测输入比例校准。"),
        prediction_row,
        format!("校准拆分（实测）：{calibrated}"),
    ]
}

/// Returns the stable context-budget severity key consumed by WebUI CSS.
/// 返回 WebUI CSS 消费的稳定上下文预算严重级别键。
fn context_budget_severity_key(context_budget: &ContextBudgetSnapshot) -> String {
    if !context_budget.baseline_matches_active_runtime {
        return String::from("baseline-mismatch");
    }
    match context_budget.severity {
        ContextBudgetSeverity::Healthy => "healthy",
        ContextBudgetSeverity::Warning => "warning",
        ContextBudgetSeverity::CompactSoon => "compact-soon",
        ContextBudgetSeverity::Overflow => "overflow",
    }
    .to_string()
}

/// Returns the user-facing context-budget severity label.
/// 返回面向用户的上下文预算严重级别标签。
fn context_budget_severity_label(severity: ContextBudgetSeverity) -> &'static str {
    match severity {
        ContextBudgetSeverity::Healthy => "正常",
        ContextBudgetSeverity::Warning => "预警",
        ContextBudgetSeverity::CompactSoon => "建议压缩",
        ContextBudgetSeverity::Overflow => "预算溢出",
    }
}

/// Formats one provider usage row for the context usage tooltip.
/// 为上下文占用提示浮层格式化单条 Provider 用量行。
fn provider_usage_label(usage: &vulcan_core::types::ProviderUsageSnapshot) -> String {
    let mut parts = Vec::new();
    if let Some(input) = usage.input_tokens {
        parts.push(format!("输入 {input}"));
    }
    if let Some(output) = usage.output_tokens {
        parts.push(format!("输出 {output}"));
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        parts.push(format!("推理 {reasoning}"));
    }
    if let Some(total) = usage.total_tokens {
        parts.push(format!("总计 {total}"));
    }
    if let Some(cached) = usage.cached_input_tokens {
        parts.push(format!("缓存命中 {cached}"));
    }
    if let Some(created) = usage.cache_creation_input_tokens {
        parts.push(format!("缓存写入 {created}"));
    }
    if parts.is_empty() {
        String::from("无可用 Token 数据")
    } else {
        parts.join(" / ")
    }
}

/// Converts session transcript entries into WebUI messages.
/// 将会话转录项转换为 WebUI 消息。
fn conversation_messages_from_session(
    conversation_id: &str,
    messages: &[SessionMessageEntry],
) -> Vec<ConversationMessage> {
    messages
        .iter()
        .filter(|message| message.is_visible_to_user())
        .filter(|message| !message.body.trim().is_empty())
        .map(|message| ConversationMessage {
            id: if message.id.is_empty() {
                format!("{}-{}", conversation_id, message.created_at)
            } else {
                message.id.clone()
            },
            conversation_id: conversation_id.to_string(),
            role: match message.kind {
                SessionMessageKind::User => MessageRole::User,
                SessionMessageKind::Assistant => MessageRole::Assistant,
                SessionMessageKind::Error => MessageRole::System,
            },
            title: non_empty_string(&message.title),
            record_kind: Some(message.record_kind.as_persisted_key().to_string()),
            body: message.body.clone(),
            metadata: message.metadata.clone(),
            attachments: composer_message_attachments_from_metadata(message.metadata.as_ref()),
        })
        .collect()
}

/// Extracts composer attachments from durable transcript metadata.
/// 从持久转录元数据中提取输入区附件。
fn composer_message_attachments_from_metadata(
    metadata: Option<&serde_json::Value>,
) -> Vec<ComposerAttachmentSummary> {
    let Some(metadata) = metadata else {
        return Vec::new();
    };
    let Some(attachment_value) = metadata
        .get("attachments")
        .or_else(|| metadata.get("transcript_attachments"))
    else {
        return Vec::new();
    };
    serde_json::from_value::<Vec<ConversationAttachment>>(attachment_value.clone())
        .unwrap_or_default()
        .iter()
        .map(composer_attachment_summary_from_core)
        .collect()
}

/// Returns whether a persisted session has readable segmented metadata on disk.
/// 返回持久化会话在磁盘上是否存在可读取的分段元数据。
fn session_segmented_metadata_exists(session: &vulcan_workbench_model::SessionSummary) -> bool {
    let Some(project_id) = session.project_id.as_deref() else {
        return false;
    };
    vulcan_workspace_store::workspace_store::find_segmented_session_directory_for_task_store(
        &vulcan_workspace_store::data_plane::application_data_root(),
        project_id,
        &session.id.to_string(),
    )
    .ok()
    .flatten()
    .is_some_and(|session_dir| session_dir.join("metadata.json").is_file())
}

/// Returns a trimmed string only when it contains visible content.
/// 仅当字符串包含可见内容时返回裁剪后的字符串。
fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Builds a fallback workbench snapshot used before real startup completes or after failure.
/// 构建真实启动完成前或失败后的回退工作台快照。
fn fallback_workbench_snapshot(logs: &[String]) -> WorkbenchSnapshot {
    let workspace_root = workspace_root();
    WorkbenchSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        modules: vec![
            runtime_module(
                "core",
                "Core Service",
                StartupStepStatus::Running,
                "服务正在启动",
            ),
            runtime_module(
                "vldb",
                "VLDB",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "luaskills",
                "LuaSkills",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "token-budget",
                "Token Budget",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "mcp",
                "MCP Runtime",
                StartupStepStatus::Pending,
                "等待真实启动流程",
            ),
            runtime_module(
                "workspace",
                "Workspace",
                StartupStepStatus::Pending,
                "等待真实工作区快照",
            ),
        ],
        projects: vec![ProjectSummary {
            id: String::from("current-workspace"),
            name: String::from("vulcan-code"),
            path: workspace_root.display().to_string(),
            pinned: false,
            icon_pack: None,
            icon_key: None,
        }],
        expanded_project_ids: vec![String::from("current-workspace")],
        conversations: Vec::new(),
        selected_conversation_id: None,
        messages: Vec::new(),
        subagent_navigation: None,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: Vec::new(),
        settings: settings(),
        providers: providers(&ProviderConfigurationFile::default()),
        reasoning_selection: reasoning_selection(&ProviderConfigurationFile::default()),
        ai_capabilities: ai_capabilities(&ProviderConfigurationFile::default().ai_tools),
        composer_attachment_capabilities: composer_attachment_capabilities(
            &ProviderConfigurationFile::default(),
            &ProviderConfigurationFile::default().ai_tools,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: Vec::new(),
        memories: Vec::new(),
        command_sessions: Vec::new(),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("fallback-{index}"), "info", "service", message)
            })
            .collect(),
        context_injection: None,
        context_usage: None,
    }
}

/// Creates feature areas matching the legacy GUI navigation surface.
/// 创建与旧版 GUI 导航面相匹配的功能区域。
fn feature_areas() -> Vec<FeatureAreaSummary> {
    vec![
        feature_area("chat", "控制台", "控制台、消息流与模型输入区", "真实快照"),
        feature_area("projects", "项目", "工作区项目列表与绑定状态", "真实快照"),
        feature_area(
            "providers",
            "模型",
            "Provider、模型、能力配置",
            "待接入配置详情",
        ),
        feature_area("tools", "工具", "工具调用、命令会话与输出", "待接入运行态"),
        feature_area(
            "plugins",
            "插件",
            "LuaSkills 与高级插件控制面板",
            "真实目录",
        ),
        feature_area(
            "memory",
            "VMM 记忆",
            "用户、项目、画像与记忆节点",
            "会话绑定快照",
        ),
        feature_area(
            "settings",
            "设置",
            "运行时、界面、权限与远程访问设置",
            "基础区域",
        ),
        feature_area("logs", "诊断", "启动日志、错误与运行时事件", "真实启动日志"),
    ]
}

/// Creates one feature area summary.
/// 创建单个功能区域摘要。
fn feature_area(id: &str, label: &str, description: &str, status: &str) -> FeatureAreaSummary {
    FeatureAreaSummary {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        status: status.to_string(),
    }
}

/// Converts LuaSkills catalog rows into plugin summaries.
/// 将 LuaSkills 目录行转换为插件摘要。
fn plugins_from_luaskills(catalog: &TauriLuaSkillsCatalog) -> Vec<PluginSummary> {
    catalog
        .skills
        .iter()
        .map(|skill| PluginSummary {
            id: skill.skill_id.clone(),
            name: skill.display_name.clone(),
            kind: format!("LuaSkill / {}", skill.root_name),
            status: if skill.enabled {
                "已启用"
            } else {
                "已停用"
            }
            .to_string(),
            capability: format!("{} 个入口：{}", skill.entry_count, skill.capability),
        })
        .collect()
}

/// Creates first-version settings sections.
/// 创建第一版设置分组。
fn settings() -> Vec<SettingSectionSummary> {
    vec![
        setting_section(
            "runtime",
            "运行时",
            "Core Service、LuaSkills、VLDB、MCP 的启动与状态设置",
            &["启动模式", "运行时目录", "MCP 工具策略", "VLDB 控制器"],
        ),
        setting_section(
            "providers",
            "模型与 Provider",
            "模型供应商、默认模型、工具能力与推理配置",
            &["默认 Provider", "默认模型", "工具调用", "推理强度"],
        ),
        setting_section(
            "plugins",
            "插件",
            "LuaSkills、Rust lib 插件、WebUI 控制面板与权限",
            &["插件目录", "启用状态", "权限声明", "WebUI 面板"],
        ),
        setting_section(
            "remote",
            "远程访问",
            "浏览器、手机、远程客户端访问 Core Service 的设置",
            &["监听地址", "访问令牌", "远程开关", "审计日志"],
        ),
    ]
}

/// Creates one settings section summary.
/// 创建单个设置分组摘要。
fn setting_section(
    id: &str,
    label: &str,
    description: &str,
    items: &[&str],
) -> SettingSectionSummary {
    SettingSectionSummary {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        items: items.iter().map(|item| item.to_string()).collect(),
    }
}

/// Creates provider summaries from the local catalog surface.
/// 从本地目录面创建 Provider 摘要。
fn providers(provider_configuration: &ProviderConfigurationFile) -> Vec<ProviderSummary> {
    let active_model_reference = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)
        .map(|(provider_id, model_id)| (provider_id.to_string(), model_id.to_string()));
    let mut configured_providers = provider_config::supported_provider_choices()
        .into_iter()
        .filter(|provider_choice| {
            provider_configuration
                .provider
                .contains_key(&provider_choice.id)
        })
        .map(|provider_choice| {
            let models = provider_config::effective_model_choices(
                &provider_choice.id,
                provider_configuration,
            )
            .into_iter()
            .map(|model_choice| {
                let active =
                    active_model_reference
                        .as_ref()
                        .is_some_and(|(provider_id, model_id)| {
                            provider_id == &provider_choice.id && model_id == &model_choice.id
                        });
                ProviderModelSummary {
                    id: model_choice.id,
                    name: model_choice.name,
                    active,
                }
            })
            .collect::<Vec<_>>();
            let active = models.iter().any(|model| model.active);
            ProviderSummary {
                id: provider_choice.id,
                name: provider_choice.name,
                status: String::from("已配置"),
                enabled_models: format!("{} 个已启用模型", models.len()),
                active,
                models,
            }
        })
        .collect::<Vec<_>>();
    configured_providers.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    if !configured_providers.is_empty() {
        return configured_providers;
    }

    let provider_root = vulcan_runtime_assets::resolve_provider_catalog_directory();
    vec![ProviderSummary {
        id: String::from("provider-catalog"),
        name: String::from("Provider Catalog"),
        status: if provider_root.is_dir() {
            "已载入"
        } else {
            "未找到"
        }
        .to_string(),
        enabled_models: provider_root.display().to_string(),
        active: false,
        models: Vec::new(),
    }]
}

/// Creates composer AI capability summaries from the shared provider configuration.
/// 从共享 Provider 配置创建输入区 AI 能力摘要。
///
/// # Parameters
/// 参数
/// `configuration` is the persisted AI tool configuration.
/// `configuration` 是持久化的 AI 工具配置。
///
/// # Returns
/// 返回值
/// Ordered AI capability summaries matching the iced workbench quick-toggle surface.
/// 与 iced 工作台快捷开关界面顺序一致的 AI 能力摘要。
fn ai_capabilities(
    configuration: &vulcan_provider_config::config::PersistedAiToolConfiguration,
) -> Vec<AiCapabilitySummary> {
    WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS
        .into_iter()
        .map(|kind| {
            let targets = configuration
                .resolved_configured_targets_for_kind(kind)
                .into_iter()
                .map(|target| AiCapabilityTargetSummary {
                    target_selector: target.target_selector,
                    vendor_id: target.vendor_id,
                    vendor_name: target.vendor_name,
                    target_alias: target.target_alias,
                    target_name: target.target_name,
                    model_id: target.model_id,
                    enabled: target.enabled,
                })
                .collect::<Vec<_>>();
            let available = configuration.has_usable_targets_for_kind(kind);
            AiCapabilitySummary {
                kind: kind.key().to_string(),
                label: kind.label().to_string(),
                available,
                enabled: available && !configuration.disabled_capability_kinds.contains(&kind),
                targets,
            }
        })
        .collect()
}

/// Creates composer attachment gates using the same capability resolver as runtime execution.
/// 使用与运行时执行相同的能力解析器创建输入区附件门控。
fn composer_attachment_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Vec<ComposerAttachmentCapabilitySummary> {
    let capabilities =
        active_workbench_model_capabilities(provider_configuration, ai_tool_configuration);
    [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Pdf,
    ]
    .into_iter()
    .map(|kind| {
        let native = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.supports_attachment_kind(kind.clone()));
        let enabled = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.allows_staged_attachment_kind(kind.clone()));
        ComposerAttachmentCapabilitySummary {
            kind: kind.modality_name().to_string(),
            label: composer_attachment_kind_label(&kind).to_string(),
            enabled,
            native,
            bridged: enabled && !native,
            accept: composer_attachment_kind_accept(&kind).to_string(),
        }
    })
    .collect()
}

/// Resolves the active workbench model capabilities from persisted provider settings.
/// 从持久化 Provider 设置解析当前工作台模型能力。
fn active_workbench_model_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ResolvedModelCapabilities> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let active_model =
        provider_config::effective_model_choices(provider_id, provider_configuration)
            .into_iter()
            .find(|model| model.id == model_id);
    Some(resolve_effective_model_capabilities(
        definition,
        active_model.as_ref(),
        ai_tool_configuration,
    ))
}

/// Creates VMM memory summaries from restored workspace state.
/// 从已恢复工作区状态创建 VMM 记忆摘要。
fn memories(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Vec<MemorySummary> {
    vec![
        MemorySummary {
            id: String::from("projects"),
            title: String::from("项目绑定"),
            category: String::from("Project"),
            state: format!("已恢复 {} 个项目", workspace.task_projects.len()),
        },
        MemorySummary {
            id: String::from("sessions"),
            title: String::from("会话绑定"),
            category: String::from("Conversation"),
            state: format!(
                "已恢复 {} 个会话运行态",
                workspace.session_runtime_state.len()
            ),
        },
    ]
}

/// Creates basic tool call summaries for service bridge commands.
/// 为 service bridge 命令创建基础工具调用摘要。
fn tool_calls() -> Vec<ToolCallSummary> {
    vec![
        ToolCallSummary {
            id: String::from("service-startup"),
            name: String::from("service_startup_snapshot"),
            status: String::from("ready"),
            detail: String::from("WebUI 可读取真实启动阶段快照。"),
        },
        ToolCallSummary {
            id: String::from("service-workbench"),
            name: String::from("service_workbench_snapshot"),
            status: String::from("ready"),
            detail: String::from("WebUI 可读取真实项目、会话、插件与当前会话正文。"),
        },
    ]
}

/// Creates command-session summaries for currently retained service backends.
/// 为当前保留的服务后端创建命令会话摘要。
fn command_sessions(controller: &ControllerRuntimeHandle) -> Vec<CommandSessionSummary> {
    vec![CommandSessionSummary {
        id: String::from("vldb-controller"),
        command: controller.executable.clone(),
        status: String::from("running"),
        preview: format!(
            "endpoint={} sqlite={}",
            controller.endpoint,
            controller.sqlite_path.display()
        ),
    }]
}

/// Creates runtime module cards shown by the dashboard.
/// 创建仪表盘展示的运行时模块卡片。
fn runtime_modules(
    controller: &ControllerRuntimeHandle,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
) -> Vec<RuntimeModuleSnapshot> {
    vec![
        runtime_module(
            "core",
            "Core Service",
            StartupStepStatus::Ready,
            "Tauri 本地服务运行中",
        ),
        runtime_module(
            "vldb",
            "VLDB",
            StartupStepStatus::Ready,
            &format!(
                "{} · {}",
                controller.endpoint,
                controller.sqlite_path.display()
            ),
        ),
        runtime_module(
            "luaskills",
            "LuaSkills",
            StartupStepStatus::Ready,
            &format!(
                "{} · 技能 {} · 入口 {}",
                luaskills_catalog.runtime_root.display(),
                luaskills_catalog.skills.len(),
                luaskills_catalog.entry_count
            ),
        ),
        runtime_module(
            "token-budget",
            "Token Budget",
            token_budget_status(token_budget_runtime),
            token_budget_runtime.status_row().as_str(),
        ),
        runtime_module(
            "mcp",
            "MCP Runtime",
            if mcp_catalog.enabled_server_count == 0 || mcp_catalog.connected_server_count > 0 {
                StartupStepStatus::Ready
            } else {
                StartupStepStatus::Warning
            },
            &format!(
                "server {}/{} · connected {} · tools {} · prompts {} · resources {}",
                mcp_catalog.enabled_server_count,
                mcp_catalog.configured_server_count,
                mcp_catalog.connected_server_count,
                mcp_catalog.tool_count,
                mcp_catalog.prompt_count,
                mcp_catalog.resource_count
            ),
        ),
        runtime_module(
            "workspace",
            "Workspace",
            StartupStepStatus::Ready,
            "项目与会话快照已恢复",
        ),
    ]
}

/// Resolves the startup status for token budget runtime diagnostics.
/// 解析 Token 预算运行时诊断对应的启动状态。
fn token_budget_status(token_budget_runtime: &TauriTokenBudgetRuntime) -> StartupStepStatus {
    if token_budget_runtime.is_available() {
        StartupStepStatus::Ready
    } else {
        StartupStepStatus::Warning
    }
}

/// Creates one runtime module snapshot.
/// 创建单个运行时模块快照。
fn runtime_module(
    id: &str,
    label: &str,
    status: StartupStepStatus,
    message: &str,
) -> RuntimeModuleSnapshot {
    RuntimeModuleSnapshot {
        id: id.to_string(),
        label: label.to_string(),
        status,
        message: message.to_string(),
    }
}

/// Appends one WebUI-visible service diagnostic message to the retained startup log buffer.
/// 向保留的启动日志缓冲区追加一条 WebUI 可见的服务诊断消息。
///
/// # Parameters
/// 参数
/// `state` owns the retained service snapshot.
/// `state` 持有保留的服务快照。
/// `message` is the user-visible diagnostic text.
/// `message` 是用户可见的诊断文本。
///
/// # Returns
/// 返回值
/// `Ok(())` after the diagnostic has been retained.
/// 诊断信息完成保留后返回 `Ok(())`。
fn push_webui_service_diagnostic(
    state: &SharedServiceBridgeState,
    message: String,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard.startup.recent_logs.push(message);
    if guard.startup.recent_logs.len() > 200 {
        let overflow = guard.startup.recent_logs.len().saturating_sub(200);
        guard.startup.recent_logs.drain(0..overflow);
    }
    Ok(())
}

/// Creates one diagnostic log entry.
/// 创建单条诊断日志。
fn log_entry(id: &str, level: &str, source: &str, message: &str) -> LogEntrySummary {
    LogEntrySummary {
        id: id.to_string(),
        level: level.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    }
}

/// Returns the repository root used by the local service bridge.
/// 返回本地服务桥接层使用的仓库根目录。
fn workspace_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Opens one native folder-picker dialog and returns the selected path when confirmed.
/// 打开一个原生文件夹选择器，并在确认后返回选中的路径。
fn pick_task_project_folder(initial_directory: &str) -> Result<Option<PathBuf>, String> {
    let mut dialog = rfd::FileDialog::new().set_title("选择要添加的项目文件夹");
    let initial_path = Path::new(initial_directory);
    if initial_path.exists() {
        dialog = dialog.set_directory(initial_path);
    }
    Ok(dialog.pick_folder())
}

/// Normalizes one project path by resolving it when the host filesystem allows it.
/// 在宿主文件系统允许时解析并规范化单个项目路径。
fn normalize_task_project_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Converts one filesystem path into a stable native display and persistence string.
/// 将单个文件系统路径转换成稳定的原生展示与持久化字符串。
fn normalized_workspace_path_text_from_path(path: &Path) -> String {
    normalize_workspace_path_text(&path.to_string_lossy())
}

/// Normalizes one raw workspace path string and strips Windows verbatim prefixes.
/// 规范化单个原始工作区路径字符串，并移除 Windows verbatim 前缀。
fn normalize_workspace_path_text(path: &str) -> String {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return String::new();
    }
    let path_without_verbatim_prefix = strip_windows_verbatim_prefix(trimmed_path);
    if looks_like_windows_path(trimmed_path, &path_without_verbatim_prefix) {
        path_without_verbatim_prefix.replace('/', "\\")
    } else {
        path_without_verbatim_prefix
    }
}

/// Returns whether one path should be displayed with Windows separators.
/// 返回单个路径是否应使用 Windows 分隔符显示。
fn looks_like_windows_path(original_path: &str, normalized_path: &str) -> bool {
    let bytes = normalized_path.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[2], b'/' | b'\\')
        || normalized_path.starts_with(r"\\")
        || original_path.starts_with(r"\\?\UNC\")
        || original_path.starts_with("//?/UNC/")
        || (cfg!(windows) && normalized_path.starts_with("//"))
}

/// Removes Windows verbatim path prefixes while preserving normal addressing semantics.
/// 移除 Windows verbatim 路径前缀，同时保留普通寻址语义。
fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    if let Some(rest) = path.strip_prefix("//?/") {
        return rest.to_string();
    }
    path.to_string()
}

/// Checks whether one project points to the normalized selected path.
/// 检查单个项目是否指向已规范化的选中路径。
fn task_project_matches_path(project: &TaskProjectSummary, normalized_path_text: &str) -> bool {
    normalized_workspace_path_text_from_path(&normalize_task_project_path(Path::new(&project.path)))
        == normalized_path_text
}

/// Builds a stable project id from the title and normalized path while avoiding collisions.
/// 根据标题与规范化路径构建稳定项目标识，并避免冲突。
fn build_task_project_id(
    workspace_snapshot: &WorkspaceSnapshot,
    title: &str,
    normalized_path_text: &str,
) -> String {
    let base_slug = slugify_identifier(title);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized_path_text.hash(&mut hasher);
    let path_suffix = format!("{:x}", hasher.finish());
    let path_suffix = &path_suffix[..path_suffix.len().min(8)];
    let candidate = if base_slug.is_empty() {
        format!("project-{path_suffix}")
    } else {
        format!("{base_slug}-{path_suffix}")
    };

    if !workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
        .any(|project| project.id == candidate)
    {
        return candidate;
    }

    let mut counter = 2usize;
    loop {
        let next_candidate = format!("{candidate}-{counter}");
        if !workspace_snapshot
            .task_projects
            .iter()
            .chain(workspace_snapshot.archived_task_projects.iter())
            .any(|project| project.id == next_candidate)
        {
            return next_candidate;
        }
        counter += 1;
    }
}

/// Converts one label into a compact ASCII identifier component.
/// 将单个标签转换为紧凑的 ASCII 标识片段。
fn slugify_identifier(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Returns the first existing path from a workspace-relative candidate list.
/// 从工作区相对候选列表中返回第一个存在的路径。
#[allow(dead_code)]
fn first_existing_path(workspace_root: &Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|candidate| workspace_root.join(candidate))
        .find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// Verifies WebUI only reports precheck progress when a concrete precheck turn is active.
    /// 验证 WebUI 只有在存在具体 precheck 回合时才报告 precheck 进度。
    #[test]
    fn webui_precheck_progress_requires_active_turn_id() {
        let mut runtime_state = SessionRuntimeStateEntry::default();

        assert!(!webui_pending_prompt_has_precheck_progress(&runtime_state));

        runtime_state.active_precheck_turn_id = Some(String::new());
        assert!(!webui_pending_prompt_has_precheck_progress(&runtime_state));

        runtime_state.active_precheck_turn_id = Some(String::from("turn-1"));
        assert!(webui_pending_prompt_has_precheck_progress(&runtime_state));
    }

    /// Verifies WebUI can rebuild runnable runtime state from visible messages after SQLite runtime storage is removed.
    /// 验证移除 SQLite 运行态存储后，WebUI 可从可见消息重建可运行的运行态。
    #[test]
    fn ensure_webui_session_runtime_state_rebuilds_missing_precheck_state() {
        let session_id = SessionId::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "新会话",
                "",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(
                session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::User,
                    "用户",
                    "第一条提示词",
                )],
            )]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::MemoryRecall)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, Some("turn-1"));

        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should be rebuilt");
        assert_eq!(
            runtime_state.active_precheck_turn_id.as_deref(),
            Some("turn-1")
        );
        assert!(runtime_state_has_concrete_conversation_context(
            runtime_state
        ));
        assert!(
            workspace_snapshot
                .hydrated_session_ids
                .contains(&session_id)
        );
        assert_eq!(
            workspace_snapshot
                .session_messages
                .get(&session_id)
                .map(Vec::len),
            Some(1)
        );
    }

    /// Verifies WebUI regular-session detection excludes persisted TaskSession identifiers from workspace-store writes.
    /// 验证 WebUI 普通会话检测会把持久化 TaskSession 标识排除在 workspace-store 写入之外。
    #[test]
    fn webui_regular_session_detection_excludes_task_session_ids() {
        let active_session_id = SessionId::new_v4();
        let archived_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                active_session_id,
                Some(String::from("project-a")),
                "活动会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: vec![SessionSummary::new(
                archived_session_id,
                Some(String::from("project-a")),
                "归档会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: active_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        assert!(workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            active_session_id
        ));
        assert!(workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            archived_session_id
        ));
        assert!(!workspace_snapshot_contains_regular_session(
            &workspace_snapshot,
            task_session_id
        ));
    }

    /// Verifies stale workspace refreshes cannot drop an in-memory prompt turn that has not reached disk.
    /// 验证陈旧工作区刷新不能丢弃尚未落盘的内存提示词回合。
    #[test]
    fn preserve_active_session_run_state_keeps_new_precheck_runtime_state() {
        let session_id = SessionId::new_v4();
        let project_id = String::from("project-a");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.active_precheck_turn_id = Some(String::from("turn-1"));
        let previous_snapshot = WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                project_id.clone(),
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                vec![session_id],
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(project_id.clone()),
                "新会话",
                "正在回忆",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(
                session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::User,
                    "用户",
                    "新提示词",
                )],
            )]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::MemoryRecall)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::from([session_id]),
        };
        let mut refreshed_snapshot = WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                project_id,
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                Vec::new(),
            )],
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
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        };

        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            Some(&previous_snapshot),
            &BTreeSet::from([session_id]),
            Some(session_id),
        );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );

        assert_eq!(refreshed_snapshot.selected_session, session_id);
        assert!(
            refreshed_snapshot
                .sessions
                .iter()
                .any(|session| session.id == session_id)
        );
        assert_eq!(
            refreshed_snapshot
                .session_runtime_state
                .get(&session_id)
                .and_then(|state| state.active_precheck_turn_id.as_deref()),
            Some("turn-1")
        );
        assert_eq!(
            refreshed_snapshot.session_run_phases.get(&session_id),
            Some(&SessionRunPhase::MemoryRecall)
        );
    }

    /// Verifies submitted sessions preserve in-memory context even without an active kernel turn.
    /// 验证提交目标会话即使没有活动内核回合也会保留内存上下文。
    #[test]
    fn preserve_active_session_run_state_keeps_submitted_session_context() {
        let session_id = SessionId::new_v4();
        let project_id = String::from("project-a");
        let previous_snapshot = WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                project_id.clone(),
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                vec![session_id],
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(project_id.clone()),
                "内存会话",
                "刚输入",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(
                session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::Assistant,
                    "助手",
                    "尚未落盘的回答",
                )],
            )]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::from([session_id]),
        };
        let mut refreshed_snapshot = WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                project_id,
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                Vec::new(),
            )],
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
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        };

        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            Some(&previous_snapshot),
            &BTreeSet::new(),
            Some(session_id),
        );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );

        assert_eq!(refreshed_snapshot.selected_session, session_id);
        assert!(refreshed_snapshot.sessions.iter().any(|session| {
            session.id == session_id && session.status == SessionState::Completed
        }));
        assert_eq!(
            refreshed_snapshot
                .task_projects
                .first()
                .map(|project| project.session_ids.as_slice()),
            Some(&[session_id][..])
        );
        assert_eq!(
            refreshed_snapshot
                .session_messages
                .get(&session_id)
                .map(Vec::len),
            Some(1)
        );
    }

    /// Verifies stale running markers do not revive a session without an active kernel turn.
    /// 验证没有活动内核回合时过时运行标记不会复活会话。
    #[test]
    fn preserve_active_session_run_state_ignores_stale_running_without_kernel() {
        let session_id = SessionId::new_v4();
        let previous_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "旧运行会话",
                "仍显示运行",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        let mut refreshed_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "旧运行会话",
                "已刷新",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        };

        let preserved_workspace_run_state =
            capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );

        let session = refreshed_snapshot
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .expect("session should still exist");
        assert_eq!(session.status, SessionState::Completed);
        assert!(
            !refreshed_snapshot
                .session_run_phases
                .contains_key(&session_id)
        );
    }

    /// Verifies TaskSession terminal reports use the memory-only fast path without completed-turn hooks.
    /// 验证 TaskSession 终态报告使用仅内存快路径且不会生成完成回合 hook。
    #[test]
    fn task_session_execution_report_fast_path_skips_completion_hook() {
        let root_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                root_session_id,
                Some(String::from("project-a")),
                "主会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: root_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        let state = Arc::new(Mutex::new(ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: Some(workspace_snapshot),
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_navigation_rounds: BTreeSet::new(),
            active_subagent_panel: None,
            detached_subagent_task_listeners: BTreeSet::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        }));
        let report = ExecutionRunReport {
            session_id: task_session_id,
            preparation: None,
            assistant_output: Some(String::from("子代理完成")),
            thinking_output: None,
            conversation_state: None,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            controller_events: Vec::new(),
        };

        let applied = apply_task_session_execution_report_to_memory(&state, report).unwrap();

        assert!(applied.completion_request.is_none());
        assert!(!workspace_snapshot_contains_regular_session(
            &applied.workspace_snapshot,
            task_session_id
        ));
        assert_eq!(
            applied
                .workspace_snapshot
                .session_run_phases
                .get(&task_session_id),
            Some(&SessionRunPhase::Ready)
        );
        let messages = applied
            .workspace_snapshot
            .session_messages
            .get(&task_session_id)
            .expect("task session report should materialize messages");
        assert!(
            messages
                .iter()
                .any(|message| message.body.contains("子代理完成"))
        );
        let guard = state.lock().unwrap();
        assert!(
            guard
                .workspace_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.hydrated_session_ids.contains(&task_session_id))
        );
    }

    /// Verifies a live detached listener keeps the parent subagent navigation visible after the parent stream finishes.
    /// 验证父级流结束后，实时分离监听器仍会让父级子代理导航保持可见。
    #[test]
    fn webui_detached_listener_keeps_running_subagent_navigation_visible() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut guard = ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::from([(
                parent_session_id,
                BTreeMap::from([(
                    task_session_id,
                    SubagentTaskPanelState {
                        task_id: task_session_id,
                        public_task_id: String::from("image-worker"),
                        parent_session_id,
                        subagent_label: String::from("图片子代理"),
                        description: Some(String::from("生成图片")),
                        run_state: SubagentTaskRunState::Running,
                        result_text: String::new(),
                        conversation_state: None,
                    },
                )]),
            )]),
            session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
            active_subagent_panel: None,
            detached_subagent_task_listeners: BTreeSet::from([task_session_id]),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        };
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                parent_session_id,
                Some(String::from("project-a")),
                "父会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
            .expect("running subagent navigation should stay visible");

        assert_eq!(
            navigation.parent_conversation_id,
            parent_session_id.to_string()
        );
        assert_eq!(
            navigation.active_conversation_id,
            parent_session_id.to_string()
        );
        assert_eq!(navigation.tasks.len(), 1);
        assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
        assert!(navigation.tasks[0].is_running);
        assert!(session_has_running_subagent_tasks_from_cache(
            &mut guard,
            &workspace_snapshot,
            parent_session_id
        ));
    }

    /// Verifies two detached WebUI subagents stop blocking the parent turn after both reach terminal states.
    /// 验证两个 WebUI 分离子代理全部进入终态后不再阻塞父级回合收尾。
    #[test]
    fn webui_completion_gate_clears_after_two_detached_subagents_finish() {
        let parent_session_id = SessionId::new_v4();
        let first_task_session_id = SessionId::new_v4();
        let second_task_session_id = SessionId::new_v4();
        let mut guard = ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::from([(
                parent_session_id,
                BTreeMap::from([
                    (
                        first_task_session_id,
                        SubagentTaskPanelState {
                            task_id: first_task_session_id,
                            public_task_id: String::from("image-worker-a"),
                            parent_session_id,
                            subagent_label: String::from("图片子代理 A"),
                            description: Some(String::from("生成第一张图片")),
                            run_state: SubagentTaskRunState::Running,
                            result_text: String::new(),
                            conversation_state: None,
                        },
                    ),
                    (
                        second_task_session_id,
                        SubagentTaskPanelState {
                            task_id: second_task_session_id,
                            public_task_id: String::from("image-worker-b"),
                            parent_session_id,
                            subagent_label: String::from("图片子代理 B"),
                            description: Some(String::from("生成第二张图片")),
                            run_state: SubagentTaskRunState::Running,
                            result_text: String::new(),
                            conversation_state: None,
                        },
                    ),
                ]),
            )]),
            session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
            active_subagent_panel: None,
            detached_subagent_task_listeners: BTreeSet::from([
                first_task_session_id,
                second_task_session_id,
            ]),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        };
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                parent_session_id,
                Some(String::from("project-a")),
                "父会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let running_tasks = running_webui_subagent_task_states_for_tree_from_cache(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
        );
        assert_eq!(running_tasks.len(), 2);
        let running_gate = WebuiTurnCompletionGate {
            pending_subagent_count: running_tasks.len(),
            has_active_hook_turn: false,
        };
        assert!(matches!(
            running_gate.completion_status(),
            ConversationTurnCompletionStatus::AwaitingSubagents
        ));

        for task_session_id in [first_task_session_id, second_task_session_id] {
            let task_state = guard
                .session_subagent_tasks
                .get_mut(&parent_session_id)
                .and_then(|tasks| tasks.get_mut(&task_session_id))
                .expect("test subagent state should exist");
            task_state.run_state = SubagentTaskRunState::Completed;
            task_state.result_text = format!("任务 {task_session_id} 已完成");
            guard
                .detached_subagent_task_listeners
                .remove(&task_session_id);
        }

        let finished_tasks = running_webui_subagent_task_states_for_tree_from_cache(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
        );
        assert!(finished_tasks.is_empty());
        assert!(!session_has_running_subagent_tasks_from_cache(
            &mut guard,
            &workspace_snapshot,
            parent_session_id
        ));
        let finished_gate = WebuiTurnCompletionGate {
            pending_subagent_count: finished_tasks.len(),
            has_active_hook_turn: false,
        };
        assert!(matches!(
            finished_gate.completion_status(),
            ConversationTurnCompletionStatus::Completed
        ));
    }

    /// Verifies stopped parents never dispatch queued work when detached subagents report terminal events.
    /// 验证已停止父会话收到分离子代理终态事件时绝不会调度排队工作。
    #[test]
    fn stopped_detached_parent_event_skips_pending_prompt_dispatch() {
        assert!(webui_detached_parent_event_should_continue_pending_prompt(
            true, false
        ));
        assert!(!webui_detached_parent_event_should_continue_pending_prompt(
            true, true
        ));
        assert!(!webui_detached_parent_event_should_continue_pending_prompt(
            false, false
        ));
        assert!(webui_detached_parent_event_should_persist_terminal_projection(true, true, false));
        assert!(!webui_detached_parent_event_should_persist_terminal_projection(true, true, true));
        assert!(
            !webui_detached_parent_event_should_persist_terminal_projection(false, true, false)
        );
    }

    /// Verifies detached subagent completion enqueues parent mailbox input without using the pending prompt queue.
    /// 验证没有 wait_agent 接管时，分离子代理完成会写入父级 mailbox 而不使用 pending prompt 队列。
    #[test]
    fn subagent_completion_queues_session_mailbox_without_wait_agent() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
            parent_session_id,
        )));
        let event = test_delegate_finished_event(task_session_id, "image-a", "生成完成：a.jpg");

        let queued = enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &event,
            true,
            false,
        )
        .expect("enqueue mailbox input");

        assert!(queued);
        let mailbox_prompt = webui_session_mailbox_input_from_kernel(
            &state,
            parent_session_id,
            Some(&task_session_id.to_string()),
        )
        .expect("mailbox prompt should read")
        .expect("mailbox prompt should exist");
        assert!(mailbox_prompt.contains("生成完成：a.jpg"));
        let guard = state.lock().expect("state lock");
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
        assert!(runtime_state.is_none_or(|entry| entry.ui_state.pending_prompt_queue.is_empty()));
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("parent session kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        let mailbox_inputs = kernel.input_queue().get_mailbox_input();
        assert_eq!(mailbox_inputs.len(), 1);
        assert_eq!(mailbox_inputs[0].kind, TurnInputKind::SubagentMessage);
        assert!(mailbox_inputs[0].trigger_turn);
        assert_eq!(
            mailbox_inputs[0].payload["mailbox_event"]["payload"]["result"],
            "生成完成：a.jpg"
        );
    }

    /// Verifies detached subagent completion does not enqueue mailbox input while wait_agent is active.
    /// 验证 wait_agent 活跃期间，分离子代理完成不会额外加入 mailbox 输入。
    #[test]
    fn subagent_completion_skips_session_mailbox_during_wait_agent() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        service_state
            .parent_sessions_waiting_for_subagents
            .insert(parent_session_id);
        let state = Arc::new(Mutex::new(service_state));
        let event = test_delegate_finished_event(task_session_id, "image-b", "生成完成：b.jpg");

        let queued = enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &event,
            true,
            false,
        )
        .expect("skip mailbox input");

        assert!(!queued);
        let guard = state.lock().expect("state lock");
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
        assert!(runtime_state.is_none_or(|entry| entry.ui_state.pending_prompt_queue.is_empty()));
    }

    /// Verifies child completion mailbox input stays suppressed after a completed wait_agent result.
    /// 验证 wait_agent 已完成收集后，子代理完成 mailbox 输入仍保持抑制。
    #[test]
    fn subagent_completion_skips_session_mailbox_after_completed_wait_agent() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        service_state
            .parent_session_completed_wait_agent_selectors
            .insert(parent_session_id, BTreeSet::new());
        let state = Arc::new(Mutex::new(service_state));
        let event = test_delegate_finished_event(task_session_id, "image-c", "生成完成：c.jpg");

        let queued = enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &event,
            true,
            false,
        )
        .expect("skip mailbox input");

        assert!(!queued);
        let guard = state.lock().expect("state lock");
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
        assert!(runtime_state.is_none_or(|entry| entry.ui_state.pending_prompt_queue.is_empty()));
    }

    /// Verifies a completed wait_agent only suppresses matching child-agent completions.
    /// 验证已完成的 wait_agent 只抑制匹配的子代理完成事件。
    #[test]
    fn subagent_completion_after_completed_wait_agent_allows_unmatched_agent() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        service_state
            .parent_session_completed_wait_agent_selectors
            .insert(
                parent_session_id,
                BTreeSet::from([String::from("agent-waited")]),
            );
        let state = Arc::new(Mutex::new(service_state));
        let event = test_delegate_finished_event(task_session_id, "agent-unmatched", "done");

        let queued = enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &event,
            true,
            false,
        )
        .expect("unmatched child should enqueue mailbox input");

        assert!(queued);
        let guard = state.lock().expect("state lock");
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    }

    /// Builds a minimal service state containing one regular parent session.
    /// 构建只包含一个常规父会话的最小服务状态。
    fn test_service_state_with_parent_session(parent_session_id: SessionId) -> ServiceBridgeState {
        ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: Some(WorkspaceSnapshot {
                task_projects: Vec::new(),
                archived_task_projects: Vec::new(),
                sessions: vec![SessionSummary::new(
                    parent_session_id,
                    Some(String::from("project-a")),
                    "父会话",
                    "",
                    SessionState::Completed,
                    0,
                    0,
                    "刚刚",
                )],
                archived_sessions: Vec::new(),
                session_messages: BTreeMap::new(),
                archived_session_messages: BTreeMap::new(),
                session_compactions: BTreeMap::new(),
                archived_session_compactions: BTreeMap::new(),
                session_runtime_state: BTreeMap::new(),
                archived_session_runtime_state: BTreeMap::new(),
                session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
                expanded_task_projects: BTreeSet::new(),
                selected_session: parent_session_id,
                hydrated_session_ids: BTreeSet::new(),
            }),
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_navigation_rounds: BTreeSet::new(),
            active_subagent_panel: None,
            detached_subagent_task_listeners: BTreeSet::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        }
    }

    /// Verifies the WebUI session kernel helper enforces one active turn.
    /// 验证 WebUI 会话内核辅助函数会强制单活动回合。
    #[test]
    fn webui_session_kernel_rejects_second_active_turn() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("first kernel turn should start");
        let second_start = start_webui_session_kernel_user_turn(&mut state, session_id, "turn-b");

        assert!(second_start.is_err());
        assert!(
            state
                .session_runtime_kernels
                .get(&session_id)
                .expect("kernel should exist")
                .has_active_turn()
        );
    }

    /// Verifies a new user turn clears stale wait-agent coordination from earlier turns.
    /// 验证新的用户回合会清理旧回合残留的 wait_agent 协调状态。
    #[test]
    fn webui_session_kernel_user_turn_clears_wait_agent_coordination() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);
        state
            .parent_sessions_waiting_for_subagents
            .insert(session_id);
        state
            .parent_session_completed_wait_agent_selectors
            .insert(session_id, BTreeSet::from([String::from("agent-a")]));
        session_runtime_kernel_mut(&mut state, session_id)
            .register_wait_scope("wait-a", vec![String::from("agent-a")], 10)
            .expect("wait scope should register");

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("kernel turn should start");

        assert!(
            !state
                .parent_sessions_waiting_for_subagents
                .contains(&session_id)
        );
        assert!(
            !state
                .parent_session_completed_wait_agent_selectors
                .contains_key(&session_id)
        );
        assert_eq!(
            state
                .session_runtime_kernels
                .get(&session_id)
                .expect("kernel should exist")
                .wait_scope_count(),
            0
        );
    }

    /// Verifies followup_task starts clear stale wait-agent completion selectors.
    /// 验证 followup_task 开始时会清理旧 wait_agent 完成选择器。
    #[test]
    fn followup_task_started_clears_completed_wait_agent_selectors() {
        let session_id = SessionId::new_v4();
        let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
            session_id,
        )));
        state
            .lock()
            .expect("state should lock")
            .parent_session_completed_wait_agent_selectors
            .insert(session_id, BTreeSet::from([String::from("agent-a")]));
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "followup_task started",
            "followup_task",
        )
        .with_metadata(Some(json!({
            "tool_name": "followup_task",
            "tool_call_id": "call-followup",
            "arguments": { "target": "agent-a", "message": "continue" }
        })));

        update_webui_wait_agent_runtime_state(&state, session_id, &event);

        assert!(
            !state
                .lock()
                .expect("state should lock")
                .parent_session_completed_wait_agent_selectors
                .contains_key(&session_id)
        );
    }

    /// Verifies kernel-active sessions are considered actively running without legacy stream handles.
    /// 验证即使没有旧流任务句柄，内核活动中的会话也会被视为正在运行。
    #[test]
    fn webui_session_active_run_uses_kernel_state_without_stream_handle() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("kernel turn should start");

        assert!(!state.active_stream_tasks.contains_key(&session_id));
        assert!(webui_session_has_active_run(&state, session_id));
    }

    /// Verifies a stopped kernel suppresses stale legacy stream handles for active-run decisions.
    /// 验证已停止内核会在活动运行判断中抑制过期旧流任务句柄。
    #[test]
    fn webui_session_stopped_kernel_suppresses_stale_stream_handle() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("kernel turn should start");
        state
            .active_stream_tasks
            .insert(session_id, tauri::async_runtime::spawn(async {}));
        stop_webui_session_kernel_generation(&mut state, session_id);

        assert!(state.active_stream_tasks.contains_key(&session_id));
        assert!(!webui_session_has_active_run(&state, session_id));
    }

    /// Verifies legacy stream handles alone cannot become the active-run fact source.
    /// 验证旧流任务句柄本身不能成为活动运行事实源。
    #[test]
    fn webui_session_active_run_ignores_stream_handle_without_kernel_turn() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        state
            .active_stream_tasks
            .insert(session_id, tauri::async_runtime::spawn(async {}));

        assert!(state.active_stream_tasks.contains_key(&session_id));
        assert!(!webui_session_has_active_run(&state, session_id));
    }

    /// Verifies finishing a WebUI session kernel turn clears the active turn.
    /// 验证结束 WebUI 会话内核回合会清理活动回合。
    #[test]
    fn webui_session_kernel_finish_clears_active_turn() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("kernel turn should start");
        finish_webui_session_kernel_turn(&mut state, session_id, TurnTerminalStatus::Completed);

        assert!(
            !state
                .session_runtime_kernels
                .get(&session_id)
                .expect("kernel should exist")
                .has_active_turn()
        );
    }

    /// Verifies stopping a WebUI session kernel generation clears the active turn and marks stopped.
    /// 验证停止 WebUI 会话内核代际会清理活动回合并标记停止。
    #[test]
    fn webui_session_kernel_stop_clears_active_turn_and_marks_stopped() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("kernel turn should start");
        stop_webui_session_kernel_generation(&mut state, session_id);

        let kernel = state
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist");
        assert!(!kernel.has_active_turn());
        assert!(kernel.is_stopped());
    }

    /// Verifies stream events are accepted only while their captured kernel generation is current.
    /// 验证流事件仅在捕获的内核代际仍为当前代际时被接受。
    #[test]
    fn webui_stream_event_generation_gate_rejects_stopped_generation() {
        let session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(session_id);
        start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
            .expect("kernel turn should start");
        let expected_generation =
            webui_session_kernel_generation(&service_state, session_id).expect("generation");
        let state = Arc::new(Mutex::new(service_state));
        let event = ExecutionStreamEvent::RequestStarted { session_id };

        assert!(webui_stream_event_matches_kernel_generation(
            &state,
            &event,
            Some(expected_generation)
        ));
        {
            let mut guard = state.lock().expect("state lock");
            stop_webui_session_kernel_generation(&mut guard, session_id);
        }

        assert!(!webui_stream_event_matches_kernel_generation(
            &state,
            &event,
            Some(expected_generation)
        ));
    }

    /// Verifies subagent mailbox input uses the generation captured when the child was spawned.
    /// 验证子代理 mailbox 输入使用子任务创建时捕获的父级代际。
    #[test]
    fn webui_subagent_mailbox_rejects_child_result_from_stale_parent_generation() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(parent_session_id);
        let metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "payload": {
                        "result_text": "old child done"
                    }
                }
            }
        });

        start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-a")
            .expect("first parent turn should start");
        bind_webui_subagent_parent_turn(&mut state, parent_session_id, task_session_id);
        let old_generation =
            webui_session_kernel_generation(&state, parent_session_id).expect("generation");
        stop_webui_session_kernel_generation(&mut state, parent_session_id);
        start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-b")
            .expect("second parent turn should start");

        assert_ne!(
            old_generation,
            webui_session_kernel_generation(&state, parent_session_id).expect("new generation")
        );
        assert!(!enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &metadata,
        ));
        let kernel = state
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 0);
    }

    /// Verifies a mailbox-triggered kernel turn accepts queued subagent mailbox input.
    /// 验证 mailbox 触发的内核回合会接收已排队的子代理 mailbox 输入。
    #[test]
    fn webui_session_kernel_mailbox_turn_accepts_subagent_input() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(parent_session_id);
        let metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "payload": {
                        "result_text": "done"
                    }
                }
            }
        });

        assert!(enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &metadata,
        ));
        start_webui_session_kernel_turn(
            &mut state,
            parent_session_id,
            "mailbox-turn",
            ActiveTurnKind::MailboxTriggered,
        )
        .expect("mailbox turn should start");

        let kernel = state
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 0);
        assert_eq!(kernel.input_queue().current_turn_len(), 1);
        assert_eq!(
            kernel.input_queue().get_pending_input()[0].kind,
            TurnInputKind::SubagentMessage
        );
    }

    /// Verifies sibling subagents spawned by one parent turn can still report during a mailbox-triggered turn.
    /// 验证同一父回合派生的兄弟子代理在 mailbox 回流回合期间仍可完成回报。
    #[test]
    fn webui_subagent_mailbox_accepts_sibling_during_mailbox_turn() {
        let parent_session_id = SessionId::new_v4();
        let first_task_session_id = SessionId::new_v4();
        let second_task_session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(parent_session_id);
        let first_metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "payload": {
                        "result_text": "first child done"
                    }
                }
            }
        });
        let second_metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "payload": {
                        "result_text": "second child done"
                    }
                }
            }
        });

        start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-a")
            .expect("parent user turn should start");
        bind_webui_subagent_parent_turn(&mut state, parent_session_id, first_task_session_id);
        bind_webui_subagent_parent_turn(&mut state, parent_session_id, second_task_session_id);
        let parent_generation =
            webui_session_kernel_generation(&state, parent_session_id).expect("generation");
        finish_webui_session_kernel_turn(
            &mut state,
            parent_session_id,
            TurnTerminalStatus::Completed,
        );
        assert!(enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            first_task_session_id,
            &first_metadata,
        ));
        start_webui_session_kernel_turn(
            &mut state,
            parent_session_id,
            "mailbox-turn",
            ActiveTurnKind::MailboxTriggered,
        )
        .expect("mailbox turn should start");

        assert_eq!(
            webui_session_kernel_generation(&state, parent_session_id),
            Some(parent_generation)
        );
        assert!(enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            second_task_session_id,
            &second_metadata,
        ));
        let kernel = state
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        let active_inputs = kernel.input_queue().get_unconsumed_mailbox_input();
        assert_eq!(active_inputs.len(), 2);
        assert!(active_inputs.iter().any(
            |input| input.payload["mailbox_event"]["payload"]["result_text"] == "second child done"
        ));
    }

    /// Verifies wait_agent completion consumes matching mailbox input and suppresses auto-trigger.
    /// 验证 wait_agent 完成后会消费匹配的 mailbox 输入并抑制自动触发。
    #[test]
    fn wait_agent_completion_consumes_matching_mailbox_input() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        let metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "child_thread_id": task_session_id.to_string(),
                    "payload": {
                        "task_name": "agent-review",
                        "task_session_id": task_session_id.to_string(),
                        "result_text": "done"
                    }
                }
            }
        });
        assert!(enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        ));
        let state = Arc::new(Mutex::new(service_state));
        let started_event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "wait_agent started",
            "waiting",
        )
        .with_metadata(Some(serde_json::json!({
            "tool_name": "wait_agent",
            "tool_call_id": "wait-call-1",
            "arguments": {
                "task_name": "agent-review"
            }
        })));
        update_webui_wait_agent_runtime_state(&state, parent_session_id, &started_event);
        let finished_event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "wait_agent finished",
            "finished",
        )
        .with_metadata(Some(serde_json::json!({
            "tool_name": "wait_agent",
            "tool_call_id": "wait-call-1",
            "host_result": {
                "kind": "subagent_status",
                "payload": {
                    "completed": true,
                    "agents": [{
                        "task_name": "agent-review",
                        "task_session_id": task_session_id.to_string(),
                        "status": "completed"
                    }]
                }
            }
        })));

        update_webui_wait_agent_runtime_state(&state, parent_session_id, &finished_event);

        let guard = state.lock().expect("state lock");
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
        assert_eq!(kernel.input_queue().consumed_input_count(), 1);
        assert_eq!(kernel.wait_scope_count(), 0);
        assert!(
            guard
                .parent_session_completed_wait_agent_selectors
                .contains_key(&parent_session_id)
        );
    }

    /// Verifies wait_agent completion consumes unclaimed mailbox input when host selectors are absent.
    /// 验证 wait_agent 完成但宿主选择器缺失时会消费未认领的 mailbox 输入。
    #[test]
    fn wait_agent_completion_consumes_mailbox_input_without_selectors() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        let metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "child_thread_id": task_session_id.to_string(),
                    "payload": {
                        "task_name": "agent-image",
                        "task_session_id": task_session_id.to_string(),
                        "result_text": "done"
                    }
                }
            }
        });
        assert!(enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        ));
        let state = Arc::new(Mutex::new(service_state));
        let started_event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "wait_agent started",
            "waiting",
        )
        .with_metadata(Some(serde_json::json!({
            "tool_name": "wait_agent",
            "tool_call_id": "wait-call-no-selectors"
        })));
        update_webui_wait_agent_runtime_state(&state, parent_session_id, &started_event);
        let finished_event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "wait_agent finished",
            "finished",
        )
        .with_metadata(Some(serde_json::json!({
            "tool_name": "wait_agent",
            "tool_call_id": "wait-call-no-selectors",
            "host_result": {
                "kind": "subagent_status",
                "payload": {
                    "completed": true
                }
            }
        })));

        update_webui_wait_agent_runtime_state(&state, parent_session_id, &finished_event);

        let guard = state.lock().expect("state lock");
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
        assert_eq!(kernel.input_queue().consumed_input_count(), 1);
        assert_eq!(kernel.wait_scope_count(), 0);
    }

    /// Verifies a late delegate-finished mailbox input is skipped after wait_agent already completed.
    /// 验证 wait_agent 已完成后迟到的委派完成 mailbox 输入会被跳过。
    #[test]
    fn session_mailbox_skips_late_delegate_finished_after_completed_wait_agent() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        service_state
            .parent_session_completed_wait_agent_selectors
            .insert(
                parent_session_id,
                BTreeSet::from([String::from("agent-review")]),
            );
        let state = Arc::new(Mutex::new(service_state));
        let event = test_delegate_finished_event(task_session_id, "agent-review", "done");

        let queued = enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &event,
            true,
            false,
        )
        .expect("late mailbox input should be checked");

        assert!(!queued);
        let guard = state.lock().expect("state lock");
        assert!(
            guard
                .session_runtime_kernels
                .get(&parent_session_id)
                .is_none_or(|kernel| kernel.input_queue().mailbox_len() == 0)
        );
    }

    /// Verifies stopped parent sessions never enqueue session mailbox input.
    /// 验证已停止的父会话绝不会加入会话 mailbox 输入。
    #[test]
    fn session_mailbox_skips_stopped_parent_session() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
            parent_session_id,
        )));
        let event = test_delegate_finished_event(task_session_id, "agent-review", "done");

        let queued = enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &event,
            true,
            true,
        )
        .expect("stopped mailbox input should be checked");

        assert!(!queued);
        let guard = state.lock().expect("state lock");
        assert!(
            guard
                .session_runtime_kernels
                .get(&parent_session_id)
                .is_none_or(|kernel| kernel.input_queue().mailbox_len() == 0)
        );
    }

    /// Verifies provider-loop mailbox intervention consumes unclaimed child-agent messages.
    /// 验证 Provider 循环 mailbox 干预会消费未被 wait 接管的子代理消息。
    #[test]
    fn mailbox_intervention_prompt_consumes_unclaimed_mailbox_input() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        let metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "child_thread_id": task_session_id.to_string(),
                    "payload": {
                        "task_name": "agent-image",
                        "task_session_id": task_session_id.to_string(),
                        "result_text": "image done"
                    }
                }
            }
        });
        assert!(enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        ));
        let state = Arc::new(Mutex::new(service_state));

        let prompt = take_webui_mailbox_intervention_prompt(&state, parent_session_id)
            .expect("mailbox intervention should read")
            .expect("mailbox intervention prompt should exist");

        assert!(prompt.contains("<subagent_mailbox_event>"));
        assert!(prompt.contains("image done"));
        let guard = state.lock().expect("state lock");
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
        assert_eq!(kernel.input_queue().consumed_input_count(), 1);
    }

    /// Verifies provider-loop mailbox intervention leaves wait-agent-owned messages untouched.
    /// 验证 Provider 循环 mailbox 干预不会消费 wait_agent 接管的消息。
    #[test]
    fn mailbox_intervention_prompt_skips_active_wait_scope() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut service_state = test_service_state_with_parent_session(parent_session_id);
        let metadata = serde_json::json!({
            "payload": {
                "subagent_mailbox_event": {
                    "kind": "completed",
                    "child_thread_id": task_session_id.to_string(),
                    "payload": {
                        "task_name": "agent-image",
                        "task_session_id": task_session_id.to_string(),
                        "result_text": "image done"
                    }
                }
            }
        });
        assert!(enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        ));
        session_runtime_kernel_mut(&mut service_state, parent_session_id)
            .register_wait_scope(
                "wait-agent",
                [task_session_id.to_string()],
                current_unix_millis_u64(),
            )
            .expect("wait scope should register");
        let state = Arc::new(Mutex::new(service_state));

        let prompt = take_webui_mailbox_intervention_prompt(&state, parent_session_id)
            .expect("mailbox intervention should read");

        assert!(prompt.is_none());
        let guard = state.lock().expect("state lock");
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
        assert_eq!(kernel.input_queue().consumed_input_count(), 0);
    }

    /// Builds one delegate-finished runtime event carrying task-session metadata.
    /// 构建一条携带 task-session 元数据的委派完成运行时事件。
    fn test_delegate_finished_event(
        task_session_id: SessionId,
        public_task_id: &str,
        result: &str,
    ) -> RuntimeEvent {
        RuntimeEvent::new(
            RuntimeEventKind::DelegateFinished,
            "子代理完成",
            "子代理任务完成。",
        )
        .with_metadata(Some(serde_json::json!({
            "kind": "task_session",
            "payload": {
                "task_id": public_task_id,
                "task_session_id": task_session_id.to_string(),
                "execution_status": "completed",
                "description": "生成图片",
                "subagent": "image-worker",
                "result": result,
            }
        })))
    }

    /// Verifies token-refresh projection can reuse existing subagent navigation without rebuilding the task list.
    /// 验证 token 刷新投影可以复用现有子代理导航而不重建任务列表。
    #[test]
    fn preserved_subagent_navigation_refresh_reads_child_surface_without_task_list_rebuild() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let guard = ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_navigation_rounds: BTreeSet::new(),
            active_subagent_panel: None,
            detached_subagent_task_listeners: BTreeSet::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        };
        let child_message =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "子代理增量")
                .with_runtime_metadata("child-output", "1000");
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                parent_session_id,
                Some(String::from("project-a")),
                "父会话",
                "",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(task_session_id, vec![child_message.clone()])]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::from([task_session_id]),
        };
        let navigation = SubagentNavigationSnapshot {
            parent_conversation_id: parent_session_id.to_string(),
            active_conversation_id: task_session_id.to_string(),
            active_task_id: Some(task_session_id.to_string()),
            tasks: Vec::new(),
        };

        let active_session_id =
            preserved_active_surface_session_id(&workspace_snapshot, Some(&navigation));
        let surface_messages = surface_messages_for_session_without_subagent_navigation_refresh(
            &guard,
            &workspace_snapshot,
            active_session_id,
        );

        assert_eq!(active_session_id, task_session_id);
        assert_eq!(surface_messages, vec![child_message]);
        assert!(guard.session_subagent_tasks.is_empty());
    }

    /// Verifies historical cached subagents do not slow or hijack a normal parent-session surface.
    /// 验证历史缓存子代理不会拖慢或劫持普通父会话界面。
    #[test]
    fn historical_subagent_cache_without_visible_round_does_not_build_navigation() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut guard = ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::from([(
                parent_session_id,
                BTreeMap::from([(
                    task_session_id,
                    SubagentTaskPanelState {
                        task_id: task_session_id,
                        public_task_id: String::from("historical-task"),
                        parent_session_id,
                        subagent_label: String::from("历史子代理"),
                        description: None,
                        run_state: SubagentTaskRunState::Running,
                        result_text: String::new(),
                        conversation_state: None,
                    },
                )]),
            )]),
            session_subagent_navigation_rounds: BTreeSet::new(),
            active_subagent_panel: None,
            detached_subagent_task_listeners: BTreeSet::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        };
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                parent_session_id,
                Some(String::from("project-a")),
                "父会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        assert_eq!(
            active_surface_session_id_from_state(&mut guard, &workspace_snapshot),
            parent_session_id
        );
        assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
        assert!(
            subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
                .is_none()
        );
    }

    /// Verifies a stale WebUI subagent panel cannot hijack another selected conversation surface.
    /// 验证 WebUI 残留的子代理面板不能劫持另一个已选会话的显示界面。
    #[test]
    fn webui_active_subagent_panel_cannot_cross_selected_session() {
        let selected_session_id = SessionId::new_v4();
        let previous_session_id = SessionId::new_v4();
        let previous_task_id = SessionId::new_v4();
        let mut guard = ServiceBridgeState {
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            agent_runtime: None,
            service_runtime: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::from([(
                previous_session_id,
                BTreeMap::from([(
                    previous_task_id,
                    SubagentTaskPanelState {
                        task_id: previous_task_id,
                        public_task_id: String::from("previous-task"),
                        parent_session_id: previous_session_id,
                        subagent_label: String::from("旧子代理"),
                        description: None,
                        run_state: SubagentTaskRunState::Running,
                        result_text: String::new(),
                        conversation_state: None,
                    },
                )]),
            )]),
            session_subagent_navigation_rounds: BTreeSet::from([previous_session_id]),
            active_subagent_panel: Some(SelectedSubagentTaskPanel {
                parent_session_id: previous_session_id,
                task_id: previous_task_id,
            }),
            detached_subagent_task_listeners: BTreeSet::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
        };
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![
                SessionSummary::new(
                    selected_session_id,
                    Some(String::from("project-a")),
                    "当前会话",
                    "",
                    SessionState::Completed,
                    0,
                    0,
                    "刚刚",
                ),
                SessionSummary::new(
                    previous_session_id,
                    Some(String::from("project-a")),
                    "旧会话",
                    "",
                    SessionState::Completed,
                    0,
                    0,
                    "刚刚",
                ),
            ],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: selected_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        assert_eq!(
            active_surface_session_id_from_state(&mut guard, &workspace_snapshot),
            selected_session_id
        );
        assert!(guard.active_subagent_panel.is_none());
        assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
    }

    /// Verifies WebUI stop projects the same stopped turn records as the iced transcript chain.
    /// 验证 WebUI 停止流程投影与 iced 转录链条一致的已停止轮次记录。
    #[test]
    fn webui_stop_session_work_appends_stopped_turn_projection_once() {
        let session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.last_safe_provider_request_count = Some(2);
        let user_entry =
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "运行一个长任务")
                .with_runtime_metadata("user-1", "1000");
        append_record(
            &mut runtime_state.conversation_state,
            SessionConversationRecord {
                id: user_entry.id.clone(),
                kind: user_entry.kind,
                record_kind: user_entry.record_kind,
                visible: user_entry.visible,
                title: user_entry.title.clone(),
                body: user_entry.body.clone(),
                metadata: user_entry.metadata.clone(),
                created_at: user_entry.created_at.clone(),
            },
        );

        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![
                SessionSummary::new(
                    session_id,
                    Some(String::from("project-a")),
                    "停止测试",
                    "正在运行",
                    SessionState::Running,
                    0,
                    0,
                    "刚刚",
                )
                .with_updated_at("1000"),
            ],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let did_stop =
            stop_webui_session_work(&mut workspace_snapshot, session_id, true, true).unwrap();

        assert!(did_stop);
        assert_eq!(
            workspace_snapshot.session_run_phases.get(&session_id),
            Some(&SessionRunPhase::Stopped)
        );
        let messages = workspace_snapshot
            .session_messages
            .get(&session_id)
            .expect("stopped projection should be materialized");
        assert!(messages.iter().any(|message| {
            message.visible
                && message.title == "执行已停止"
                && message.body.contains("当前 AI 调用")
        }));
        assert!(messages.iter().any(|message| {
            !message.visible && message.record_kind == SessionMessageRecordKind::TurnTerminated
        }));
        let turn_stats = messages
            .iter()
            .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
            .expect("stopped turn stats should be projected");
        assert!(turn_stats.visible);
        assert_eq!(turn_stats.title, "Turn Stats · stopped");
        assert!(turn_stats.body.contains("状态：已中止"));
        assert!(turn_stats.body.contains("Provider 调用：2"));
        let stopped_message_count = messages.len();

        let did_stop_again =
            stop_webui_session_work(&mut workspace_snapshot, session_id, false, false).unwrap();

        assert!(!did_stop_again);
        let messages_after_second_stop = workspace_snapshot
            .session_messages
            .get(&session_id)
            .expect("stopped projection should still exist");
        assert_eq!(messages_after_second_stop.len(), stopped_message_count);
    }

    /// Verifies a kernel-active turn still receives a stopped projection when its stream handle is gone.
    /// 验证内核活动回合即使流句柄已丢失也仍会收到停止投影。
    #[test]
    fn webui_stop_session_work_uses_kernel_active_state_without_stream_handle() {
        let session_id = SessionId::new_v4();
        let runtime_state = SessionRuntimeStateEntry {
            last_safe_provider_request_count: Some(1),
            ..SessionRuntimeStateEntry::default()
        };
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "停止测试",
                "正在运行",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let did_stop =
            stop_webui_session_work(&mut workspace_snapshot, session_id, true, false).unwrap();

        assert!(did_stop);
        assert_eq!(
            workspace_snapshot.session_run_phases.get(&session_id),
            Some(&SessionRunPhase::Stopped)
        );
        let messages = workspace_snapshot
            .session_messages
            .get(&session_id)
            .expect("kernel-active stop should be projected");
        assert!(messages.iter().any(|message| {
            message.visible
                && message.title == "执行已停止"
                && message.body.contains("当前 AI 调用")
        }));
    }

    /// Verifies persisted completed task mirrors repair stale running spawn-agent display cards.
    /// 验证持久化的已完成任务镜像会修复陈旧的运行中 spawn_agent 展示卡片。
    #[test]
    fn task_mirror_completed_status_repairs_stale_running_tool_card() {
        let task_id = SessionId::new_v4();
        let mut messages = vec![
            test_spawn_agent_tool_message(
                task_id,
                "running",
                SessionMessageRecordKind::ToolCallOutput,
            ),
            test_spawn_agent_tool_message(
                task_id,
                "running",
                SessionMessageRecordKind::ToolResultOutput,
            ),
        ];
        let mirrors = vec![SessionTaskMirrorSnapshot {
            task_session_id: task_id,
            status: String::from("completed"),
            line_count: 3,
            updated_at: String::from("1780000000000"),
        }];

        apply_task_mirror_statuses_to_parent_messages(
            messages.as_mut_slice(),
            None,
            mirrors.as_slice(),
            |_| false,
        );

        for message in messages {
            let metadata = message.metadata.as_ref().expect("metadata");
            assert_eq!(
                task_session_execution_status_from_tool_metadata(metadata),
                Some("completed")
            );
            assert_eq!(
                metadata
                    .get("tool_display_state")
                    .and_then(serde_json::Value::as_str),
                Some("completed")
            );
            assert_eq!(message.title, "调用 spawn_agent 工具 已完成");
        }
    }

    /// Verifies live runtime state overrides stale terminal task mirrors.
    /// 验证实时运行态会覆盖陈旧的终态任务镜像。
    #[test]
    fn task_mirror_display_keeps_running_when_task_is_live() {
        let task_id = SessionId::new_v4();
        let mut messages = vec![test_spawn_agent_tool_message(
            task_id,
            "completed",
            SessionMessageRecordKind::ToolCallOutput,
        )];
        let mirrors = vec![SessionTaskMirrorSnapshot {
            task_session_id: task_id,
            status: String::from("completed"),
            line_count: 3,
            updated_at: String::from("1780000000000"),
        }];

        apply_task_mirror_statuses_to_parent_messages(
            messages.as_mut_slice(),
            None,
            mirrors.as_slice(),
            |_| true,
        );

        let metadata = messages[0].metadata.as_ref().expect("metadata");
        assert_eq!(
            task_session_execution_status_from_tool_metadata(metadata),
            Some("running")
        );
        assert_eq!(
            metadata
                .get("tool_display_state")
                .and_then(serde_json::Value::as_str),
            Some("running")
        );
    }

    /// Builds one parent spawn-agent tool output message with task-session metadata.
    /// 构建一条带 task-session 元数据的父级 spawn_agent 工具输出消息。
    fn test_spawn_agent_tool_message(
        task_id: SessionId,
        execution_status: &str,
        record_kind: SessionMessageRecordKind,
    ) -> SessionMessageEntry {
        let mut message = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "调用 spawn_agent 工具 进行中",
            "spawn_agent returned",
        )
        .with_runtime_metadata(format!("tool-{task_id}"), String::from("1780000000000"));
        message.record_kind = record_kind;
        message.metadata = Some(json!({
            "tool_name": "spawn_agent",
            "display_state": execution_status,
            "tool_display_state": execution_status,
            "host_result": {
                "kind": "task_session",
                "payload": {
                    "task_id": "image-worker",
                    "task_session_id": task_id.to_string(),
                    "execution_status": execution_status
                }
            }
        }));
        message
    }
}
