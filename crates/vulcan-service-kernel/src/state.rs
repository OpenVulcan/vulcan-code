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
    /// Detached child-task listeners already started by the WebUI bridge.
    /// WebUI 桥接层已经启动的分离子任务监听器集合。
    pub(crate) detached_subagent_task_listeners: BTreeSet<SessionId>,
    /// Parent-turn bindings keyed by detached child task id.
    /// 按分离子任务标识索引的父回合绑定。
    pub(crate) subagent_parent_turn_bindings: BTreeMap<SessionId, WebuiSubagentParentTurnBinding>,
    /// Child tasks whose current run should not block the parent turn completion gate.
    /// 当前运行不应阻塞父回合完成闸门的子任务集合。
    pub(crate) subagent_completion_gate_ignored_tasks: BTreeSet<SessionId>,
    /// Parent sessions currently blocked inside a model-requested wait_agent call.
    /// 当前正阻塞在模型请求的 wait_agent 调用中的父会话集合。
    pub(crate) parent_sessions_waiting_for_subagents: BTreeSet<SessionId>,
    /// Active WebUI stream task handles keyed by session id.
    /// 按会话标识索引的 WebUI 活动流任务句柄。
    pub(crate) active_stream_tasks: BTreeMap<SessionId, ServiceTaskHandle<()>>,
    /// Session runtime kernels keyed by root WebUI session id.
    /// 按 WebUI 根会话标识索引的会话运行时内核。
    pub(crate) session_runtime_kernels: BTreeMap<SessionId, SessionRuntimeKernel>,
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

    /// Verifies fork requests reject historical message-bound rollback fields.
    /// 验证 fork 请求会拒绝历史消息绑定回滚字段。
    #[test]
    fn conversation_fork_request_rejects_before_message_id() {
        let error = serde_json::from_value::<ConversationForkRequest>(serde_json::json!({
            "conversationId": "conversation-a",
            "title": "Conversation A fork",
            "beforeMessageId": "message-a"
        }))
        .expect_err("fork request should reject message-bound fork fields");

        assert!(error.to_string().contains("unknown field"));
        assert!(error.to_string().contains("beforeMessageId"));
    }
}
