//! Core service state and shared task helpers.
//! 核心服务状态与共享任务辅助能力。

use super::*;

/// Shared runtime handles used to execute real conversation turns.
/// 用于执行真实会话轮次的共享运行时句柄集合。
#[derive(Clone)]
pub(crate) struct AgentRuntimeBridge {
    /// Provider configuration captured during service startup.
    /// 服务启动期间捕获的 Provider 配置。
    pub(crate) provider_configuration: ProviderConfigurationFile,
    /// Standalone AI capability configuration captured during service startup.
    /// 服务启动期间捕获的独立 AI 能力配置。
    pub(crate) ai_tool_configuration: PersistedAiToolConfiguration,
    /// Agent profile catalog reused by runtime preparation.
    /// 运行时准备阶段复用的 Agent 画像目录。
    pub(crate) agent_profile_catalog: AgentProfileCatalog,
    /// LuaSkills runtime handle reused by provider-visible tool execution.
    /// Provider 可见工具执行复用的 LuaSkills 运行时句柄。
    pub(crate) luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// MCP runtime access reused by provider-visible MCP tools.
    /// Provider 可见 MCP 工具复用的 MCP 运行时访问面。
    pub(crate) mcp_runtime: Option<McpRuntimeAccess>,
    /// Shared task runtime used by nested subagent tools.
    /// 嵌套子代理工具使用的共享任务运行时。
    pub(crate) task_runtime: Arc<TaskRuntime>,
    /// Shared ask-question runtime used by the ask tool.
    /// ask 工具使用的共享提问运行时。
    pub(crate) question_runtime: Arc<QuestionRuntime>,
    /// Shared command-session runtime used by shell session tools.
    /// shell 会话工具使用的共享命令会话运行时。
    pub(crate) command_session_runtime: Arc<CommandSessionRuntime>,
    /// Shared token-budget runtime used by context assembly.
    /// 上下文装配使用的共享 Token 预算运行时。
    pub(crate) token_budget_runtime: Arc<TokenBudgetRuntime>,
    /// Shared conversation hook runtime used to mirror WebUI turn assembly semantics.
    /// 用于复刻 WebUI 回合组装语义的共享会话 hook 运行时。
    pub(crate) conversation_hook_runtime: Arc<Mutex<ConversationHookRuntime>>,
    /// Shared VMM hook runtime snapshot used by built-in memory hooks.
    /// 内置记忆 hook 使用的共享 VMM hook 运行时快照。
    pub(crate) vmm_hook_runtime: VmmHookRuntimeHandle,
}

/// User prompt edit submit request sent by the WebUI transcript action.
/// WebUI 转录操作发送的用户提示词编辑提交请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserPromptEditSubmitRequest {
    /// Conversation id that owns the user prompt.
    /// 拥有该用户提示词的会话标识。
    pub(crate) conversation_id: String,
    /// Stable user prompt message id.
    /// 稳定用户提示词消息标识。
    pub(crate) message_id: String,
    /// Replacement prompt text.
    /// 替换后的提示词文本。
    pub(crate) prompt: String,
    /// Attachments replayed with the edited prompt.
    /// 随编辑后提示词重放的附件。
    #[serde(default)]
    pub(crate) attachments: Vec<ComposerAttachmentSummary>,
    /// Whether the backend may stop an active run before editing.
    /// 后端是否可以先停止当前运行再执行编辑。
    #[serde(default)]
    pub(crate) stop_running: bool,
}

/// Conversation fork request sent by the WebUI final-turn action.
/// WebUI 最终回合操作发送的会话分叉请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConversationForkRequest {
    /// Conversation id that should be copied into a new session.
    /// 需要复制为新会话的会话标识。
    pub(crate) conversation_id: String,
    /// Localized title assigned to the forked session.
    /// 分配给分叉会话的本地化标题。
    pub(crate) title: String,
    /// Optional retained message anchor for a message-scoped fork.
    /// 消息级分叉时可选保留的消息锚点。
    #[serde(default)]
    pub(crate) anchor_message_id: Option<String>,
}

/// Session-scoped AI capability configuration save request sent by the WebUI composer dialog.
/// WebUI 输入区控制项发送的会话计划模式更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionPlanModeUpdateRequest {
    /// Conversation id whose Plan mode state should change.
    /// 需要更新计划模式状态的会话标识。
    pub(crate) conversation_id: Option<String>,
    /// Whether Plan mode should be enabled for the selected session.
    /// 是否为当前会话启用计划模式。
    pub(crate) enabled: bool,
}

/// Session-scoped VMM memory policy update request sent by the WebUI composer menu.
/// WebUI 输入区菜单发送的会话级 VMM 记忆策略更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionVmmPolicyUpdateRequest {
    /// Conversation id whose VMM memory policy should change.
    /// 需要更新 VMM 记忆策略的会话标识。
    pub(crate) conversation_id: Option<String>,
    /// Policy key to update: precheck or writeback.
    /// 需要更新的策略键：前置注入或后置提交。
    pub(crate) policy: String,
    /// New tri-state policy mode: inherit, enabled, or disabled.
    /// 新的三态策略模式：跟随、开启或关闭。
    pub(crate) mode: String,
}

/// Empty session lifecycle hook used by the WebUI external stream lifecycle runner.
/// WebUI 外部流生命周期 runner 使用的空会话生命周期 hook。
#[derive(Debug, Clone, Copy)]
pub(crate) struct WebuiSessionLifecycleHook;

impl SessionLifecycleHook for WebuiSessionLifecycleHook {
    /// Ignores session lifecycle events after durable append.
    /// 在持久化追加后忽略 会话生命周期事件。
    fn after_event_appended(&self, _event: &SessionBoundaryEvent) -> Result<(), String> {
        Ok(())
    }
}

/// Number of recent turns loaded for the selected conversation.
/// 为当前选中会话加载的最近轮次数量。
pub(crate) const SELECTED_SESSION_TURN_LOAD_LIMIT: usize = 30;

/// Maximum loaded subagent transcript surfaces retained by the Tauri service.
/// Tauri 服务保留的已加载子代理转录界面最大数量。
pub(crate) const SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT: usize = 128;

/// Milliseconds before an unused loaded subagent transcript surface can be evicted.
/// 已加载但未使用的子代理转录界面允许被清理前的毫秒数。
pub(crate) const SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS: i128 = 30 * 60 * 1000;

/// Parent-turn binding captured when a detached WebUI subagent starts.
/// WebUI 分离子代理启动时捕获的父回合绑定。
#[derive(Debug, Clone)]
pub(crate) struct WebuiSubagentParentTurnBinding {
    /// Parent session that spawned the detached child task.
    /// 派生分离子任务的父会话。
    pub(crate) parent_session_id: SessionId,
    /// Parent kernel generation that was current at child creation time.
    /// 子任务创建时父级内核的当前代际。
    pub(crate) source_generation: u64,
    /// Parent turn id that was active at child creation time.
    /// 子任务创建时处于活动状态的父级回合标识。
    pub(crate) parent_turn_id: Option<String>,
}

/// Exact child execution identity used by parent-turn bindings and terminal delivery.
/// 父回合绑定与终态投递使用的精确子执行身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SubagentExecutionKey {
    /// Stable child task session reused across executions.
    /// 跨执行复用的稳定子任务会话。
    pub(crate) task_id: SessionId,
    /// Unique concrete execution attempt.
    /// 唯一具体执行尝试。
    pub(crate) execution_id: String,
}

impl SubagentExecutionKey {
    /// Builds one owned key from a stable task and concrete execution identity.
    /// 从稳定任务与具体执行身份构建一个有所有权的键。
    pub(crate) fn new(task_id: SessionId, execution_id: &str) -> Self {
        Self {
            task_id,
            execution_id: execution_id.to_string(),
        }
    }
}

/// Long-lived in-memory detail cache for one loaded WebUI subagent surface.
/// 单个已加载 WebUI 子代理界面的长生命周期内存详情缓存。
#[derive(Debug, Clone)]
pub(crate) struct WebuiSubagentSurfaceDetailCacheEntry {
    /// Cached task panel state including durable child conversation detail when available.
    /// 缓存的任务面板状态，包含可用的持久子会话详情。
    pub(crate) task_state: SubagentTaskPanelState,
    /// Cached projected child transcript messages for direct WebUI surface rendering.
    /// 用于 WebUI 界面直接渲染的子转录消息缓存。
    pub(crate) messages: Vec<SessionMessageEntry>,
    /// Last access timestamp in Unix milliseconds.
    /// 最近访问时间戳，单位为 Unix 毫秒。
    pub(crate) last_access_ms: i128,
}

/// Authoritative reason why one session temporarily rejects new kernel turn admission.
/// 单个会话暂时拒绝新 kernel 回合准入的权威原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionTurnAdmissionReservationKind {
    /// One existing-session user submission owns turn registration until its kernel commit.
    /// 单个已有会话用户提交在 kernel 提交前拥有回合登记权。
    PromptSubmission,
    /// The session is crossing an archive or removal lifecycle transaction.
    /// 会话正在跨越归档或删除生命周期事务。
    LifecycleTransition,
    /// The session Goal authority is being changed and stale dispatch must not register.
    /// 会话 Goal 权威正在变更，过期分发不得注册。
    GoalMutation,
}

impl SessionTurnAdmissionReservationKind {
    /// Returns the stable diagnostic label for this reservation reason.
    /// 返回此预留原因的稳定诊断标签。
    pub(crate) const fn diagnostic_label(self) -> &'static str {
        match self {
            Self::PromptSubmission => "user prompt submission",
            Self::LifecycleTransition => "archive or removal lifecycle transition",
            Self::GoalMutation => "Goal authority mutation",
        }
    }
}

/// Owned reservation that blocks new kernel turns for one exact session.
/// 阻止单个精确会话启动新 kernel 回合的有所有权预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionTurnAdmissionReservation {
    /// Unique owner identity used to prevent a stale guard from deleting a replacement reservation.
    /// 用于防止过期守卫删除替代预留的唯一所有者身份。
    pub(crate) reservation_id: Uuid,
    /// Domain reason that owns the reservation.
    /// 持有此预留的领域原因。
    pub(crate) kind: SessionTurnAdmissionReservationKind,
}

/// Project-level scope serialized against lifecycle transitions and prompt submissions.
/// 与生命周期转换和提示词提交串行化的项目级作用域。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProjectAdmissionScope {
    /// One exact active project identity.
    /// 单个精确活动项目身份。
    Exact(String),
    /// A submission whose authoritative project cannot be selected before workspace loading.
    /// 在工作区加载前无法选择权威项目的提交。
    UnresolvedSubmission,
}

/// Authoritative reason why one project scope temporarily rejects competing mutations.
/// 单个项目作用域暂时拒绝竞争变更的权威原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectAdmissionReservationKind {
    /// A prompt submission owns project membership until its workspace commit.
    /// 提示词提交在工作区提交前拥有项目成员关系。
    PromptSubmission,
    /// The project is crossing an archive or removal lifecycle transaction.
    /// 项目正在跨越归档或移除生命周期事务。
    LifecycleTransition,
}

impl ProjectAdmissionReservationKind {
    /// Returns the stable diagnostic label for this project reservation reason.
    /// 返回此项目预留原因的稳定诊断标签。
    pub(crate) const fn diagnostic_label(self) -> &'static str {
        match self {
            Self::PromptSubmission => "user prompt submission",
            Self::LifecycleTransition => "archive or removal lifecycle transition",
        }
    }
}

/// Owned reservation that blocks conflicting work for one project admission scope.
/// 阻止单个项目准入作用域冲突工作的有所有权预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectAdmissionReservation {
    /// Unique owner identity used to protect successor reservations from stale cleanup.
    /// 用于保护后继预留不被过期清理删除的唯一所有者身份。
    pub(crate) reservation_id: Uuid,
    /// Domain reason that owns the project scope.
    /// 持有项目作用域的领域原因。
    pub(crate) kind: ProjectAdmissionReservationKind,
}

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
pub struct ServiceBridgeState {
    /// Host shell capabilities supplied by the concrete application shell.
    /// 由具体应用壳提供的宿主 shell 能力。
    pub(crate) shell_host: Arc<dyn ServiceShellHost>,
    /// Latest startup snapshot.
    /// 最近一次启动快照。
    pub(crate) startup: StartupSnapshot,
    /// Latest workbench snapshot.
    /// 最近一次工作台快照。
    pub(crate) workbench: WorkbenchSnapshot,
    /// Monotonic version for workspace-driven workbench rebuilds.
    /// 工作区驱动的工作台重建单调版本号。
    pub(crate) workbench_generation: u64,
    /// Latest workspace snapshot retained for streaming turn updates.
    /// 为流式会话轮次更新保留的最近工作区快照。
    pub(crate) workspace_snapshot: Option<WorkspaceSnapshot>,
    /// Retained lazy conversation body windows keyed by session id.
    /// 按会话标识索引的懒加载会话正文保留窗口。
    pub(crate) hydrated_conversation_windows: BTreeMap<SessionId, HydratedConversationWindowState>,
    /// Long-lived shared agent runtime bridge.
    /// 长生命周期共享 Agent 运行时桥接。
    pub(crate) agent_runtime: Option<AgentRuntimeBridge>,
    /// Tokio runtime that owns bootstrap-spawned background tasks such as shared MCP access.
    /// 持有引导期后台任务的 Tokio 运行时，例如共享 MCP 访问任务。
    pub(crate) service_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Browser service discovery snapshot shared by `/health` and the dev discovery file.
    /// `/health` 与开发发现文件共享的浏览器服务发现快照。
    pub(crate) webui_service_discovery: Option<SharedWebuiServiceDiscoverySnapshot>,
    /// Long-lived controller runtime handle.
    /// 长生命周期控制器运行时句柄。
    pub(crate) controller_runtime: Option<ControllerRuntimeHandle>,
    /// Long-lived LuaSkills runtime handle.
    /// 长生命周期 LuaSkills 运行时句柄。
    pub(crate) luaskills_runtime: Option<TauriLuaSkillsHandle>,
    /// Cached LuaSkills catalog captured during startup or explicit refresh.
    /// 启动或显式刷新时捕获的 LuaSkills 目录缓存。
    pub(crate) cached_luaskills_catalog: Option<TauriLuaSkillsCatalog>,
    /// Long-lived token budget runtime handle.
    /// 长生命周期 Token 预算运行时句柄。
    pub(crate) token_budget_runtime: Option<Arc<TauriTokenBudgetRuntime>>,
    /// Long-lived MCP runtime handle.
    /// 长生命周期 MCP 运行时句柄。
    pub(crate) mcp_runtime: Option<TauriMcpRuntimeHandle>,
    /// Cached MCP catalog captured during startup or explicit refresh.
    /// 启动或显式刷新时捕获的 MCP 目录缓存。
    pub(crate) cached_mcp_catalog: Option<TauriMcpRuntimeCatalog>,
    /// Latest context-budget snapshots keyed by session id.
    /// 按会话标识索引的最近上下文预算快照。
    pub(crate) live_context_budgets: BTreeMap<SessionId, ContextBudgetSnapshot>,
    /// Latest goal summaries keyed by session id for synchronous workbench projection.
    /// 按会话标识索引的最近目标摘要，用于同步构建工作台投影。
    pub(crate) session_goal_summaries: BTreeMap<SessionId, SessionGoalSummary>,
    /// Monotonic Goal dispatch-authority version keyed by session id.
    /// 按会话标识索引的 Goal 分发权威单调版本号。
    pub(crate) session_goal_dispatch_versions: BTreeMap<SessionId, u64>,
    /// Goal ids that already received one budget-limit wrap-up guidance in this process.
    /// 本进程内已经收到预算耗尽收尾引导的目标标识集合。
    pub(crate) budget_limit_reported_goal_ids: BTreeSet<String>,
    /// Root sessions whose unclosed subagent metadata has already been loaded from disk.
    /// 已经从磁盘加载过未关闭子智能体元数据的根会话集合。
    pub(crate) webui_preloaded_unclosed_subagent_roots: BTreeSet<SessionId>,
    /// Long-lived loaded child transcript details keyed by child task id.
    /// 按子任务标识索引的长生命周期已加载子转录详情。
    pub(crate) subagent_surface_detail_cache:
        BTreeMap<SessionId, WebuiSubagentSurfaceDetailCacheEntry>,
    /// Parent sessions whose current coordination round should keep subagent navigation visible.
    /// 当前协调轮次中应保持子代理导航可见的父会话集合。
    pub(crate) session_subagent_navigation_rounds: BTreeSet<SessionId>,
    /// Currently opened subagent surface selection.
    /// 当前打开的子代理界面选择。
    pub(crate) active_subagent_panel: Option<SelectedSubagentTaskPanel>,
    /// Current detached child execution owned by each WebUI listener task id.
    /// 每个 WebUI 监听任务标识当前拥有的分离子执行。
    pub(crate) detached_subagent_task_listeners: BTreeMap<SessionId, String>,
    /// Parent-turn bindings keyed by exact detached child execution.
    /// 按精确分离子执行索引的父回合绑定。
    pub(crate) subagent_parent_turn_bindings:
        BTreeMap<SubagentExecutionKey, WebuiSubagentParentTurnBinding>,
    /// Exact child executions that should not block the parent turn completion gate.
    /// 不应阻塞父回合完成闸门的精确子执行集合。
    pub(crate) subagent_completion_gate_ignored_tasks: BTreeSet<SubagentExecutionKey>,
    /// Parent sessions currently blocked inside a model-requested wait_agent call.
    /// 当前正阻塞在模型请求的 wait_agent 调用中的父会话集合。
    pub(crate) parent_sessions_waiting_for_subagents: BTreeSet<SessionId>,
    /// Active WebUI stream task handles keyed by session id.
    /// 按会话标识索引的 WebUI 活动流任务句柄。
    pub(crate) active_stream_tasks: BTreeMap<SessionId, ActiveStreamTask>,
    /// Session runtime kernels keyed by root WebUI session id.
    /// 按 WebUI 根会话标识索引的会话运行时内核。
    pub(crate) session_runtime_kernels: BTreeMap<SessionId, SessionRuntimeKernel>,
    /// Owned turn-admission reservations keyed by exact session id.
    /// 按精确会话标识索引的有所有权回合准入预留。
    pub(crate) session_turn_admission_reservations:
        BTreeMap<SessionId, SessionTurnAdmissionReservation>,
    /// Owned project-admission reservations keyed by exact or unresolved scope.
    /// 按精确或未解析作用域索引的有所有权项目准入预留。
    pub(crate) project_admission_reservations:
        BTreeMap<ProjectAdmissionScope, ProjectAdmissionReservation>,
    /// Latest background persistence version keyed by session id.
    /// 按会话标识索引的最新后台持久化版本。
    pub(crate) session_persist_versions: BTreeMap<SessionId, u64>,
    /// Latest live transcript persistence version keyed by session id.
    /// 按会话标识索引的最新运行中转录持久化版本。
    pub(crate) session_transcript_persist_versions: BTreeMap<SessionId, u64>,
    /// Latest addon persistence version keyed by session id and addon namespace.
    /// 按会话标识与附属状态命名空间索引的最新附属状态持久化版本。
    pub(crate) session_addon_persist_versions: BTreeMap<(SessionId, SessionAddonNamespace), u64>,
}

/// Thread-safe service state handle.
/// 线程安全的服务状态句柄。
pub type SharedServiceBridgeState = Arc<Mutex<ServiceBridgeState>>;

/// Locks shared service state for exact admission-reservation cleanup, recovering poisoned ownership state when necessary.
/// 为精确准入预留清理锁定共享服务状态，并在必要时恢复中毒的所有权状态。
///
/// # Parameters
/// # 参数
/// `state` owns the authoritative typed admission reservation tables.
/// `state` 持有权威的类型化准入预留表。
/// `owner_label` identifies the exact Guard kind performing Drop cleanup.
/// `owner_label` 标识执行 Drop 清理的精确 Guard 类型。
///
/// # Returns
/// # 返回值
/// A service-state guard that permits only the caller's exact, id-qualified cleanup operation.
/// 返回仅供调用方执行精确且带所有者标识清理操作的服务状态守卫。
pub(crate) fn lock_service_state_for_admission_cleanup<'state>(
    state: &'state SharedServiceBridgeState,
    owner_label: &str,
) -> std::sync::MutexGuard<'state, ServiceBridgeState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!(
                "[webui-service] recovering poisoned service state for {owner_label} admission reservation cleanup"
            );
            poisoned.into_inner()
        }
    }
}

/// Host-provided shell capabilities required by the service kernel.
/// 服务内核所需的宿主壳能力。
pub trait ServiceShellHost: Send + Sync {
    /// Opens one local path with the host file manager.
    /// 使用宿主文件管理器打开单个本地路径。
    fn open_path(&self, path: &str) -> Result<(), String>;

    /// Opens one URL with the host default browser.
    /// 使用宿主默认浏览器打开单个 URL。
    fn open_url(&self, url: &str) -> Result<(), String>;
}

/// Join handle type used by service-owned asynchronous tasks.
/// 服务自有异步任务使用的 join 句柄类型。
pub(crate) type ServiceTaskHandle<T> = tokio::task::JoinHandle<T>;

/// Active provider or compaction task retained with its request-scoped Goal binding.
/// 携带请求级 Goal 绑定保留的活动 Provider 或压缩任务。
#[derive(Debug)]
pub(crate) struct ActiveStreamTask {
    /// Unique identity used to prevent stale cleanup from removing a replacement task.
    /// 用于防止过期清理删除替换任务的唯一身份。
    task_id: Uuid,
    /// Abortable asynchronous task handle.
    /// 可中止的异步任务句柄。
    handle: ServiceTaskHandle<()>,
    /// Goal identifier captured when the provider execution started.
    /// Provider 执行启动时捕获的目标标识。
    session_goal_id: Option<String>,
}

impl ActiveStreamTask {
    /// Creates one retained active-stream task entry.
    /// 创建一个保留的活动流任务条目。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the unique identity shared by the retained entry and its cleanup path.
    /// `task_id` 是保留条目与其清理路径共享的唯一身份。
    /// `handle` is the abortable task spawned for the session.
    /// `handle` 是为会话启动的可中止任务。
    /// `session_goal_id` binds normal provider work to its launch-time Goal; compaction uses `None`.
    /// `session_goal_id` 将普通 Provider 工作绑定到启动期目标；压缩任务使用 `None`。
    ///
    /// # Returns
    /// 返回值
    /// A retained task entry with inseparable ownership metadata.
    /// 返回带有不可分割所有权元数据的保留任务条目。
    pub(crate) fn new(
        task_id: Uuid,
        handle: ServiceTaskHandle<()>,
        session_goal_id: Option<String>,
    ) -> Self {
        Self {
            task_id,
            handle,
            session_goal_id,
        }
    }

    /// Returns the launch-time Goal identifier bound to this task.
    /// 返回绑定到此任务的启动期目标标识。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed Goal identifier, or `None` for work unrelated to a Goal.
    /// 返回借用的目标标识；与 Goal 无关的工作返回 `None`。
    pub(crate) fn session_goal_id(&self) -> Option<&str> {
        self.session_goal_id.as_deref()
    }

    /// Aborts the retained asynchronous task.
    /// 中止保留的异步任务。
    ///
    /// # Returns
    /// 返回值
    /// Returns nothing after forwarding cancellation to the Tokio task handle.
    /// 将取消请求转发给 Tokio 任务句柄后无返回值。
    pub(crate) fn abort(&self) {
        self.handle.abort();
    }
}

/// Returns whether an active stream task still owns the retained session entry.
/// 返回活动流任务是否仍拥有保留的会话条目。
///
/// # Parameters
/// 参数
/// `state` owns the active stream task map.
/// `state` 持有活动流任务映射。
/// `session_id` identifies the session slot being inspected.
/// `session_id` 标识正在检查的会话槽位。
/// `task_id` is the unique identity captured by the active task.
/// `task_id` 是活动任务捕获的唯一身份。
///
/// # Returns
/// 返回值
/// Returns `true` only when the retained entry still belongs to the exact task id.
/// 仅当保留条目仍属于精确 task id 时返回 `true`。
pub(crate) fn active_stream_task_matches_id(
    state: &ServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
) -> bool {
    state
        .active_stream_tasks
        .get(&session_id)
        .is_some_and(|entry| entry.task_id == task_id)
}

/// Removes an active stream task only when the caller still owns the retained entry.
/// 仅当调用方仍拥有保留条目时移除活动流任务。
///
/// # Parameters
/// 参数
/// `state` owns the active stream task map.
/// `state` 持有活动流任务映射。
/// `session_id` identifies the session slot being cleaned up.
/// `session_id` 标识正在清理的会话槽位。
/// `task_id` is the unique identity captured by the finishing task.
/// `task_id` 是结束任务捕获的唯一身份。
///
/// # Returns
/// 返回值
/// The removed entry when identities match, otherwise `None` without changing the replacement.
/// 身份匹配时返回已移除条目，否则返回 `None` 且不修改替换任务。
pub(crate) fn remove_active_stream_task_if_id(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
) -> Option<ActiveStreamTask> {
    // owns_entry reuses the exact non-mutating ownership predicate shared by terminal transactions.
    // owns_entry 复用终态事务共享的精确只读所有权判定。
    let owns_entry = active_stream_task_matches_id(state, session_id, task_id);
    owns_entry
        .then(|| state.active_stream_tasks.remove(&session_id))
        .flatten()
}

/// Failure returned when an active stream task cannot complete gated registration.
/// 活动流任务无法完成闸门注册时返回的失败。
#[derive(Debug)]
pub(crate) enum ActiveStreamTaskRegistrationError {
    /// The authoritative registry could not be locked, so no new entry was installed.
    /// 无法锁定权威注册表，因此未安装新条目。
    RegistryUnavailable {
        /// Exact mutex failure text retained for diagnostics.
        /// 为诊断保留的精确互斥锁失败文本。
        detail: String,
    },
    /// The spawned task exited before its start gate opened.
    /// 已启动任务在启动闸门打开前退出。
    StartGateClosed {
        /// Whether cleanup removed the exact entry installed by this registration attempt.
        /// 清理是否移除了本次注册尝试安装的精确条目。
        owned_entry_removed: bool,
    },
}

impl ActiveStreamTaskRegistrationError {
    /// Creates a start-gate failure carrying the exact registry-removal ownership fact.
    /// 创建携带精确注册表移除所有权事实的启动闸门失败。
    ///
    /// # Parameters
    /// # 参数
    /// `owned_entry_removed` reports whether cleanup removed this attempt's exact task entry.
    /// `owned_entry_removed` 表示清理是否移除了本次尝试的精确任务条目。
    ///
    /// # Returns
    /// # 返回值
    /// A typed start-gate registration error preserving the supplied ownership evidence.
    /// 返回保留所提供所有权证据的强类型启动闸门注册错误。
    pub(crate) fn start_gate_closed(owned_entry_removed: bool) -> Self {
        Self::StartGateClosed {
            owned_entry_removed,
        }
    }

    /// Returns whether this failed attempt proved ownership by removing its exact registered entry.
    /// 返回本次失败尝试是否通过移除精确注册条目证明了所有权。
    ///
    /// # Parameters
    /// # 参数
    /// This method has no parameters beyond the registration error.
    /// 此方法除注册错误外没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// `true` only when start-gate cleanup removed the exact task identity.
    /// 仅当启动闸门清理移除了精确任务身份时返回 `true`。
    pub(crate) fn owned_entry_removed(&self) -> bool {
        matches!(
            self,
            Self::StartGateClosed {
                owned_entry_removed: true
            }
        )
    }
}

impl std::fmt::Display for ActiveStreamTaskRegistrationError {
    /// Formats the stable registration failure diagnostic.
    /// 格式化稳定的注册失败诊断。
    ///
    /// # Parameters
    /// # 参数
    /// `formatter` receives the stable English diagnostic text for the registration failure.
    /// `formatter` 接收注册失败的稳定英文诊断文本。
    ///
    /// # Returns
    /// # 返回值
    /// The formatter result after writing the complete diagnostic.
    /// 返回写入完整诊断后的格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegistryUnavailable { detail } => {
                write!(formatter, "failed to lock service state: {detail}")
            }
            Self::StartGateClosed {
                owned_entry_removed,
            } => write!(
                formatter,
                "registered stream task exited before its start gate opened; owned_entry_removed={owned_entry_removed}"
            ),
        }
    }
}

impl std::error::Error for ActiveStreamTaskRegistrationError {}

/// Spawns one stream task behind a gate that opens only after durable in-memory registration.
/// 在内存注册完成后才打开的闸门后启动一个流任务。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative active stream registry.
/// `state` 持有权威活动流注册表。
/// `session_id` selects the session slot to replace.
/// `session_id` 选择要替换的会话槽位。
/// `task_id` uniquely identifies the new task and all of its cleanup paths.
/// `task_id` 唯一标识新任务及其所有清理路径。
/// `session_goal_id` binds ordinary provider work to its launch-time Goal.
/// `session_goal_id` 将普通 Provider 工作绑定到启动期目标。
/// `future` is the task body that must not execute before registration succeeds.
/// `future` 是注册成功前不得执行的任务主体。
///
/// # Returns
/// 返回值
/// Success after replacing and cancelling any previous task, or an explicit registration/start error.
/// 替换并中止旧任务后返回成功，或返回显式注册/启动错误。
pub(crate) fn spawn_registered_active_stream_task<F>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    task_id: Uuid,
    session_goal_id: Option<String>,
    future: F,
) -> Result<(), ActiveStreamTaskRegistrationError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let (start_sender, start_receiver) = tokio::sync::oneshot::channel::<()>();
    let handle = spawn_service_task(async move {
        if start_receiver.await.is_ok() {
            future.await;
        }
    });
    let previous_task = match state.lock() {
        Ok(mut guard) => guard.active_stream_tasks.insert(
            session_id,
            ActiveStreamTask::new(task_id, handle, session_goal_id),
        ),
        Err(error) => {
            handle.abort();
            return Err(ActiveStreamTaskRegistrationError::RegistryUnavailable {
                detail: error.to_string(),
            });
        }
    };
    if let Some(previous_task) = previous_task {
        previous_task.abort();
    }
    if start_sender.send(()).is_err() {
        // owned_entry_removed preserves the exact ownership fact needed by failure terminalization.
        // owned_entry_removed 保留失败终结所需的精确所有权事实。
        let owned_entry_removed = state.lock().is_ok_and(|mut guard| {
            remove_active_stream_task_if_id(&mut guard, session_id, task_id).is_some()
        });
        return Err(ActiveStreamTaskRegistrationError::start_gate_closed(
            owned_entry_removed,
        ));
    }
    Ok(())
}

/// Spawns one asynchronous service task through the active Tokio runtime.
/// 通过当前 Tokio 运行时启动单个异步服务任务。
///
/// # Parameters
/// 参数
/// `future` is the asynchronous operation owned by the local service layer.
/// `future` 是本地服务层拥有的异步操作。
///
/// # Returns
/// 返回值
/// A join handle that can be retained or aborted by service state.
/// 返回可由服务状态保留或中止的 join 句柄。
pub(crate) fn spawn_service_task<F>(future: F) -> ServiceTaskHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::task::spawn(future)
}

/// Spawns one blocking service operation through the active Tokio runtime.
/// 通过当前 Tokio 运行时启动单个阻塞式服务操作。
///
/// # Parameters
/// 参数
/// `operation` is the blocking closure that must not run on an async worker.
/// `operation` 是不能在异步 worker 上直接运行的阻塞闭包。
///
/// # Returns
/// 返回值
/// A join handle resolving to the blocking operation result.
/// 返回解析为阻塞操作结果的 join 句柄。
pub(crate) fn spawn_service_blocking_task<F, R>(operation: F) -> ServiceTaskHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
}

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
pub(crate) fn webui_workspace_persist_lock() -> Arc<tokio::sync::Mutex<()>> {
    WEBUI_WORKSPACE_PERSIST_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies fork requests accept the explicit retained message anchor.
    /// 验证 fork 请求接受显式保留消息锚点。
    #[test]
    fn conversation_fork_request_accepts_anchor_message_id() {
        let request = serde_json::from_value::<ConversationForkRequest>(serde_json::json!({
            "conversationId": "conversation-a",
            "title": "Conversation A fork",
            "anchorMessageId": "message-a"
        }))
        .expect("fork request should accept explicit anchor");

        assert_eq!(request.anchor_message_id.as_deref(), Some("message-a"));
    }
}
