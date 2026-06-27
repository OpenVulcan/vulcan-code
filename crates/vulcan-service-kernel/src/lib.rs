//! Core service kernel used by Vulcan Code WebUI hosts.
//! Vulcan Code WebUI 宿主使用的核心服务内核。

use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    env,
    future::Future,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use vldb_controller_client::ControllerSqliteValue;
#[cfg(test)]
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport, ExecutionStreamEvent,
    RuntimePromptIntervention,
    agent_profile::AgentProfileCatalog,
    agent_runtime::{
        context_injection_runtime::{
            context_injection_panel_cards_for_kind,
            remove_legacy_vmm_profile_context_injection_state,
        },
        conversation_hooks::{ConversationHookRuntime, ConversationTurnCompletionStatus},
        events::{RuntimeEvent, RuntimeEventKind},
        prompt_composer_runtime::{
            invalidate_runtime_prompt_luaskill_catalog_cache,
            invalidate_runtime_prompt_tool_visibility_cache,
        },
        question_runtime::{PendingQuestionChangeCallback, QuestionRuntime},
        task_runtime::{DetachedTaskRuntimeEvent, TaskRuntime},
        token_budget_runtime::TokenBudgetRuntime,
        vmm_hooks::VmmHookRuntimeHandle,
    },
    conversation_protocol::{
        conversation_state_from_session_messages, conversation_state_to_session_messages,
        merge_session_messages_with_conversation_state_records,
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{McpRuntimeAccess, bootstrap_mcp_runtime_access},
    provider_capabilities::{
        ResolvedModelCapabilities, resolve_effective_model_capabilities, resolve_model_capabilities,
    },
    provider_config::{
        self, PersistedAiToolConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
    },
    provider_input_normalizer::normalize_provider_user_input_for_model,
    session_event_runtime::{
        SessionStreamingRecordIds, append_runtime_event_projection_to_state,
        append_stream_delta_record_to_state, apply_checkpoint_conversation_state,
        apply_terminal_execution_report_to_state, collapse_streaming_thinking_in_state,
    },
    stream_runtime_run_with_intervention,
    tooling::{CommandSessionRuntime, command_session_runtime::CommandSessionInputEvent},
    turn_stats::build_session_turn_stats_from_state_with_segments,
    workbench_ai_capability_router::{
        WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS, effective_ai_tool_configuration_for_prompt_overrides,
        effective_workbench_disabled_ai_capability_kinds,
    },
};
use vulcan_context::{
    ContextBudgetSeverity, ContextBudgetSnapshot, SessionConversationRecord, append_record,
};
use vulcan_context_injection::{
    ContextInjectionState, InjectionCompactionPolicy, InjectionInsert, InjectionInsertItem,
    InjectionKind, InjectionPanelCard,
};
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::types::{ConversationAttachment, ProviderRequestSnapshot, ProviderUsageSnapshot};
use vulcan_provider_config::config::{
    AiToolKind, PersistedLuaSkillToolRegistrationState, PersistedMcpOAuthConfig,
    PersistedMcpServerEntry, PersistedMcpTransportKind, PersistedPromptSkillState,
    ProviderFieldKey,
};
use vulcan_service_api::{
    AgentSkillLoadIssueSummary, AgentSkillSummary, AgentSkillsCatalogSummary,
    AppendMessageBodyDelta, CommandSessionSummary, ComposerAttachmentSummary,
    ContextInjectionCardSummary, ContextInjectionItemSummary, ContextInjectionSnapshot,
    ContextUsageSnapshot, ConversationForkSourceSummary, ConversationMessage, ConversationSummary,
    FeatureAreaSummary, HydratedConversationWindowSummary, KeyValueSummary, LogEntrySummary,
    LuaSkillSummary, LuaSkillToolSummary, LuaSkillsCatalogSummary, McpServerSummary, MemorySummary,
    MessageRole, PendingQuestionOptionSummary, PendingQuestionPromptSummary,
    PendingQuestionSnapshot, PluginSummary, ProjectSummary, ProviderFieldSummary,
    ProviderModelSummary, ProviderSummary, ReasoningSelectionOptionSummary,
    ReasoningSelectionSummary, RuntimeModuleSnapshot, ServiceEvent, ServiceEventEmitter,
    SessionAddonChangedEvent, SessionGoalSummary, SessionPlanArtifactSummary,
    SessionPlanModeSummary, SessionTodolistItemSummary, SessionTodolistSummary,
    SettingSectionSummary, StartupSnapshot, StartupStepStatus, SubagentNavigationSnapshot,
    ToolCallSummary, UserPromptEditDraftSummary, VmmMemorySnapshot,
    WEBUI_SERVICE_DISCOVERY_STATUS_FAILED, WEBUI_SERVICE_DISCOVERY_STATUS_READY,
    WEBUI_SERVICE_PORT_ENV, WEBUI_STATIC_DIR_ENV, WorkbenchDelta, WorkbenchDeltaEnvelope,
    WorkbenchSnapshot, webui_service_discovery_snapshot_with_status,
    webui_service_starting_discovery_snapshot, webui_static_root_from_candidates,
};
use vulcan_service_host::{
    SharedWebuiServiceDiscoverySnapshot, WebuiServiceHttpConfig, WebuiServiceRpcDispatcher,
    bind_webui_service_http_listener, serve_webui_service_http_listener,
    write_webui_service_discovery_snapshot,
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
    PendingSessionPromptEntry, PendingSessionPromptSource, SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
    SelectedSubagentTaskPanel, SessionAddonNamespace, SessionAddonState, SessionId,
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionPlanArtifactState,
    SessionPlanModeState, SessionRunPhase, SessionRuntimeControls, SessionRuntimeStateEntry,
    SessionState, SessionSummary, SessionTitleSource, SessionTodolistItem,
    SessionTodolistItemStatus, SessionTodolistState, SessionTurnStatsKind, SessionTurnStatsStatus,
    SessionTurnUsageSegment, SessionWorkMode, SubagentTaskPanelState, SubagentTaskRunState,
    TaskProjectSummary, attach_tool_display_state_to_metadata, build_session_record_message_id,
    build_stopped_task_session_metadata, collect_running_subagent_task_states_for_tree,
    find_task_session_tool_message_id_for_payload, merge_subagent_task_panel_cache,
    merge_tool_display_metadata, normalize_subagent_task_panel_state,
    parse_subagent_task_panel_metadata, parse_subagent_task_prompt_metadata,
    select_subagent_navigation_panel_states, subagent_navigation_round_should_remain_visible,
    subagent_navigation_surface_is_visible, subagent_navigation_task_label,
    subagent_task_panel_is_openable,
    subagent_task_panel_state_detail_from_segmented_task_session_store,
    subagent_task_panel_state_has_recoverable_conversation,
    task_session_execution_status_from_tool_metadata, task_session_message_match_score,
    task_session_payload_from_tool_metadata,
};
use vulcan_workspace_store::{
    data_plane::{APP_SPACE_ID, ControllerRuntimeHandle},
    workspace_store::{
        GoalUpdate, SessionTaskMirrorSnapshot, ThreadGoal, ThreadGoalStatus, VulcanGoalStore,
        WorkspaceSegmentedSessionBoundaryStore, WorkspaceSnapshot, fork_persisted_session,
        load_persisted_user_prompt_edit_draft, load_segmented_session_fork_source_for_project,
        record_persisted_session_fork_source_marker, rollback_persisted_session_before_user_prompt,
    },
};

mod ai_capabilities;
mod attachments;
mod context_compaction;
mod conversation_history;
mod conversation_submission;
mod execution_report_memory;
mod git;
mod pending_prompt_services;
mod pending_prompts;
mod project_directories;
mod provider_settings;
mod service_bootstrap;
mod session_goal_lifecycle;
mod session_hydration;
mod vmm;
mod vmm_hooks;
mod webui_prompt_lifecycle;
mod webui_turn_chaining;
mod workbench_model_routes;
mod workbench_surface_projection;
mod workspace_files;
use ai_capabilities::{
    ai_capabilities, ai_capability_has_native_path, ai_capability_vendor_catalog,
    ai_capability_vendors, parse_ai_capability_target_order_overrides,
    parse_ai_capability_target_selectors, service_remove_ai_capability_vendor,
    service_save_ai_capability_targets, service_save_ai_capability_vendor,
    service_set_ai_capability_target_enabled, set_ai_capability_target_order_for_kind,
};
use attachments::{
    composer_attachment_capabilities, composer_attachment_summary_from_core,
    composer_attachment_summary_to_core, composer_message_attachments_from_metadata,
    service_execute_project_attachment_cleanup, service_pick_composer_attachments,
    service_preview_project_attachment_cleanup, service_stage_composer_attachment_bytes,
    service_stage_composer_attachment_paths,
};
use context_compaction::{
    MANUAL_CONTEXT_COMPACTION_TRIGGER, append_webui_context_compaction_status_message_to_workspace,
    context_compaction_budget_note_from_state,
    maybe_auto_compact_webui_session_before_provider_request, spawn_webui_context_compaction_task,
    webui_context_compaction_plan_from_conversation_state,
};
use conversation_history::{
    HydratedConversationWindowState, hydrated_conversation_window_summaries,
    load_hydrated_session_workbench, retain_hydrated_conversation_windows_for_workspace,
    service_session_history_page, service_session_messages, service_session_tool_detail,
};
use conversation_submission::{
    service_submit_conversation_prompt, service_submit_user_prompt_edit,
};
#[cfg(test)]
use execution_report_memory::apply_task_session_execution_report_to_memory;
use execution_report_memory::{
    apply_conversation_checkpoint, apply_execution_report_to_memory,
    apply_task_session_execution_report_to_memory_in_place,
    spawn_conversation_completion_hook_task,
};
use git::{
    WebuiWorkspaceDiffFileDetail, WebuiWorkspaceDiffFileDetailRequest, WebuiWorkspaceDiffRequest,
    WebuiWorkspaceDiffSearchRequest, WebuiWorkspaceDiffSearchResponse, WebuiWorkspaceDiffSnapshot,
    WebuiWorkspaceGitFileActionRequest, WebuiWorkspaceGitFileActionResult, git_commit_settings,
    load_webui_workspace_diff_file_detail, load_webui_workspace_diff_search,
    load_webui_workspace_diff_snapshot, run_webui_workspace_git_file_action,
    service_generate_git_commit_message, service_run_git_commit_action,
    service_save_git_commit_settings,
};
use pending_prompt_services::{
    service_edit_pending_prompt, service_remove_pending_prompt, service_reorder_pending_prompts,
};
use pending_prompts::{
    clear_internal_pending_prompt_queue_for_stop, enqueue_webui_pending_prompt_from_submission,
    pending_prompt_summaries_from_runtime_state, provider_configuration_for_model_reference,
    runtime_state_has_queued_pending_prompt_work, session_has_blocking_system_prompt_work,
    take_webui_internal_guidance_prompts,
};
pub use project_directories::service_add_project_from_path;
use project_directories::service_browse_project_directories;
use provider_settings::{
    service_remove_provider_configuration, service_save_provider_configuration,
    service_save_provider_enabled_models, service_save_provider_model_override,
};
use service_bootstrap::startup_snapshot_with_phase;
pub use service_bootstrap::{emit_initial_service_events, spawn_service_bootstrap};
use session_goal_lifecycle::{
    account_session_goal_from_report, cache_session_goal_live_usage_from_checkpoint,
    cache_session_goal_summary, clear_budget_limit_reported_goal_id,
    enqueue_session_goal_objective_updated_guidance, ensure_regular_session_exists,
    ensure_session_can_enter_plan_mode, ensure_session_not_in_plan_mode_for_goal,
    goal_summary_from_tool_finished_event, parse_session_goal_status,
    session_goal_context_for_prompt_from_state, session_goal_request_session_id,
    session_goal_summary_from_thread_goal, sync_selected_session_goal_summary_from_store,
    validate_session_goal_budget, validate_session_goal_objective,
};
#[cfg(test)]
use session_goal_lifecycle::{
    goal_token_delta_for_turn_stats, latest_session_turn_stats_from_state,
    render_goal_budget_limited_context, render_goal_continuation_context,
    render_goal_objective_updated_context, render_session_goal_context_for_prompt,
};
use session_hydration::{
    apply_hydration_to_selected_workspace_snapshot, capture_preserved_workspace_run_state,
    hydrate_retained_submission_session_if_needed, hydrate_selected_session_runtime_if_needed,
    load_selected_session_display_hydration, load_workspace_for_submission,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, runtime_state_has_concrete_conversation_context,
    sync_hydrated_session_dialogue_ready_marker,
};
use vmm::{
    service_load_project_vmm_config, service_run_vmm_memory_action, service_save_vmm_memory_config,
    static_vmm_profile_snapshot, vmm_memory_snapshot,
};
use vmm_hooks::{apply_vmm_writeback_effects_to_runtime_state, build_vmm_hook_config_snapshot};
use webui_prompt_lifecycle::{
    maybe_prepare_next_webui_pending_prompt, maybe_start_webui_mailbox_turn_if_ready,
    service_insert_pending_prompt, start_webui_precheck_or_continue,
    webui_shared_prompt_precheck_plan,
};
use webui_turn_chaining::{
    maybe_start_session_goal_continuation_if_idle, maybe_start_webui_turn_chain_after_finished,
};
use workbench_model_routes::{
    provider_secondary_route_summary_from_openrouter_selection,
    service_list_workbench_model_secondary_routes, service_select_workbench_model,
    service_select_workbench_model_secondary_route, service_select_workbench_reasoning_effort,
};
#[cfg(test)]
use workbench_surface_projection::active_surface_session_id_from_state;
use workbench_surface_projection::{
    active_surface_messages_from_workspace_and_state, preserved_active_surface_session_id,
    subagent_navigation_snapshot_from_cached_state, subagent_navigation_snapshot_from_state,
    subagent_run_state_key, surface_messages_for_session_without_subagent_navigation_refresh,
    webui_subagent_panel_has_recoverable_detail,
};
use workspace_files::{service_workspace_files_list, service_workspace_files_read};

/// Context-injection tag used for the latest session AI capability runtime update.
/// 最新会话 AI 能力运行时变更注入使用的上下文注入标签。
const SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG: &str = "session_ai_capability_update";
/// Stable dedupe key that replaces stale AI capability update notices in the current context window.
/// 用于在当前上下文窗口内替换旧 AI 能力变更通知的稳定去重键。
const SESSION_AI_CAPABILITY_UPDATE_DEDUPE_KEY: &str = "session_ai_capability_update:latest";
use vulcan_service_runtime::luaskills_bridge::{TauriLuaSkillsCatalog, TauriLuaSkillsHandle};
use vulcan_service_runtime::mcp_bridge::{TauriMcpRuntimeCatalog, TauriMcpRuntimeHandle};
use vulcan_service_runtime::token_budget_bridge::TauriTokenBudgetRuntime;

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
    /// Shared conversation hook runtime used to mirror WebUI turn assembly semantics.
    /// 用于复刻 WebUI 回合组装语义的共享会话 hook 运行时。
    conversation_hook_runtime: Arc<Mutex<ConversationHookRuntime>>,
    /// Shared VMM hook runtime snapshot used by built-in memory hooks.
    /// 内置记忆 hook 使用的共享 VMM hook 运行时快照。
    vmm_hook_runtime: VmmHookRuntimeHandle,
}

/// User prompt edit submit request sent by the WebUI transcript action.
/// WebUI 转录操作发送的用户提示词编辑提交请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPromptEditSubmitRequest {
    /// Conversation id that owns the user prompt.
    /// 拥有该用户提示词的会话标识。
    conversation_id: String,
    /// Stable user prompt message id.
    /// 稳定用户提示词消息标识。
    message_id: String,
    /// Replacement prompt text.
    /// 替换后的提示词文本。
    prompt: String,
    /// Attachments replayed with the edited prompt.
    /// 随编辑后提示词重放的附件。
    #[serde(default)]
    attachments: Vec<ComposerAttachmentSummary>,
    /// Whether the backend may stop an active run before editing.
    /// 后端是否可以先停止当前运行再执行编辑。
    #[serde(default)]
    stop_running: bool,
}

/// Conversation fork request sent by the WebUI final-turn action.
/// WebUI 最终回合操作发送的会话分叉请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationForkRequest {
    /// Conversation id that should be copied into a new session.
    /// 需要复制为新会话的会话标识。
    conversation_id: String,
    /// Localized title assigned to the forked session.
    /// 分配给分叉会话的本地化标题。
    title: String,
    /// Optional user message id whose previous boundary should become the fork endpoint.
    /// 可选用户消息标识；其上一边界将成为分叉终点。
    before_message_id: Option<String>,
}

/// Session-scoped AI capability configuration save request sent by the WebUI composer dialog.
/// WebUI 输入区弹窗发送的会话级 AI 能力配置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionAiCapabilityConfigurationSaveRequest {
    /// Owning project id used only as a fallback when no conversation id exists.
    /// 所属项目标识，仅在会话标识不存在时作为回退使用。
    project_id: Option<String>,
    /// Conversation id whose addon AI capability namespace should be updated.
    /// 需要更新附属 AI 能力命名空间的会话标识。
    conversation_id: Option<String>,
    /// Complete disabled capability kind list from the dialog draft.
    /// 弹窗草稿中的完整禁用能力类型列表。
    disabled_ai_capability_kinds: Vec<String>,
    /// Complete disabled target selector list from the dialog draft.
    /// 弹窗草稿中的完整禁用目标选择器列表。
    disabled_ai_capability_target_selectors: Vec<String>,
    /// Complete target order overrides grouped by capability kind.
    /// 按能力类型分组的完整目标顺序覆盖。
    ai_capability_target_order_overrides: BTreeMap<String, Vec<String>>,
}

/// Session goal save request sent by the WebUI composer goal dialog.
/// WebUI 输入区目标弹窗发送的会话目标保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionGoalSaveRequest {
    /// Conversation id whose goal should be created or updated.
    /// 需要创建或更新目标的会话标识。
    conversation_id: Option<String>,
    /// Concrete objective text to persist.
    /// 需要持久化的具体目标文本。
    objective: String,
    /// Optional positive token budget explicitly requested by the user.
    /// 用户明确要求时提供的可选正数 token 预算。
    token_budget: Option<i64>,
}

/// Session goal resume request sent by the WebUI budget-limited goal strip.
/// WebUI 预算耗尽目标条发送的会话目标继续请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionGoalResumeRequest {
    /// Conversation id whose stopped goal should resume.
    /// 需要恢复已停止目标的会话标识。
    conversation_id: Option<String>,
    /// User-authored objective that should continue in the next stage.
    /// 下一阶段需要继续推进的用户目标文本。
    objective: String,
    /// Optional positive token budget for the next stage.
    /// 下一阶段使用的可选正数 token 预算。
    token_budget: Option<i64>,
}

/// Session goal clear request sent by the WebUI goal area.
/// WebUI 目标区域发送的会话目标清理请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionGoalClearRequest {
    /// Conversation id whose goal should be removed.
    /// 需要移除目标的会话标识。
    conversation_id: Option<String>,
}

/// Session goal status update request sent by the WebUI goal area.
/// WebUI 目标区域发送的会话目标状态更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionGoalStatusUpdateRequest {
    /// Conversation id whose goal status should change.
    /// 需要更新目标状态的会话标识。
    conversation_id: Option<String>,
    /// Next status key accepted by the user-facing controls.
    /// 用户界面控件接受的下一状态键。
    status: String,
}

/// Session Plan mode update request sent by the WebUI composer controls.
/// WebUI 输入区控制项发送的会话计划模式更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionPlanModeUpdateRequest {
    /// Conversation id whose Plan mode state should change.
    /// 需要更新计划模式状态的会话标识。
    conversation_id: Option<String>,
    /// Whether Plan mode should be enabled for the selected session.
    /// 是否为当前会话启用计划模式。
    enabled: bool,
}

/// MCP server configuration save request sent by the WebUI settings page.
/// WebUI 设置页发送的 MCP 服务器配置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpServerSaveRequest {
    /// Existing server identifier, or `None` when creating one.
    /// 已有服务器标识；创建新服务器时为 `None`。
    server_id: Option<String>,
    /// User-facing server name.
    /// 面向用户显示的服务器名称。
    name: String,
    /// Whether the server should be registered by the runtime.
    /// 该服务器是否应由运行时注册。
    enabled: bool,
    /// Transport key such as `stdio`, `http`, or `sse`.
    /// 传输键，例如 `stdio`、`http` 或 `sse`。
    transport: String,
    /// Stdio command.
    /// Stdio 启动命令。
    command: String,
    /// Stdio argument vector.
    /// Stdio 参数列表。
    args: Vec<String>,
    /// Remote endpoint URL.
    /// 远端端点 URL。
    url: String,
    /// Remote bearer token.
    /// 远端 Bearer Token。
    bearer_token: String,
    /// Explicit HTTP headers.
    /// 显式 HTTP 请求头。
    headers: Vec<KeyValueInput>,
    /// HTTP headers resolved from environment variables.
    /// 从环境变量解析的 HTTP 请求头。
    headers_from_env: Vec<KeyValueInput>,
    /// OAuth client identifier.
    /// OAuth 客户端标识。
    oauth_client_id: String,
    /// OAuth client secret.
    /// OAuth 客户端密钥。
    oauth_client_secret: String,
    /// OAuth scope list.
    /// OAuth scope 列表。
    oauth_scopes: Vec<String>,
    /// OAuth redirect URI.
    /// OAuth 回调 URI。
    oauth_redirect_uri: String,
    /// Stdio working directory.
    /// Stdio 工作目录。
    cwd: String,
    /// Environment variables injected into stdio servers.
    /// 注入 stdio 服务器的环境变量。
    env: Vec<KeyValueInput>,
    /// Environment variable names passed through to stdio servers.
    /// 透传给 stdio 服务器的环境变量名。
    env_passthrough: Vec<String>,
}

/// Generic string key/value input used by settings save requests.
/// 设置保存请求使用的通用字符串键值输入。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyValueInput {
    /// Raw key.
    /// 原始键。
    key: String,
    /// Raw value.
    /// 原始值。
    value: String,
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
const SELECTED_SESSION_TURN_LOAD_LIMIT: usize = 30;

/// Maximum loaded subagent transcript surfaces retained by the Tauri service.
/// Tauri 服务保留的已加载子代理转录界面最大数量。
const SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT: usize = 128;

/// Milliseconds before an unused loaded subagent transcript surface can be evicted.
/// 已加载但未使用的子代理转录界面允许被清理前的毫秒数。
const SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS: i128 = 30 * 60 * 1000;

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

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
pub struct ServiceBridgeState {
    /// Host shell capabilities supplied by the concrete application shell.
    /// 由具体应用壳提供的宿主 shell 能力。
    shell_host: Arc<dyn ServiceShellHost>,
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
    /// Retained lazy conversation body windows keyed by session id.
    /// 按会话标识索引的懒加载会话正文保留窗口。
    hydrated_conversation_windows: BTreeMap<SessionId, HydratedConversationWindowState>,
    /// Long-lived shared agent runtime bridge.
    /// 长生命周期共享 Agent 运行时桥接。
    agent_runtime: Option<AgentRuntimeBridge>,
    /// Tokio runtime that owns bootstrap-spawned background tasks such as shared MCP access.
    /// 持有引导期后台任务的 Tokio 运行时，例如共享 MCP 访问任务。
    service_runtime: Option<Arc<tokio::runtime::Runtime>>,
    /// Browser service discovery snapshot shared by `/health` and the dev discovery file.
    /// `/health` 与开发发现文件共享的浏览器服务发现快照。
    webui_service_discovery: Option<SharedWebuiServiceDiscoverySnapshot>,
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
    /// Latest goal summaries keyed by session id for synchronous workbench projection.
    /// 按会话标识索引的最近目标摘要，用于同步构建工作台投影。
    session_goal_summaries: BTreeMap<SessionId, SessionGoalSummary>,
    /// Goal ids that already received one budget-limit wrap-up guidance in this process.
    /// 本进程内已经收到预算耗尽收尾引导的目标标识集合。
    budget_limit_reported_goal_ids: BTreeSet<String>,
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
    active_stream_tasks: BTreeMap<SessionId, ServiceTaskHandle<()>>,
    /// Session runtime kernels keyed by root WebUI session id.
    /// 按 WebUI 根会话标识索引的会话运行时内核。
    session_runtime_kernels: BTreeMap<SessionId, SessionRuntimeKernel>,
    /// Latest background persistence version keyed by session id.
    /// 按会话标识索引的最新后台持久化版本。
    session_persist_versions: BTreeMap<SessionId, u64>,
    /// Latest live transcript persistence version keyed by session id.
    /// 按会话标识索引的最新运行中转录持久化版本。
    session_transcript_persist_versions: BTreeMap<SessionId, u64>,
    /// Latest addon persistence version keyed by session id and addon namespace.
    /// 按会话标识与附属状态命名空间索引的最新附属状态持久化版本。
    session_addon_persist_versions: BTreeMap<(SessionId, SessionAddonNamespace), u64>,
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
type ServiceTaskHandle<T> = tokio::task::JoinHandle<T>;

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
fn spawn_service_task<F>(future: F) -> ServiceTaskHandle<F::Output>
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
fn spawn_service_blocking_task<F, R>(operation: F) -> ServiceTaskHandle<R>
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

/// Returns whether one WebUI session is idle enough for destructive user-prompt editing.
/// 返回单个 WebUI 会话是否空闲到足以执行破坏性用户提示词编辑。
fn webui_session_allows_user_prompt_editing(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
) -> bool {
    if session_id == SessionId::nil() {
        return false;
    }
    if state.active_stream_tasks.contains_key(&session_id)
        || webui_session_kernel_has_active_turn(state, session_id)
        || webui_session_has_active_run(state, session_id)
    {
        return false;
    }
    running_webui_subagent_task_states_for_tree_from_live_cache(state, session_id).is_empty()
}

/// Returns whether one workspace surface is visibly idle for user-prompt editing.
/// 返回单个工作区表面从可见状态看是否空闲到允许用户提示词编辑。
fn workspace_surface_allows_user_prompt_editing(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    subagent_navigation: Option<&SubagentNavigationSnapshot>,
) -> bool {
    if session_id == SessionId::nil() {
        return false;
    }
    if workspace_snapshot
        .sessions
        .iter()
        .chain(workspace_snapshot.archived_sessions.iter())
        .find(|session| session.id == session_id)
        .is_none_or(|session| session.status == SessionState::Running)
    {
        return false;
    }
    if workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .is_some_and(|phase| *phase == SessionRunPhase::Preparing)
    {
        return false;
    }
    if workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(|runtime_state| {
            runtime_state_has_queued_pending_prompt_work(runtime_state)
                || runtime_state
                    .active_precheck_turn_id
                    .as_deref()
                    .is_some_and(|turn_id| !turn_id.trim().is_empty())
        })
    {
        return false;
    }
    !subagent_navigation
        .is_some_and(|navigation| navigation.tasks.iter().any(|task| task.is_running))
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
        ExecutionStreamEvent::ProviderRequestCheckpoint { session_id, .. }
        | ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id }
        | ExecutionStreamEvent::ProviderLoweringRepair { session_id, .. }
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

/// Creates the initial shared service state used by any host shell.
/// 创建任意宿主壳使用的初始共享服务状态。
///
/// # Returns
/// 返回值
/// A shared state handle initialized with pending startup and fallback workbench snapshots.
/// 返回已初始化待启动快照和回退工作台快照的共享状态句柄。
///
/// # Parameters
/// 参数
/// `shell_host` provides host-specific file picker and open-url/path capabilities.
/// `shell_host` 提供宿主特定的文件选择器与打开 URL/路径能力。
pub fn create_service_bridge_state(
    shell_host: Arc<dyn ServiceShellHost>,
) -> SharedServiceBridgeState {
    Arc::new(Mutex::new(ServiceBridgeState {
        shell_host,
        startup: startup_snapshot_with_phase("pending", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
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
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    }))
}

/// Opens a trusted external URL with the system default browser.
/// 使用系统默认浏览器打开可信外部链接。
fn service_open_external_url(
    state: SharedServiceBridgeState,
    url: String,
) -> Result<String, String> {
    if url != "https://github.com/OpenVulcan/vulcan-code" {
        return Err(String::from("不允许打开未授权的外部链接。"));
    }
    shell_host_from_state(&state)?.open_url(&url)?;
    Ok(String::from("已请求使用系统默认浏览器打开 GitHub。"))
}

/// Returns the host shell adapter from shared service state.
/// 从共享服务状态中返回宿主 shell 适配器。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns the shell adapter.
/// `state` 是持有 shell 适配器的共享服务状态。
///
/// # Returns
/// 返回值
/// A cloned shell host adapter reference for use outside the state lock.
/// 返回可在状态锁之外使用的 shell 宿主适配器引用。
fn shell_host_from_state(
    state: &SharedServiceBridgeState,
) -> Result<Arc<dyn ServiceShellHost>, String> {
    state
        .lock()
        .map(|guard| Arc::clone(&guard.shell_host))
        .map_err(|error| format!("failed to lock service shell host: {error}"))
}

/// Dispatches one normalized service RPC request to the concrete local service implementation.
/// 将单个标准化服务 RPC 请求分发到具体本地服务实现。
async fn dispatch_service_rpc(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    let params = params.unwrap_or_else(|| json!({}));
    match method.as_str() {
        "startup.snapshot" | "service_startup_snapshot" => {
            rpc_json_result(service_startup_snapshot(state))
        }
        "workbench.snapshot" | "service_workbench_snapshot" => {
            rpc_json_result(service_workbench_snapshot(state))
        }
        "workspace.diff.snapshot" | "service_workspace_diff_snapshot" => rpc_json_result(
            service_workspace_diff_snapshot(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "request")?,
            )
            .await,
        ),
        "workspace.diff.fileDetail" | "service_workspace_diff_file_detail" => rpc_json_result(
            service_workspace_diff_file_detail(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "request")?,
                rpc_required_param(&params, "file")?,
            )
            .await,
        ),
        "workspace.diff.search" | "service_workspace_diff_search" => rpc_json_result(
            service_workspace_diff_search(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "request")?,
                rpc_required_param(&params, "search")?,
            )
            .await,
        ),
        "workspace.git.fileAction" | "service_workspace_git_file_action" => rpc_json_result(
            service_workspace_git_file_action(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "workspace.files.list" | "service_workspace_files_list" => rpc_json_result(
            service_workspace_files_list(
                rpc_required_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "directoryPath")?,
                rpc_optional_param(&params, "filters")?,
            )
            .await,
        ),
        "workspace.files.read" | "service_workspace_files_read" => rpc_json_result(
            service_workspace_files_read(
                rpc_required_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "filePath")?,
                rpc_optional_param(&params, "filters")?,
            )
            .await,
        ),
        "service_refresh_luaskills_catalog" => {
            rpc_json_result(service_refresh_luaskills_catalog(state).await)
        }
        "service_install_luaskill" => rpc_json_result(
            service_install_luaskill(
                state,
                rpc_optional_param(&params, "skillId")?,
                rpc_required_param(&params, "source")?,
                rpc_required_param(&params, "sourceType")?,
            )
            .await,
        ),
        "service_update_luaskill" => rpc_json_result(
            service_update_luaskill(state, rpc_required_param(&params, "skillId")?).await,
        ),
        "service_set_luaskill_enabled" => rpc_json_result(
            service_set_luaskill_enabled(
                state,
                rpc_required_param(&params, "skillId")?,
                rpc_required_param(&params, "enabled")?,
            )
            .await,
        ),
        "service_uninstall_luaskill" => rpc_json_result(
            service_uninstall_luaskill(state, rpc_required_param(&params, "skillId")?).await,
        ),
        "service_set_luaskill_tool_registration_state" => rpc_json_result(
            service_set_luaskill_tool_registration_state(
                state,
                rpc_required_param(&params, "skillId")?,
                rpc_required_param(&params, "localName")?,
                rpc_required_param(&params, "registrationState")?,
            )
            .await,
        ),
        "service_refresh_agent_skills_catalog" => {
            rpc_json_result(service_refresh_agent_skills_catalog(state).await)
        }
        "service_set_agent_skills_enabled" => rpc_json_result(
            service_set_agent_skills_enabled(state, rpc_required_param(&params, "enabled")?).await,
        ),
        "service_set_agent_skill_enabled" => rpc_json_result(
            service_set_agent_skill_enabled(
                state,
                rpc_required_param(&params, "name")?,
                rpc_required_param(&params, "enabled")?,
            )
            .await,
        ),
        "service_load_project_agent_skill_overrides" => {
            rpc_json_result(service_load_project_agent_skill_overrides(
                state,
                rpc_required_param(&params, "projectId")?,
            ))
        }
        "service_set_agent_skill_project_state" => {
            rpc_json_result(service_set_agent_skill_project_state(
                state,
                rpc_required_param(&params, "projectId")?,
                rpc_required_param(&params, "name")?,
                rpc_required_param(&params, "stateKey")?,
            ))
        }
        "service_run_vmm_memory_action" => rpc_json_result(
            service_run_vmm_memory_action(state, rpc_required_param(&params, "request")?).await,
        ),
        "service_load_project_vmm_config" => rpc_json_result(service_load_project_vmm_config(
            state,
            rpc_required_param(&params, "projectId")?,
        )),
        "service_preview_project_attachment_cleanup" => rpc_json_result(
            service_preview_project_attachment_cleanup(
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_execute_project_attachment_cleanup" => rpc_json_result(
            service_execute_project_attachment_cleanup(
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_vmm_memory_config" => rpc_json_result(
            service_save_vmm_memory_config(state, rpc_required_param(&params, "request")?).await,
        ),
        "service_session_messages" => rpc_json_result(
            service_session_messages(app, state, rpc_required_param(&params, "sessionId")?).await,
        ),
        "service_session_history_page" => rpc_json_result(
            service_session_history_page(
                app,
                state,
                rpc_required_param(&params, "sessionId")?,
                rpc_required_param(&params, "beforeOrdinal")?,
            )
            .await,
        ),
        "service_session_tool_detail" => rpc_json_result(
            service_session_tool_detail(
                state,
                rpc_required_param(&params, "sessionId")?,
                rpc_optional_param(&params, "recordId")?,
                rpc_optional_param(&params, "toolCallId")?,
                rpc_optional_param(&params, "seq")?,
                rpc_optional_param(&params, "messageIndex")?,
            )
            .await,
        ),
        "service_submit_conversation_prompt" => rpc_json_result(
            service_submit_conversation_prompt(
                app,
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "prompt")?,
                rpc_optional_param(&params, "hiddenModelGuidance")?,
                rpc_optional_param(&params, "executionMode")?,
                rpc_optional_param(&params, "reasoningEffort")?,
                rpc_optional_param(&params, "attachments")?,
                rpc_optional_param(&params, "disabledAiCapabilityKinds")?,
                rpc_optional_param(&params, "disabledAiCapabilityTargetSelectors")?,
                rpc_optional_param(&params, "aiCapabilityTargetOrderOverrides")?,
            )
            .await,
        ),
        "service_stop_conversation_run" => {
            rpc_json_result(service_stop_conversation_run(app, state).await)
        }
        "service_stop_command_session" => rpc_json_result(
            service_stop_command_session(
                app,
                state,
                rpc_required_param(&params, "commandSessionId")?,
                rpc_optional_param(&params, "ownerSessionId")?,
            )
            .await,
        ),
        "service_get_command_session_snapshot" => rpc_json_result(
            service_get_command_session_snapshot(
                state,
                rpc_required_param(&params, "commandSessionId")?,
                rpc_optional_param(&params, "ownerSessionId")?,
            )
            .await,
        ),
        "service_send_command_session_input" => rpc_json_result(
            service_send_command_session_input(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_resize_command_session" => rpc_json_result(
            service_resize_command_session(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_get_user_prompt_edit_draft" => rpc_json_result(
            service_get_user_prompt_edit_draft(
                state,
                rpc_required_param(&params, "conversationId")?,
                rpc_required_param(&params, "messageId")?,
            )
            .await,
        ),
        "service_submit_user_prompt_edit" => rpc_json_result(
            service_submit_user_prompt_edit(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_fork_conversation" => rpc_json_result(
            service_fork_conversation(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_save_session_goal" => rpc_json_result(
            service_save_session_goal(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_resume_session_goal" => rpc_json_result(
            service_resume_session_goal(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_clear_session_goal" => rpc_json_result(
            service_clear_session_goal(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_update_session_goal_status" => rpc_json_result(
            service_update_session_goal_status(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_set_session_plan_mode" => rpc_json_result(
            service_set_session_plan_mode(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_compact_selected_conversation_context" => {
            rpc_json_result(service_compact_selected_conversation_context(app, state).await)
        }
        "service_reply_pending_question" => rpc_json_result(service_reply_pending_question(
            app,
            state,
            rpc_required_param(&params, "requestId")?,
            rpc_required_param(&params, "answers")?,
        )),
        "service_reject_pending_question" => rpc_json_result(service_reject_pending_question(
            app,
            state,
            rpc_required_param(&params, "requestId")?,
        )),
        "service_open_subagent_panel" => rpc_json_result(
            service_open_subagent_panel(
                app,
                state,
                rpc_required_param(&params, "parentConversationId")?,
                rpc_required_param(&params, "taskId")?,
            )
            .await,
        ),
        "service_close_subagent_panel" => {
            rpc_json_result(service_close_subagent_panel(app, state).await)
        }
        "service_stop_subagent_task" => rpc_json_result(
            service_stop_subagent_task(
                app,
                state,
                rpc_required_param(&params, "parentConversationId")?,
                rpc_required_param(&params, "taskId")?,
            )
            .await,
        ),
        "service_edit_pending_prompt" => rpc_json_result(
            service_edit_pending_prompt(app, state, rpc_required_param(&params, "entryId")?).await,
        ),
        "service_insert_pending_prompt" => rpc_json_result(
            service_insert_pending_prompt(app, state, rpc_required_param(&params, "entryId")?)
                .await,
        ),
        "service_remove_pending_prompt" => rpc_json_result(
            service_remove_pending_prompt(app, state, rpc_required_param(&params, "entryId")?)
                .await,
        ),
        "service_reorder_pending_prompts" => rpc_json_result(
            service_reorder_pending_prompts(
                app,
                state,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "draggedEntryId")?,
                rpc_optional_param(&params, "targetEntryId")?,
            )
            .await,
        ),
        "service_pick_composer_attachments" => rpc_json_result(
            service_pick_composer_attachments(
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "kind")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_paths" => rpc_json_result(
            service_stage_composer_attachment_paths(
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "paths")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_bytes" => rpc_json_result(
            service_stage_composer_attachment_bytes(
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "fileName")?,
                rpc_required_param(&params, "mimeType")?,
                rpc_required_param(&params, "dataBase64")?,
            )
            .await,
        ),
        "service_select_workbench_model" => rpc_json_result(
            service_select_workbench_model(
                app,
                state,
                rpc_required_param(&params, "providerId")?,
                rpc_required_param(&params, "modelId")?,
            )
            .await,
        ),
        "service_list_workbench_model_secondary_routes" => rpc_json_result(
            service_list_workbench_model_secondary_routes(
                state,
                rpc_required_param(&params, "providerId")?,
                rpc_required_param(&params, "modelId")?,
            )
            .await,
        ),
        "service_select_workbench_model_secondary_route" => rpc_json_result(
            service_select_workbench_model_secondary_route(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_select_workbench_reasoning_effort" => rpc_json_result(
            service_select_workbench_reasoning_effort(
                app,
                state,
                rpc_required_param(&params, "effort")?,
            )
            .await,
        ),
        "service_save_provider_configuration" => rpc_json_result(
            service_save_provider_configuration(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_remove_provider_configuration" => rpc_json_result(
            service_remove_provider_configuration(
                app,
                state,
                rpc_required_param(&params, "providerId")?,
            )
            .await,
        ),
        "service_save_provider_enabled_models" => rpc_json_result(
            service_save_provider_enabled_models(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_provider_model_override" => rpc_json_result(
            service_save_provider_model_override(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_git_commit_settings" => rpc_json_result(
            service_save_git_commit_settings(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_generate_git_commit_message" => rpc_json_result(
            service_generate_git_commit_message(state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_run_git_commit_action" => rpc_json_result(
            service_run_git_commit_action(state, rpc_required_param(&params, "request")?).await,
        ),
        "service_save_ai_capability_vendor" => rpc_json_result(
            service_save_ai_capability_vendor(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_remove_ai_capability_vendor" => rpc_json_result(
            service_remove_ai_capability_vendor(
                app,
                state,
                rpc_required_param(&params, "vendorId")?,
            )
            .await,
        ),
        "service_set_ai_capability_target_enabled" => rpc_json_result(
            service_set_ai_capability_target_enabled(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_ai_capability_targets" => rpc_json_result(
            service_save_ai_capability_targets(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_save_session_ai_capability_configuration" => rpc_json_result(
            service_save_session_ai_capability_configuration(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_mcp_server_configuration" => rpc_json_result(
            service_save_mcp_server_configuration(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_remove_mcp_server_configuration" => rpc_json_result(
            service_remove_mcp_server_configuration(
                app,
                state,
                rpc_required_param(&params, "serverId")?,
            )
            .await,
        ),
        "service_open_project_directory_path" => rpc_json_result(
            service_open_project_directory_path(state, rpc_required_param(&params, "path")?),
        ),
        "service_open_project_directory" => rpc_json_result(service_open_project_directory(
            state,
            rpc_required_param(&params, "projectId")?,
        )),
        "service_browse_project_directories" => rpc_json_result(
            service_browse_project_directories(state, rpc_optional_param(&params, "path")?),
        ),
        "service_add_project_from_path" => rpc_json_result(
            service_add_project_from_path(
                state,
                PathBuf::from(rpc_required_param::<String>(&params, "path")?),
            )
            .await,
        ),
        "service_toggle_project_pinned" => rpc_json_result(
            service_toggle_project_pinned(state, rpc_required_param(&params, "projectId")?).await,
        ),
        "service_reorder_projects" => rpc_json_result(
            service_reorder_projects(state, rpc_required_param(&params, "projectIds")?).await,
        ),
        "service_rename_project" => rpc_json_result(
            service_rename_project(
                state,
                rpc_required_param(&params, "projectId")?,
                rpc_required_param(&params, "name")?,
            )
            .await,
        ),
        "service_set_project_icon" => rpc_json_result(
            service_set_project_icon(
                state,
                rpc_required_param(&params, "projectId")?,
                rpc_required_param(&params, "iconPack")?,
                rpc_required_param(&params, "iconKey")?,
            )
            .await,
        ),
        "service_reset_project_icon" => rpc_json_result(
            service_reset_project_icon(state, rpc_required_param(&params, "projectId")?).await,
        ),
        "service_set_expanded_task_projects" => rpc_json_result(
            service_set_expanded_task_projects(state, rpc_required_param(&params, "projectIds")?)
                .await,
        ),
        "service_archive_project_conversations" => rpc_json_result(
            service_archive_project_conversations(state, rpc_required_param(&params, "projectId")?)
                .await,
        ),
        "service_remove_project" => rpc_json_result(
            service_remove_project(state, rpc_required_param(&params, "projectId")?).await,
        ),
        "service_rename_conversation" => rpc_json_result(
            service_rename_conversation(
                state,
                rpc_required_param(&params, "conversationId")?,
                rpc_required_param(&params, "name")?,
            )
            .await,
        ),
        "service_archive_conversation" => rpc_json_result(
            service_archive_conversation(state, rpc_required_param(&params, "conversationId")?)
                .await,
        ),
        "service_mark_conversation_read" => rpc_json_result(
            service_mark_conversation_read(state, rpc_required_param(&params, "conversationId")?)
                .await,
        ),
        "service_select_conversation" => rpc_json_result(
            service_select_conversation(state, rpc_required_param(&params, "conversationId")?)
                .await,
        ),
        "service_open_external_url" => rpc_json_result(service_open_external_url(
            state,
            rpc_required_param(&params, "url")?,
        )),
        _ => Err(format!("unknown service RPC method: {method}")),
    }
}

/// Serializes a service handler result into a JSON RPC payload.
/// 将服务处理器结果序列化为 JSON RPC 载荷。
fn rpc_json_result<T>(result: Result<T, String>) -> Result<Value, String>
where
    T: Serialize,
{
    result.and_then(|value| {
        serde_json::to_value(value)
            .map_err(|error| format!("failed to serialize service RPC response: {error}"))
    })
}

/// Reads a required typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取必填类型值。
fn rpc_required_param<T>(params: &Value, key: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let object = params
        .as_object()
        .ok_or_else(|| String::from("service RPC params must be a JSON object"))?;
    let value = object
        .get(key)
        .ok_or_else(|| format!("missing service RPC parameter: {key}"))?;
    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid service RPC parameter `{key}`: {error}"))
}

/// Reads an optional typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取可选类型值。
fn rpc_optional_param<T>(params: &Value, key: &str) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    let Some(object) = params.as_object() else {
        return Err(String::from("service RPC params must be a JSON object"));
    };
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| format!("invalid service RPC parameter `{key}`: {error}"))
}

/// Spawns the browser-facing local service bridge on the first available loopback port.
/// 在首个可用回环端口启动面向浏览器的本地服务桥接层。
pub fn spawn_webui_service_http_bridge(
    event_emitter: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    event_bus: broadcast::Sender<ServiceEvent>,
) {
    std::thread::Builder::new()
        .name(String::from("vulcan-webui-http-bridge"))
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!(
                        "[webui-service] browser service bridge runtime creation failed: {error}"
                    );
                    return;
                }
            };
            runtime.block_on(run_webui_service_http_bridge(
                event_emitter,
                state,
                event_bus,
            ));
        })
        .expect("failed to spawn Vulcan WebUI HTTP bridge thread");
}

/// Runs the browser-facing local service bridge inside an owned Tokio runtime.
/// 在自有 Tokio 运行时中运行面向浏览器的本地服务桥接层。
///
/// # Parameters
/// 参数
/// `event_emitter` forwards service events to all frontend sinks.
/// `event_emitter` 将服务事件转发给所有前端接收器。
/// `state` is the shared service state used by RPC dispatch.
/// `state` 是 RPC 分发使用的共享服务状态。
/// `event_bus` broadcasts service events to WebSocket clients.
/// `event_bus` 将服务事件广播给 WebSocket 客户端。
async fn run_webui_service_http_bridge(
    event_emitter: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    event_bus: broadcast::Sender<ServiceEvent>,
) {
    let port_override = env::var(WEBUI_SERVICE_PORT_ENV).ok();
    let (listener, address) = match bind_webui_service_http_listener(port_override.as_deref()).await
    {
        Ok(bound) => bound,
        Err(error) => {
            eprintln!("[webui-service] browser service bridge failed to bind: {error}");
            return;
        }
    };
    let port = address.port();
    let service_token = SessionId::new_v4().to_string();
    let discovery = webui_service_starting_discovery_snapshot(
        port,
        std::process::id(),
        current_unix_millis_text(),
        service_token.clone(),
    );
    let shared_discovery = Arc::new(Mutex::new(discovery.clone()));
    if let Ok(mut guard) = state.lock() {
        guard.webui_service_discovery = Some(Arc::clone(&shared_discovery));
    }
    if let Err(error) =
        write_webui_service_discovery_snapshot(&discovery, dirs::home_dir(), std::env::temp_dir())
    {
        eprintln!("[webui-service] browser service discovery write failed: {error}");
    }
    let rpc_event_emitter = event_emitter.clone();
    let rpc_state = state.clone();
    let rpc_dispatcher: WebuiServiceRpcDispatcher = Arc::new(move |method, params| {
        let event_emitter = rpc_event_emitter.clone();
        let state = rpc_state.clone();
        Box::pin(async move { dispatch_service_rpc(event_emitter, state, method, params).await })
    });
    let service_config = WebuiServiceHttpConfig {
        discovery: shared_discovery,
        service_token,
        event_bus,
        static_root: webui_static_root(),
        rpc_dispatcher,
    };
    eprintln!("[webui-service] browser service bridge listening on http://{address}");
    if let Err(error) = serve_webui_service_http_listener(listener, service_config).await {
        eprintln!("[webui-service] browser service bridge stopped: {error}");
    }
}

/// Marks the browser-facing service discovery snapshot with a terminal bootstrap status.
/// 使用终态引导状态标记浏览器侧服务发现快照。
///
/// # Parameters
/// 参数
/// `state` contains the shared discovery snapshot, and `status` is the new discovery status string.
/// `state` 包含共享发现快照，`status` 是新的发现状态字符串。
fn mark_webui_service_discovery_status(state: &SharedServiceBridgeState, status: &'static str) {
    let shared_discovery = match state.lock() {
        Ok(guard) => guard.webui_service_discovery.clone(),
        Err(error) => error.into_inner().webui_service_discovery.clone(),
    };
    let Some(shared_discovery) = shared_discovery else {
        return;
    };
    let ready_snapshot = {
        let mut snapshot = match shared_discovery.lock() {
            Ok(snapshot) => snapshot,
            Err(error) => error.into_inner(),
        };
        let port = snapshot.port;
        let pid = snapshot.pid;
        let started_at = snapshot.started_at.clone();
        let service_token = snapshot.service_token.clone();
        *snapshot = webui_service_discovery_snapshot_with_status(
            status,
            port,
            pid,
            started_at,
            service_token,
        );
        snapshot.clone()
    };
    if let Err(error) = write_webui_service_discovery_snapshot(
        &ready_snapshot,
        dirs::home_dir(),
        std::env::temp_dir(),
    ) {
        eprintln!("[webui-service] browser service discovery ready write failed: {error}");
    }
}

/// Marks the browser-facing service discovery snapshot as ready after bootstrap completes.
/// 在引导完成后将浏览器侧服务发现快照标记为 ready。
fn mark_webui_service_discovery_ready(state: &SharedServiceBridgeState) {
    mark_webui_service_discovery_status(state, WEBUI_SERVICE_DISCOVERY_STATUS_READY);
}

/// Marks the browser-facing service discovery snapshot as failed after bootstrap fails.
/// 在引导失败后将浏览器侧服务发现快照标记为 failed。
fn mark_webui_service_discovery_failed(state: &SharedServiceBridgeState) {
    mark_webui_service_discovery_status(state, WEBUI_SERVICE_DISCOVERY_STATUS_FAILED);
}

/// Resolves the WebUI static root used by the browser-facing service.
/// 解析面向浏览器服务使用的 WebUI 静态根目录。
fn webui_static_root() -> Option<PathBuf> {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vulcan-code-web/dist")
        .components()
        .collect::<PathBuf>();
    let override_path = env::var(WEBUI_STATIC_DIR_ENV).ok();
    webui_static_root_from_candidates(override_path.as_deref(), Some(manifest_path))
}

/// Returns the current startup snapshot.
/// 返回当前启动快照。
fn service_startup_snapshot(state: SharedServiceBridgeState) -> Result<StartupSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.startup.clone())
        .map_err(|error| format!("failed to lock service startup state: {error}"))
}

/// Returns the current workbench snapshot.
/// 返回当前工作台快照。
fn service_workbench_snapshot(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map(|guard| workbench_with_live_pending_question(&guard))
        .map_err(|error| format!("failed to lock service workbench state: {error}"))
}

/// Loads a Git diff snapshot for one WebUI project workspace.
/// 为单个 WebUI 项目工作区加载 Git diff 快照。
async fn service_workspace_diff_snapshot(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: Option<WebuiWorkspaceDiffRequest>,
) -> Result<WebuiWorkspaceDiffSnapshot, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        load_webui_workspace_diff_snapshot(&resolved_workspace_path, request)
    })
    .await
    .map_err(|error| format!("workspace diff task failed: {error}"))?
}

/// Loads a Git diff detail for one WebUI project file.
/// 为单个 WebUI 项目文件加载 Git diff 详情。
async fn service_workspace_diff_file_detail(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: Option<WebuiWorkspaceDiffRequest>,
    file: WebuiWorkspaceDiffFileDetailRequest,
) -> Result<WebuiWorkspaceDiffFileDetail, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        load_webui_workspace_diff_file_detail(&resolved_workspace_path, request, file)
    })
    .await
    .map_err(|error| format!("workspace diff file detail task failed: {error}"))?
}

/// Searches the complete backend Git diff and returns compact file-level hits.
/// 搜索后端完整 Git diff 并返回紧凑的文件级命中。
async fn service_workspace_diff_search(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: Option<WebuiWorkspaceDiffRequest>,
    search: WebuiWorkspaceDiffSearchRequest,
) -> Result<WebuiWorkspaceDiffSearchResponse, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        load_webui_workspace_diff_search(&resolved_workspace_path, request, search)
    })
    .await
    .map_err(|error| format!("workspace diff search task failed: {error}"))?
}

/// Executes one file-level Git action for a WebUI workspace.
/// 为 WebUI 工作区执行单个文件级 Git 操作。
async fn service_workspace_git_file_action(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: WebuiWorkspaceGitFileActionRequest,
) -> Result<WebuiWorkspaceGitFileActionResult, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        run_webui_workspace_git_file_action(&resolved_workspace_path, request)
    })
    .await
    .map_err(|error| format!("workspace Git file action task failed: {error}"))?
}

/// Resolves the workspace path for a WebUI diff request from backend-owned state first.
/// 优先从后端持有的状态中解析 WebUI diff 请求对应的工作区路径。
fn webui_diff_workspace_path_from_state(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    fallback_path: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(fallback_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())));
    };
    let resolved_project_id = project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            conversation_id
                .and_then(parse_session_id)
                .and_then(|session_id| {
                    owning_project_id_for_session(workspace_snapshot, session_id)
                })
        });
    if let Some(resolved_project_id) = resolved_project_id
        && let Some(project) = workspace_snapshot
            .task_projects
            .iter()
            .find(|project| project.id == resolved_project_id)
    {
        return Ok(project.path.clone());
    }
    Ok(fallback_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())))
}

/// Runs a LuaSkills management operation on a blocking worker and returns its result.
/// 在阻塞工作线程运行 LuaSkills 管理操作并返回结果。
async fn run_luaskills_management_blocking<T, F>(
    label: &'static str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    spawn_service_blocking_task(operation)
        .await
        .map_err(|error| format!("LuaSkills 管理操作 `{label}` 任务异常结束：{error}"))?
}

/// Refreshes the LuaSkills catalog used by the WebUI skill page.
/// 刷新 WebUI 技能页面使用的 LuaSkills 目录。
async fn service_refresh_luaskills_catalog(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let catalog = run_luaskills_management_blocking("refresh catalog", move || {
        vulcan_service_runtime::luaskills_bridge::refresh_luaskills_catalog(&handle)
    })
    .await?;
    store_luaskills_catalog(&state, catalog)?;
    refresh_workbench_snapshot(state).await
}

/// Installs one LuaSkill package into the USER layer and refreshes the WebUI snapshot.
/// 将单个 LuaSkill 包安装到 USER 层并刷新 WebUI 快照。
async fn service_install_luaskill(
    state: SharedServiceBridgeState,
    skill_id: Option<String>,
    source: String,
    source_type: String,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("install skill", move || {
        vulcan_service_runtime::luaskills_bridge::install_luaskill(
            &handle,
            skill_id,
            source,
            source_type,
        )
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Updates one managed LuaSkill package and refreshes the WebUI snapshot.
/// 更新单个受管 LuaSkill 包并刷新 WebUI 快照。
async fn service_update_luaskill(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("update skill", move || {
        vulcan_service_runtime::luaskills_bridge::update_luaskill(&handle, skill_id)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Enables or disables one LuaSkill package and refreshes the WebUI snapshot.
/// 启用或停用单个 LuaSkill 包并刷新 WebUI 快照。
async fn service_set_luaskill_enabled(
    state: SharedServiceBridgeState,
    skill_id: String,
    enabled: bool,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("set skill enabled", move || {
        vulcan_service_runtime::luaskills_bridge::set_luaskill_enabled(&handle, skill_id, enabled)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Uninstalls one ordinary LuaSkill package and refreshes the WebUI snapshot.
/// 卸载单个普通 LuaSkill 包并刷新 WebUI 快照。
async fn service_uninstall_luaskill(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("uninstall skill", move || {
        vulcan_service_runtime::luaskills_bridge::uninstall_luaskill(&handle, skill_id)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Persists one LuaSkill tool registration state and refreshes the WebUI snapshot.
/// 持久化单个 LuaSkill 工具注册状态并刷新 WebUI 快照。
async fn service_set_luaskill_tool_registration_state(
    state: SharedServiceBridgeState,
    skill_id: String,
    local_name: String,
    registration_state: String,
) -> Result<WorkbenchSnapshot, String> {
    let next_state = parse_luaskill_tool_registration_state(&registration_state)?;
    mutate_provider_configuration(&state, |configuration| {
        configuration
            .luaskills
            .set_tool_registration_state(&skill_id, &local_name, next_state);
        Ok(())
    })?;
    refresh_luaskills_local_state(&state)
}

/// Refreshes Agent Skills by rebuilding the current WebUI workbench snapshot.
/// 通过重建当前 WebUI 工作台快照刷新 Agent Skills。
async fn service_refresh_agent_skills_catalog(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    refresh_workbench_snapshot(state).await
}

/// Persists the Agent Skills global switch and refreshes only the Agent Skills catalog.
/// 持久化 Agent Skills 全局开关并仅刷新 Agent Skills 目录。
async fn service_set_agent_skills_enabled(
    state: SharedServiceBridgeState,
    enabled: bool,
) -> Result<AgentSkillsCatalogSummary, String> {
    mutate_provider_configuration(&state, |configuration| {
        configuration.prompt_skills.enabled = enabled;
        Ok(())
    })?;
    refresh_agent_skills_local_state(&state)
}

/// Persists one global Agent Skill state and refreshes only the Agent Skills catalog.
/// 持久化单个全局 Agent Skill 状态并仅刷新 Agent Skills 目录。
async fn service_set_agent_skill_enabled(
    state: SharedServiceBridgeState,
    name: String,
    enabled: bool,
) -> Result<AgentSkillsCatalogSummary, String> {
    let state_override = if enabled {
        PersistedPromptSkillState::Enabled
    } else {
        PersistedPromptSkillState::Disabled
    };
    mutate_provider_configuration(&state, |configuration| {
        configuration
            .prompt_skills
            .set_skill_state(&name, Some(state_override));
        Ok(())
    })?;
    refresh_agent_skills_local_state(&state)
}

/// Loads project-level Agent Skill overrides for the WebUI project settings modal.
/// 为 WebUI 项目设置弹窗加载项目级 Agent Skill 覆盖。
fn service_load_project_agent_skill_overrides(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<BTreeMap<String, String>, String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let project_configuration = load_project_configuration(&project_path);
    Ok(project_configuration
        .prompt_skills
        .skill_overrides
        .into_iter()
        .map(|(name, state)| {
            let value = if state.is_enabled() {
                "enabled"
            } else {
                "disabled"
            };
            (name, value.to_string())
        })
        .collect())
}

/// Persists one project-level Agent Skill tri-state override without rebuilding the full snapshot.
/// 持久化单个项目级 Agent Skill 三态覆盖，但不重建完整快照。
fn service_set_agent_skill_project_state(
    state: SharedServiceBridgeState,
    project_id: String,
    name: String,
    state_key: String,
) -> Result<(), String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let mut project_configuration = load_project_configuration(&project_path);
    let stable_name = name.trim();
    if stable_name.is_empty() {
        return Err(String::from("项目级 Agent Skill 名称不能为空。"));
    }
    match parse_agent_skill_project_state(&state_key)? {
        Some(state_override) => {
            project_configuration
                .prompt_skills
                .skill_overrides
                .insert(stable_name.to_string(), state_override);
        }
        None => {
            project_configuration
                .prompt_skills
                .skill_overrides
                .remove(stable_name);
        }
    }
    vulcan_provider_config::config::save_project_configuration_file(
        &PathBuf::from(&project_path),
        &project_configuration,
    )?;
    Ok(())
}

/// Submits answers for the oldest visible ask-tool request and resumes the blocked tool call.
/// 提交当前可见 ask 工具请求的答案，并恢复被阻塞的工具调用。
fn service_reply_pending_question(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request_id: String,
    answers: Vec<Vec<String>>,
) -> Result<WorkbenchSnapshot, String> {
    let agent_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; question runtime is not ready")
        })?
    };
    let answered = agent_runtime
        .question_runtime
        .reply(&request_id, answers)
        .map_err(|error| format!("failed to submit ask answers: {error}"))?;
    if !answered {
        return Err(format!(
            "pending ask request `{request_id}` is no longer active"
        ));
    }
    store_and_emit_pending_question_workbench_snapshot(&app, &state)
}

/// Rejects the visible ask-tool request after explicit user confirmation.
/// 在用户明确确认后拒绝当前可见 ask 工具请求。
fn service_reject_pending_question(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let agent_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; question runtime is not ready")
        })?
    };
    let rejected = agent_runtime
        .question_runtime
        .reject(&request_id)
        .map_err(|error| format!("failed to reject ask request: {error}"))?;
    if !rejected {
        return Err(format!(
            "pending ask request `{request_id}` is no longer active"
        ));
    }
    store_and_emit_pending_question_workbench_snapshot(&app, &state)
}

/// Loads one backend-validated draft for a user prompt edit dialog.
/// 加载一份供用户提示词编辑弹窗使用的后端校验草稿。
async fn service_get_user_prompt_edit_draft(
    state: SharedServiceBridgeState,
    conversation_id: String,
    message_id: String,
) -> Result<UserPromptEditDraftSummary, String> {
    let session_id = parse_session_id(&conversation_id)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_INVALID_CONVERSATION"))?;
    let controller_runtime = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        if !webui_session_allows_user_prompt_editing(&mut guard, session_id) {
            return Err(String::from("ERR_USER_PROMPT_EDIT_SESSION_BUSY"));
        }
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        if !workspace_user_prompt_message_allows_destructive_edit(
            workspace,
            session_id,
            &message_id,
        ) {
            return Err(String::from("ERR_USER_PROMPT_EDIT_NOT_EDITABLE"));
        }
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?
    };
    let target =
        load_persisted_user_prompt_edit_draft(&controller_runtime, session_id, &message_id).await?;
    Ok(UserPromptEditDraftSummary {
        conversation_id,
        message_id,
        prompt: target.prompt,
        attachments: target
            .attachments
            .iter()
            .map(composer_attachment_summary_from_core)
            .collect(),
        running: false,
    })
}

/// Submits one user prompt edit by rolling back the tail and immediately replaying the edited prompt.
/// 通过回滚尾部并立即重放编辑后提示词来提交一次用户提示词编辑。
/// Forks one persisted conversation into a new idle session and opens the clone.
/// 将一个持久化会话分叉为新的空闲会话并打开克隆。
async fn service_fork_conversation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: ConversationForkRequest,
) -> Result<WorkbenchSnapshot, String> {
    let source_session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_INVALID_CONVERSATION"))?;
    let (controller_runtime, source_session, project_id) = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        if !webui_session_allows_user_prompt_editing(&mut guard, source_session_id) {
            return Err(String::from("ERR_SESSION_FORK_SESSION_BUSY"));
        }
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        let source_session = workspace
            .sessions
            .iter()
            .find(|session| session.id == source_session_id)
            .cloned()
            .ok_or_else(|| String::from("ERR_SESSION_FORK_SOURCE_NOT_FOUND"))?;
        let project_id = owning_project_id_for_session(workspace, source_session_id)
            .ok_or_else(|| String::from("ERR_SESSION_FORK_PROJECT_NOT_FOUND"))?;
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        (controller_runtime, source_session, project_id)
    };
    let target_session_id = SessionId::new_v4();
    let fork_title = if request.title.trim().is_empty() {
        source_session.title.clone()
    } else {
        request.title.trim().to_string()
    };
    let fork_result = fork_persisted_session(
        &controller_runtime,
        &source_session,
        &project_id,
        target_session_id,
        fork_title,
    )
    .await?;
    if let Some(before_message_id) = request
        .before_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        rollback_persisted_session_before_user_prompt(
            &controller_runtime,
            fork_result.session.id,
            before_message_id,
        )
        .await?;
    }
    record_persisted_session_fork_source_marker(
        &controller_runtime,
        fork_result.session.id,
        &source_session,
        &project_id,
    )?;
    let target_session_id_text = fork_result.session.id.to_string();
    load_hydrated_session_workbench(
        &app,
        &state,
        fork_result.session.id,
        &target_session_id_text,
    )
    .await
    .map(|hydrated| hydrated.workbench)
}

/// Rebuilds only the ask-modal projection on the retained workbench snapshot and emits it.
/// 仅重建保留工作台快照上的 ask 弹窗投影并发送事件。
fn store_and_emit_pending_question_workbench_snapshot(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let pending_question =
            pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.pending_question = pending_question;
        guard.workbench.clone()
    };
    emit_service_event_with_trace(
        app,
        "pending_question_workbench_emit",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
}

/// Installs the WebUI projection callback used by the ask-question runtime.
/// 安装 ask 提问运行时使用的 WebUI 投影回调。
fn install_webui_question_runtime_event_bridge(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    question_runtime: &Arc<QuestionRuntime>,
) -> Result<(), String> {
    let app = app.clone();
    let state = Arc::clone(state);
    let callback: PendingQuestionChangeCallback = Arc::new(move || {
        if let Err(error) = store_and_emit_pending_question_workbench_snapshot(&app, &state) {
            webui_trace(
                "pending_question_projection_failed",
                format!("error={error}"),
            );
        }
    });
    question_runtime.set_change_callback(Some(callback))
}

/// Returns the retained workbench with the latest live ask-runtime pending request attached.
/// 返回附加最新实时 ask 运行时待处理请求的保留工作台。
fn workbench_with_live_pending_question(guard: &ServiceBridgeState) -> WorkbenchSnapshot {
    let mut workbench = guard.workbench.clone();
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() {
        workbench.hydrated_conversation_windows =
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows);
        append_hydrated_workspace_messages(&mut workbench.messages, workspace_snapshot);
    }
    workbench.pending_question =
        pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
    workbench
}

/// Returns the display transcript window for one session from a concrete workspace snapshot.
/// 从具体工作区快照返回单个会话的显示层转录窗口。
fn display_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(messages) = workspace_snapshot.session_messages.get(&session_id) {
        return Some(filter_legacy_goal_internal_user_messages(messages.clone()));
    }
    display_record_messages_from_workspace(workspace_snapshot, session_id)
}

/// Returns display-record messages for fallback rendering without deriving from runtime-only messages.
/// 返回用于显示回退的记录流消息，不从仅运行态消息派生。
fn display_record_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) {
        let messages = conversation_state_to_session_messages(&runtime_state.conversation_state);
        if !messages.is_empty() {
            return Some(filter_legacy_goal_internal_user_messages(messages));
        }
    }
    None
}

/// Merges one runtime dialogue state into the display transcript window for a session.
/// 将单个运行态对话状态合并进对应会话的显示层转录窗口。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the workspace snapshot whose display layer will be updated.
/// `workspace_snapshot` 是显示层将被更新的工作区快照。
/// `session_id` selects the session that owns both layers.
/// `session_id` 选择同时拥有两层状态的会话。
/// `runtime_state` is the dialogue-layer runtime state to project into display rows.
/// `runtime_state` 是需要投影成显示行的对话层运行态。
fn merge_runtime_state_records_into_display_messages(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    runtime_state: &SessionRuntimeStateEntry,
) {
    let merged_messages = merge_session_messages_with_conversation_state_records(
        workspace_snapshot
            .session_messages
            .get(&session_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &runtime_state.conversation_state,
    );
    workspace_snapshot
        .session_messages
        .insert(session_id, merged_messages);
}

/// Removes legacy persisted Goal internal prompts that were incorrectly stored as user messages.
/// 移除历史上被错误持久化成用户消息的 Goal 内部提示词。
fn filter_legacy_goal_internal_user_messages(
    messages: Vec<SessionMessageEntry>,
) -> Vec<SessionMessageEntry> {
    messages
        .into_iter()
        .filter(|message| !session_message_is_legacy_goal_internal_user_prompt(message))
        .collect()
}

/// Rewrites one runtime state after removing legacy Goal internal user messages.
/// 移除历史 Goal 内部用户消息后重写单个运行态。
fn sanitize_runtime_state_legacy_goal_internal_messages(
    runtime_state: &mut SessionRuntimeStateEntry,
) {
    let messages = conversation_state_to_session_messages(&runtime_state.conversation_state);
    if messages
        .iter()
        .any(session_message_is_legacy_goal_internal_user_prompt)
    {
        runtime_state.conversation_state = conversation_state_from_session_messages(
            &filter_legacy_goal_internal_user_messages(messages),
        );
    }
}

/// Returns whether one message is a known legacy Goal internal prompt persisted before the hidden fix.
/// 返回单条消息是否为隐藏修复前已知被持久化的历史 Goal 内部提示词。
fn session_message_is_legacy_goal_internal_user_prompt(message: &SessionMessageEntry) -> bool {
    if message.kind != SessionMessageKind::User {
        return false;
    }
    let body = message.body.trim_start();
    body.starts_with("Continue working toward the active thread goal.")
        || body.starts_with("The active thread goal has reached its token budget.")
        || body.starts_with("The active thread goal objective was edited by the user.")
}

/// Submits one user prompt through the shared agent runtime and returns the immediate workbench snapshot.
/// 通过共享 Agent 运行时提交一条用户提示词，并返回即时工作台快照。
/// Starts prompt-based compaction for the selected WebUI conversation.
/// 为当前选中的 WebUI 会话启动提示词式上下文压缩。
async fn service_compact_selected_conversation_context(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
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
        "context_compaction_requested",
        format!("session={selected_session_id}"),
    );

    let (has_active_stream_task, has_active_kernel_turn, has_active_session_run) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.active_stream_tasks.contains_key(&selected_session_id),
            webui_session_kernel_has_active_turn(&guard, selected_session_id),
            webui_session_has_active_run(&guard, selected_session_id),
        )
    };
    if has_active_stream_task || has_active_kernel_turn || has_active_session_run {
        return Err(String::from(
            "当前会话仍有运行中的执行链路，暂时无法执行手动上下文压缩。",
        ));
    }

    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    hydrate_selected_session_runtime_if_needed(
        controller_runtime.clone(),
        &mut workspace_snapshot,
        selected_session_id,
    )
    .await?;
    ensure_webui_session_runtime_state(&mut workspace_snapshot, selected_session_id, None);
    let runtime_state_snapshot = workspace_snapshot
        .session_runtime_state
        .get(&selected_session_id)
        .cloned()
        .unwrap_or_default();
    let plan = webui_context_compaction_plan_from_conversation_state(
        selected_session_id,
        &runtime_state_snapshot.conversation_state,
        runtime_state_snapshot.context_start_message_id.as_deref(),
        "当前会话暂时没有可压缩的历史上下文。",
    )?;

    let budget_note = context_compaction_budget_note_from_state(
        &state,
        selected_session_id,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
    );
    let previous_phase = workspace_snapshot
        .session_run_phases
        .get(&selected_session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Ready);
    let status_message_id = append_webui_context_compaction_status_message_to_workspace(
        &mut workspace_snapshot,
        selected_session_id,
        &budget_note,
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
    );
    workspace_snapshot
        .session_run_phases
        .insert(selected_session_id, SessionRunPhase::Preparing);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == selected_session_id)
    {
        session.status = SessionState::Running;
        session.last_updated_label = String::from("刚刚");
        session.updated_at = current_unix_millis_text();
    }
    let turn_id = format!("context-compaction-{}", current_unix_millis_text());
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_turn(
            &mut guard,
            selected_session_id,
            &turn_id,
            ActiveTurnKind::ContextCompaction,
        )?;
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

    spawn_webui_context_compaction_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime,
        selected_session_id,
        previous_phase,
        status_message_id,
        budget_note,
        MANUAL_CONTEXT_COMPACTION_TRIGGER.to_string(),
        plan,
        runtime_state_snapshot.conversation_state.clone(),
    )?;
    Ok(workbench)
}

/// Stops the selected WebUI conversation run and clears guided internal queue work.
/// 停止当前选中的 WebUI 会话运行，并清理已引导的内部队列任务。
async fn service_stop_conversation_run(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
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
    let mut workspace_snapshot = with_workspace_mutation(&app, &state, |workspace| {
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
    if did_stop_work
        && let Some(goal) = VulcanGoalStore::new(controller_runtime.clone())
            .pause_active_thread_goal(selected_session_id)
            .await?
    {
        cache_session_goal_summary(&state, selected_session_id, Some(goal))?;
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
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        spawn_service_task(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!(
                        "[webui-service] stopped conversation persist snapshot failed: {error}"
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
                    "[webui-service] stopped conversation background persist failed: {error}"
                );
            }
        });
    }
    Ok(workbench)
}

/// Stops one live command session and rebuilds the WebUI workbench snapshot.
/// 停止单条实时命令会话并重建 WebUI 工作台快照。
async fn service_stop_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let command_session_id = command_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid command session id '{command_session_id}': {error}"))?;
    let owner_session_id = match owner_session_id {
        Some(owner_session_id) => owner_session_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid owner session id '{owner_session_id}': {error}"))?,
        None => agent_runtime
            .command_session_runtime
            .try_snapshot_session_by_id(command_session_id)
            .map(|snapshot| snapshot.owner_session_id)
            .ok_or_else(|| {
                format!("Command session `{command_session_id}` was not found in the live runtime.")
            })?,
    };
    webui_trace(
        "stop_command_session_requested",
        format!("owner={owner_session_id} command_session={command_session_id}"),
    );
    agent_runtime
        .command_session_runtime
        .stop_session(owner_session_id, command_session_id, Some(1500))
        .await?;
    let workspace_snapshot =
        current_workspace_snapshot_from_shared_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    store_current_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )
}

/// Reads one live command-session snapshot without rebuilding the full WebUI workbench.
/// 读取单条实时命令会话快照，但不重建完整 WebUI 工作台。
async fn service_get_command_session_snapshot(
    state: SharedServiceBridgeState,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<CommandSessionSummary, String> {
    let (_, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let command_session_id = command_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid command session id '{command_session_id}': {error}"))?;
    let snapshot = agent_runtime
        .command_session_runtime
        .try_snapshot_session_by_id(command_session_id)
        .ok_or_else(|| {
            format!("Command session `{command_session_id}` was not found in the live runtime.")
        })?;
    if let Some(owner_session_id) = owner_session_id {
        let owner_session_id = owner_session_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid owner session id '{owner_session_id}': {error}"))?;
        if snapshot.owner_session_id != owner_session_id {
            return Err(format!(
                "Command session `{command_session_id}` does not belong to owner `{owner_session_id}`."
            ));
        }
    }
    Ok(command_session_summary_from_snapshot(snapshot))
}

/// Request body for sending structured input to one live command session.
/// 向单条实时命令会话发送结构化输入的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandSessionInputRequest {
    /// Target command session identifier.
    /// 目标命令会话标识。
    command_session_id: String,
    /// Optional owner conversation/session identifier.
    /// 可选归属会话标识。
    owner_session_id: Option<String>,
    /// Ordered terminal input events.
    /// 保序终端输入事件。
    inputs: Vec<CommandSessionInputEvent>,
}

/// Request body for resizing one live command session.
/// 调整单条实时命令会话尺寸的请求体。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandSessionResizeRequest {
    /// Target command session identifier.
    /// 目标命令会话标识。
    command_session_id: String,
    /// Optional owner conversation/session identifier.
    /// 可选归属会话标识。
    owner_session_id: Option<String>,
    /// Desired terminal row count.
    /// 期望终端行数。
    rows: u16,
    /// Desired terminal column count.
    /// 期望终端列数。
    cols: u16,
}

/// Resolves command-session ids and owner ids for service terminal actions.
/// 为服务终端动作解析命令会话标识与归属标识。
fn resolve_service_command_session_ids(
    agent_runtime: &AgentRuntimeBridge,
    command_session_id: String,
    owner_session_id: Option<String>,
) -> Result<(SessionId, SessionId), String> {
    let command_session_id = command_session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid command session id '{command_session_id}': {error}"))?;
    let owner_session_id = match owner_session_id {
        Some(owner_session_id) => owner_session_id
            .parse::<SessionId>()
            .map_err(|error| format!("invalid owner session id '{owner_session_id}': {error}"))?,
        None => agent_runtime
            .command_session_runtime
            .try_snapshot_session_by_id(command_session_id)
            .map(|snapshot| snapshot.owner_session_id)
            .ok_or_else(|| {
                format!("Command session `{command_session_id}` was not found in the live runtime.")
            })?,
    };
    Ok((command_session_id, owner_session_id))
}

/// Sends structured input to one live command session and rebuilds the WebUI snapshot.
/// 向单条实时命令会话发送结构化输入并重建 WebUI 快照。
async fn service_send_command_session_input(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionInputRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let (command_session_id, owner_session_id) = resolve_service_command_session_ids(
        &agent_runtime,
        request.command_session_id,
        request.owner_session_id,
    )?;
    agent_runtime
        .command_session_runtime
        .write_session_input_events(owner_session_id, command_session_id, request.inputs)
        .await?;
    let workspace_snapshot =
        current_workspace_snapshot_from_shared_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    store_current_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )
}

/// Resizes one live command session and rebuilds the WebUI snapshot.
/// 调整单条实时命令会话大小并重建 WebUI 快照。
async fn service_resize_command_session(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: CommandSessionResizeRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let (command_session_id, owner_session_id) = resolve_service_command_session_ids(
        &agent_runtime,
        request.command_session_id,
        request.owner_session_id,
    )?;
    agent_runtime
        .command_session_runtime
        .resize_session(
            owner_session_id,
            command_session_id,
            request.rows,
            request.cols,
        )
        .await?;
    let workspace_snapshot =
        current_workspace_snapshot_from_shared_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    store_current_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )
}

/// Saves or updates the selected session goal through the user-facing WebUI API.
/// 通过面向用户的 WebUI API 保存或更新当前会话目标。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target conversation and objective.
/// `request` 携带目标会话与目标文本。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the goal is persisted.
/// 目标持久化后的刷新工作台快照。
async fn service_save_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    validate_session_goal_objective(&request.objective)?;
    validate_session_goal_budget(request.token_budget)?;
    ensure_regular_session_exists(&state, session_id, "保存目标")?;
    ensure_session_not_in_plan_mode_for_goal(&state, session_id)?;
    let objective = request.objective.trim().to_string();
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?;
    let objective_changed = existing_goal
        .as_ref()
        .is_some_and(|goal| goal.objective != objective);
    let goal = if let Some(existing_goal) = existing_goal {
        if existing_goal.status == ThreadGoalStatus::Complete {
            goal_store
                .replace_thread_goal(
                    session_id,
                    objective.as_str(),
                    ThreadGoalStatus::Active,
                    request.token_budget,
                )
                .await
                .map_err(|error| format!("保存目标失败：{error}"))?
        } else {
            goal_store
                .update_thread_goal(
                    session_id,
                    GoalUpdate {
                        objective: Some(objective),
                        status: Some(saved_goal_status_after_edit(existing_goal.status)),
                        token_budget: Some(request.token_budget),
                        expected_goal_id: Some(existing_goal.goal_id),
                    },
                )
                .await
                .map_err(|error| format!("保存目标失败：{error}"))?
                .ok_or_else(|| String::from("当前目标已变化，请刷新后重试。"))?
        }
    } else {
        goal_store
            .replace_thread_goal(
                session_id,
                objective.as_str(),
                ThreadGoalStatus::Active,
                request.token_budget,
            )
            .await
            .map_err(|error| format!("保存目标失败：{error}"))?
    };
    if objective_changed
        && goal.status == ThreadGoalStatus::Active
        && let Err(error) = enqueue_session_goal_objective_updated_guidance(
            &state,
            session_id,
            &session_goal_summary_from_thread_goal(&goal),
        )
    {
        eprintln!("[webui-service] enqueue goal objective update guidance failed: {error}");
    }
    refresh_session_goal_summary_and_emit(
        app,
        state,
        &controller_runtime,
        &agent_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &logs,
        session_id,
        Some(goal),
    )
}

/// Returns the lifecycle status to keep after a user edits an existing goal.
/// 返回用户编辑既有目标后应保留的生命周期状态。
fn saved_goal_status_after_edit(status: ThreadGoalStatus) -> ThreadGoalStatus {
    if status == ThreadGoalStatus::Complete {
        ThreadGoalStatus::Active
    } else {
        status
    }
}

/// Resumes a stopped session goal as a fresh user-approved budget stage.
/// 将已停止的会话目标恢复为用户确认后的新预算阶段。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target session, objective, and next-stage budget.
/// `request` 携带目标会话、目标文本与下一阶段预算。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the goal resumes.
/// 目标恢复后的刷新工作台快照。
async fn service_resume_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalResumeRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    validate_session_goal_objective(&request.objective)?;
    validate_session_goal_budget(request.token_budget)?;
    ensure_regular_session_exists(&state, session_id, "继续目标")?;
    ensure_session_not_in_plan_mode_for_goal(&state, session_id)?;
    let objective = request.objective.trim().to_string();
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?
        .ok_or_else(|| String::from("当前会话还没有可继续的目标。"))?;
    if existing_goal.status == ThreadGoalStatus::Complete {
        return Err(String::from("已完成目标不能继续，请创建新目标。"));
    }
    if existing_goal.status == ThreadGoalStatus::Active {
        return Err(String::from("当前目标已经在进行中。"));
    }
    let objective_changed = existing_goal.objective != objective;
    let goal = goal_store
        .resume_thread_goal(
            session_id,
            GoalUpdate {
                objective: Some(objective),
                status: Some(ThreadGoalStatus::Active),
                token_budget: Some(request.token_budget),
                expected_goal_id: Some(existing_goal.goal_id),
            },
        )
        .await
        .map_err(|error| format!("继续目标失败：{error}"))?
        .ok_or_else(|| String::from("当前目标已变化，请刷新后重试。"))?;
    if goal.status != ThreadGoalStatus::Active {
        return Err(String::from("当前目标状态不支持继续。"));
    }
    clear_budget_limit_reported_goal_id(&state, &goal.goal_id)?;
    if objective_changed
        && let Err(error) = enqueue_session_goal_objective_updated_guidance(
            &state,
            session_id,
            &session_goal_summary_from_thread_goal(&goal),
        )
    {
        eprintln!("[webui-service] enqueue resumed goal objective update guidance failed: {error}");
    }
    let snapshot = refresh_session_goal_summary_and_emit(
        app.clone(),
        state.clone(),
        &controller_runtime,
        &agent_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &logs,
        session_id,
        Some(goal),
    )?;
    if let Err(error) = maybe_start_session_goal_continuation_if_idle(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        session_id,
    )
    .await
    {
        eprintln!("[webui-service] resumed goal continuation failed: {error}");
    }
    Ok(snapshot)
}

/// Clears the selected session goal through the user-facing WebUI API.
/// 通过面向用户的 WebUI API 清理当前会话目标。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target conversation id.
/// `request` 携带目标会话标识。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the goal is removed.
/// 目标移除后的刷新工作台快照。
async fn service_clear_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalClearRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    ensure_regular_session_exists(&state, session_id, "清理目标")?;
    let removed_goal = VulcanGoalStore::new(controller_runtime.clone())
        .delete_thread_goal(session_id)
        .await
        .map_err(|error| format!("清理目标失败：{error}"))?;
    if let Some(removed_goal) = removed_goal.as_ref() {
        clear_budget_limit_reported_goal_id(&state, &removed_goal.goal_id)?;
    }
    refresh_session_goal_summary_and_emit(
        app,
        state,
        &controller_runtime,
        &agent_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &logs,
        session_id,
        None,
    )
}

/// Updates the selected session goal status through the user-facing WebUI API.
/// 通过面向用户的 WebUI API 更新当前会话目标状态。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target conversation id and next status.
/// `request` 携带目标会话标识与下一状态。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the goal status changes.
/// 目标状态变更后的刷新工作台快照。
async fn service_update_session_goal_status(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalStatusUpdateRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    ensure_regular_session_exists(&state, session_id, "更新目标状态")?;
    let status = parse_session_goal_status(&request.status)?;
    let goal = VulcanGoalStore::new(controller_runtime.clone())
        .update_thread_goal(
            session_id,
            GoalUpdate {
                objective: None,
                status: Some(status),
                token_budget: None,
                expected_goal_id: None,
            },
        )
        .await
        .map_err(|error| format!("更新目标状态失败：{error}"))?
        .ok_or_else(|| String::from("当前会话还没有可更新的目标。"))?;
    refresh_session_goal_summary_and_emit(
        app,
        state,
        &controller_runtime,
        &agent_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &logs,
        session_id,
        Some(goal),
    )
}

/// Opens one subagent transcript surface under the selected parent conversation.
/// 打开当前父会话下的单个子代理转录界面。
async fn service_open_subagent_panel(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
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
        emit_workbench_event(&app, &workbench);
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
    let snapshot = emit_subagent_display_route_workbench_snapshot(&app, &state, workspace_snapshot);
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
async fn service_close_subagent_panel(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
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
    emit_subagent_display_route_workbench_snapshot(&app, &state, workspace_snapshot)
}

/// Stops one running subagent task through the shared task runtime.
/// 通过共享任务运行时停止单个运行中的子代理任务。
async fn service_stop_subagent_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
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
        with_workspace_mutation(&app, &state, |workspace| {
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
        &state,
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )?;
    if !stopped_parent_session_ids.is_empty() {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        spawn_service_task(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[webui-service] stopped subagent persist snapshot failed: {error}");
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
                        "[webui-service] stopped subagent background persist failed: {error}"
                    );
                }
            }
        });
    }
    Ok(workbench)
}

/// Selects and persists the active workbench provider/model pair.
/// 选择并持久化当前工作台使用的供应商与模型组合。
/// Saves the current session AI capability quick-panel draft into the session addon document.
/// 将当前会话 AI 能力快捷面板草稿保存到会话附属状态文档。
async fn service_save_session_ai_capability_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionAiCapabilityConfigurationSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能保存会话级 AI 能力配置。"))?;
    let requested_project_id = request
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let disabled_kinds =
        parse_session_ai_capability_disabled_kinds(request.disabled_ai_capability_kinds)?;
    let disabled_target_selectors =
        parse_ai_capability_target_selectors(Some(request.disabled_ai_capability_target_selectors));
    let target_order_overrides = parse_ai_capability_target_order_overrides(Some(
        request.ai_capability_target_order_overrides,
    ))?;
    let (controller_runtime, workspace_snapshot, provider_configuration, previous_configuration) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let default_ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要保存 AI 能力配置的会话：{session_id}"));
        }
        if let Some(project_id) = requested_project_id
            && owning_project_id_for_session(workspace_snapshot, session_id).as_deref()
                != Some(project_id)
        {
            return Err(String::from(
                "当前会话与请求项目不匹配，已拒绝保存 AI 能力配置。",
            ));
        }
        let previous_configuration = session_ai_tool_configuration_or_default(
            workspace_snapshot,
            session_id,
            &default_ai_tool_configuration,
        )
        .clone();
        (
            controller_runtime,
            workspace_snapshot.clone(),
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.provider_configuration.clone())
                .unwrap_or_default(),
            previous_configuration,
        )
    };
    let mut next_configuration = previous_configuration.clone();
    apply_session_ai_capability_dialog_draft(
        &mut next_configuration,
        &disabled_kinds,
        &disabled_target_selectors,
        &target_order_overrides,
    )?;
    let model_capabilities = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| {
            runtime_state_base_model_capabilities(runtime_state, &provider_configuration)
        })
        .or_else(|| active_provider_configuration_model_capabilities(&provider_configuration));
    let capability_update_injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        model_capabilities.as_ref(),
    );
    let updated_at = current_unix_millis_text();
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_ai_tool_configuration(&next_configuration, updated_at.clone());
    invalidate_webui_session_addon_persist_version(
        &state,
        session_id,
        SessionAddonNamespace::AiCapabilities,
    )?;
    let addon_persist_lock = webui_workspace_persist_lock();
    let _addon_persist_guard = addon_persist_lock.lock().await;
    let persisted_addon_state = persist_workspace_session_addon_state_update(
        controller_runtime,
        workspace_snapshot,
        session_id,
        addon_state,
    )
    .await?;
    invalidate_runtime_prompt_tool_visibility_cache();
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(&app, &state, |workspace| {
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: updated_at.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            runtime_state.ai_tool_configuration = Some(next_configuration.clone());
            if let Some(injection) = capability_update_injection.clone() {
                runtime_state
                    .context_injection_state
                    .apply_insert(injection, &updated_at);
            }
            runtime_state.updated_at = updated_at.clone();
        })?;
    emit_session_addon_changed_event(
        &app,
        session_id,
        &[SessionAddonNamespace::AiCapabilities],
        persisted_addon_state.revision,
        latest_session_addon_updated_at(&persisted_addon_state),
    );
    let _ = workspace_snapshot;
    current_workbench_snapshot_from_state(&state)
}

/// Updates the selected session Plan mode state through the WebUI API.
/// 通过 WebUI API 更新当前会话的计划模式状态。
///
/// # Parameters
/// 参数
/// `app` emits addon-change notifications after persistence.
/// `app` 在持久化后发出附属状态变更通知。
/// `state` owns the retained workbench snapshot.
/// `state` 持有保留的工作台快照。
/// `request` carries the target conversation and desired Plan enablement.
/// `request` 携带目标会话与期望的计划模式启停状态。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the Plan mode state changes.
/// 计划模式状态变化后的刷新工作台快照。
async fn service_set_session_plan_mode(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionPlanModeUpdateRequest,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能切换计划模式。"))?;
    let (controller_runtime, workspace_snapshot, existing_plan_mode) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换计划模式的会话：{session_id}"));
        }
        let existing_plan_mode = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.plan_mode.clone())
            .unwrap_or_default();
        (
            controller_runtime,
            workspace_snapshot.clone(),
            existing_plan_mode,
        )
    };
    let updated_at = current_unix_millis_text();
    let mut next_plan_mode = existing_plan_mode;
    if request.enabled {
        ensure_session_can_enter_plan_mode(&controller_runtime, session_id).await?;
        next_plan_mode.pending_execution_notice = false;
    }
    if !request.enabled {
        next_plan_mode.pending_execution_notice = next_plan_mode.prompt_injected;
        next_plan_mode.prompt_injected = false;
    }
    next_plan_mode.artifact = None;
    let work_mode = SessionWorkMode {
        execution_mode: if request.enabled {
            String::from("plan")
        } else {
            String::from("build")
        },
        plan_mode: next_plan_mode.clone(),
    };
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_work_mode(&work_mode, updated_at.clone());
    invalidate_webui_session_addon_persist_version(
        &state,
        session_id,
        SessionAddonNamespace::WorkMode,
    )?;
    let addon_persist_lock = webui_workspace_persist_lock();
    let _addon_persist_guard = addon_persist_lock.lock().await;
    let persisted_addon_state = persist_workspace_session_addon_state_update(
        controller_runtime,
        workspace_snapshot,
        session_id,
        addon_state,
    )
    .await?;
    with_workspace_mutation_preserving_subagent_navigation(&app, &state, |workspace| {
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: updated_at.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        work_mode.apply_to_runtime_state(runtime_state);
        runtime_state.updated_at = updated_at.clone();
    })?;
    emit_session_addon_changed_event(
        &app,
        session_id,
        &[SessionAddonNamespace::WorkMode],
        persisted_addon_state.revision,
        latest_session_addon_updated_at(&persisted_addon_state),
    );
    current_workbench_snapshot_from_state(&state)
}

/// Builds one context-window-scoped model notice for session AI capability changes.
/// 为会话 AI 能力变化构建一条仅在当前上下文窗口内生效的模型通知。
///
/// # Parameters
/// 参数
/// `previous_configuration` is the session configuration before the save operation.
/// `previous_configuration` 是保存操作前的会话配置。
/// `next_configuration` is the session configuration after the save operation.
/// `next_configuration` 是保存操作后的会话配置。
/// `model_capabilities` describes native capability support for the active model.
/// `model_capabilities` 描述当前模型的原生能力支持。
///
/// # Returns
/// 返回值
/// A persistent hidden-user injection that clears on context compaction, or `None` when nothing model-visible changed.
/// 返回一条上下文压缩时清除的持久隐藏用户注入；若无模型可见变化则返回 `None`。
fn session_ai_capability_configuration_update_injection(
    previous_configuration: &PersistedAiToolConfiguration,
    next_configuration: &PersistedAiToolConfiguration,
    model_capabilities: Option<&ResolvedModelCapabilities>,
) -> Option<InjectionInsert> {
    let previous_capabilities = ai_capabilities(previous_configuration, model_capabilities);
    let next_capabilities = ai_capabilities(next_configuration, model_capabilities);
    let mut changes = Vec::new();
    for kind in WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS {
        let Some(previous) = previous_capabilities
            .iter()
            .find(|capability| capability.kind == kind.key())
        else {
            continue;
        };
        let Some(next) = next_capabilities
            .iter()
            .find(|capability| capability.kind == kind.key())
        else {
            continue;
        };
        if ai_capability_has_native_path(kind, model_capabilities) {
            continue;
        }
        let previous_default = ai_capability_default_route(previous_configuration, kind);
        let next_default = ai_capability_default_route(next_configuration, kind);
        let mut details = Vec::new();
        if previous.enabled != next.enabled {
            let state = if next.enabled { "ENABLED" } else { "DISABLED" };
            details.push(format!(
                "state changed: {state}; native={}, bridged={}",
                next.native, next.bridged
            ));
        }
        if previous_default != next_default {
            details.push(format!(
                "default route changed: {} -> {}",
                format_ai_capability_default_route(previous_default.as_ref()),
                format_ai_capability_default_route(next_default.as_ref())
            ));
        }
        if !details.is_empty() {
            changes.push(format!(
                "- `{}` ({}): {}",
                next.kind,
                next.label,
                details.join("; ")
            ));
        }
    }
    if changes.is_empty() {
        return None;
    }
    let content = format!(
        "Runtime AI extension capability update for this session.\n\
         Treat this block as newer than any earlier conversation history about available tools.\n\
         The following session AI extension capability changes were just saved:\n{}\n\n\
         Operational rule: rely on the currently registered tool schema and this latest runtime update. \
         If a capability is listed as DISABLED or no matching tool is registered in the tool schema, do not claim you can call it. \
         This notice is persistent only for the current context window and must be forgotten after context compaction.",
        changes.join("\n")
    );
    let mut item = InjectionInsertItem::new(content);
    item.dedupe_key = Some(SESSION_AI_CAPABILITY_UPDATE_DEDUPE_KEY.to_string());
    item.compaction_policy = Some(InjectionCompactionPolicy::Clear);
    let mut insert = InjectionInsert::single(
        InjectionKind::Persistent,
        SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG,
        "Latest saved session AI extension capability changes. This notice clears on context compaction.",
        String::new(),
        None,
    );
    insert.items = vec![item];
    insert.compaction_policy = InjectionCompactionPolicy::Clear;
    insert.source_display = Some(String::from("Session AI capabilities"));
    insert.priority = -35;
    Some(insert)
}

/// Returns the default route selected by the first enabled target for one capability.
/// 返回单个能力由首个已启用目标决定的默认路由。
///
/// # Parameters
/// 参数
/// `configuration` is the AI tool configuration to inspect.
/// `configuration` 是需要检查的 AI 工具配置。
/// `kind` selects the semantic capability.
/// `kind` 选择语义能力。
///
/// # Returns
/// 返回值
/// The first enabled route target and model id when a host route exists.
/// 存在宿主路由时返回首个已启用路由目标及模型标识。
fn ai_capability_default_route(
    configuration: &PersistedAiToolConfiguration,
    kind: AiToolKind,
) -> Option<(String, String)> {
    configuration
        .resolved_route_targets_for_kind(kind)
        .into_iter()
        .next()
        .map(|target| (target.target_selector, target.model_id))
}

/// Formats one optional AI capability default route for model-facing notices.
/// 为面向模型的通知格式化单个可选 AI 能力默认路由。
///
/// # Parameters
/// 参数
/// `route` is the optional default route tuple.
/// `route` 是可选的默认路由元组。
///
/// # Returns
/// 返回值
/// A compact stable route label.
/// 返回紧凑稳定的路由标签。
fn format_ai_capability_default_route(route: Option<&(String, String)>) -> String {
    route
        .map(|(target, model)| format!("`{target}` using model `{model}`"))
        .unwrap_or_else(|| String::from("none"))
}

/// Persists one complete session addon document after the caller enters the addon write queue.
/// 调用方进入附属状态写入队列后，持久化单个完整会话附属状态文档。
async fn persist_workspace_session_addon_state_update(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    addon_state: SessionAddonState,
) -> Result<SessionAddonState, String> {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return Err(format!("未找到要保存附属状态的会话：{session_id}"));
    }
    let Some((session, archived)) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| (session.clone(), false))
        .or_else(|| {
            workspace_snapshot
                .archived_sessions
                .iter()
                .find(|session| session.id == session_id)
                .map(|session| (session.clone(), true))
        })
    else {
        return Err(format!("未找到要保存附属状态的会话：{session_id}"));
    };
    let task_projects = workspace_snapshot
        .task_projects
        .iter()
        .cloned()
        .chain(workspace_snapshot.archived_task_projects.iter().cloned())
        .collect::<Vec<_>>();
    vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update(
        controller_runtime,
        &task_projects,
        &session,
        archived,
        &addon_state,
    )
    .await
    .map_err(|error| format!("会话附属状态保存失败：{error}"))
}

/// Parses the complete disabled capability list from the WebUI session dialog.
/// 解析 WebUI 会话弹窗中的完整禁用能力列表。
fn parse_session_ai_capability_disabled_kinds(
    raw_kinds: Vec<String>,
) -> Result<BTreeSet<AiToolKind>, String> {
    let mut disabled_kinds = BTreeSet::new();
    for raw_kind in raw_kinds {
        let capability_kind = parse_ai_tool_kind(&raw_kind)?;
        if capability_kind.supports_workbench_quick_toggle() {
            disabled_kinds.insert(capability_kind);
        }
    }
    Ok(disabled_kinds)
}

/// Applies one WebUI session dialog draft to a persisted AI capability configuration.
/// 将 WebUI 会话弹窗草稿应用到持久化 AI 能力配置。
fn apply_session_ai_capability_dialog_draft(
    configuration: &mut PersistedAiToolConfiguration,
    disabled_kinds: &BTreeSet<AiToolKind>,
    disabled_target_selectors: &BTreeSet<String>,
    target_order_overrides: &BTreeMap<AiToolKind, Vec<String>>,
) -> Result<(), String> {
    configuration.disabled_capability_kinds = disabled_kinds.clone();
    for vendor in &mut configuration.vendors {
        let vendor_id = vendor.vendor_id.trim().to_ascii_lowercase();
        for binding in &mut vendor.targets {
            if !binding.capability_kind.supports_workbench_quick_toggle() {
                continue;
            }
            let selector = format!("{}/{}", vendor_id, binding.target_alias());
            binding.enabled = !disabled_target_selectors
                .iter()
                .any(|disabled| disabled.eq_ignore_ascii_case(&selector));
        }
    }
    for (capability_kind, target_order) in target_order_overrides {
        set_ai_capability_target_order_for_kind(
            configuration,
            *capability_kind,
            target_order.clone(),
        )?;
    }
    configuration.normalize();
    Ok(())
}

/// Saves one MCP server configuration from the WebUI settings page.
/// 从 WebUI 设置页保存单个 MCP 服务器配置。
async fn service_save_mcp_server_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: McpServerSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let (server_id, entry) = mcp_server_entry_from_request(request)?;
    let next_configuration = {
        let mut configuration = provider_configuration_from_state(&state)?;
        configuration.mcp.servers.insert(server_id.clone(), entry);
        provider_config::save_configuration_file(&configuration)
            .map_err(|error| format!("保存 MCP 服务器失败：{error}"))?;
        configuration
    };
    let next_mcp_runtime = bootstrap_mcp_runtime_access(next_configuration.clone())
        .await
        .ok();
    let workbench = store_provider_and_mcp_configuration_after_change(
        &state,
        next_configuration,
        next_mcp_runtime,
    )?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Removes one MCP server configuration from the WebUI settings page.
/// 从 WebUI 设置页移除单个 MCP 服务器配置。
async fn service_remove_mcp_server_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    server_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let mut next_configuration = provider_configuration_from_state(&state)?;
    next_configuration
        .mcp
        .servers
        .remove(server_id.trim())
        .ok_or_else(|| format!("未找到 MCP 服务器 `{}`。", server_id.trim()))?;
    provider_config::save_configuration_file(&next_configuration)
        .map_err(|error| format!("移除 MCP 服务器失败：{error}"))?;
    let next_mcp_runtime = bootstrap_mcp_runtime_access(next_configuration.clone())
        .await
        .ok();
    let workbench = store_provider_and_mcp_configuration_after_change(
        &state,
        next_configuration,
        next_mcp_runtime,
    )?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}

/// Opens one project folder in the host system file manager.
/// 在宿主系统文件管理器中打开单个项目文件夹。
fn service_open_project_directory(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<String, String> {
    let shell_host = shell_host_from_state(&state)?;
    let project = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    shell_host.open_path(&project.path)?;
    Ok(format!("已请求打开项目目录：{}。", project.name))
}

/// Opens one arbitrary project directory path in the host system file manager.
/// 在宿主系统文件管理器中打开任意项目目录路径。
fn service_open_project_directory_path(
    state: SharedServiceBridgeState,
    path: String,
) -> Result<String, String> {
    shell_host_from_state(&state)?.open_path(&path)?;
    Ok(format!("已请求打开目录：{path}。"))
}

/// Returns the best initial directory for the native project folder picker.
/// 返回原生项目文件夹选择器使用的最佳初始目录。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns the current workbench snapshot.
/// `state` 是持有当前工作台快照的共享服务状态。
///
/// # Returns
/// 返回值
/// A normalized directory path string suitable for the host folder picker.
/// 返回适合宿主文件夹选择器使用的标准化目录路径字符串。
pub fn service_add_project_initial_directory(
    state: &SharedServiceBridgeState,
) -> Result<String, String> {
    Ok(state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .projects
        .first()
        .map(|project| project.path.clone())
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())))
}

/// Toggles one project pinned state without rebuilding the full workbench snapshot.
/// 切换单个项目置顶状态，但不重建完整工作台快照。
async fn service_toggle_project_pinned(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<(), String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.clone()),
            ]],
        )
        .await
        .map_err(|error| format!("切换项目置顶状态失败：{error}"))?;
    update_project_pinned_local_state(&state, &project_id)?;
    Ok(())
}

/// Reorders visible projects inside one pinned or regular section.
/// 在同一个置顶或普通分区内重排可见项目。
async fn service_reorder_projects(
    state: SharedServiceBridgeState,
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
async fn service_rename_project(
    state: SharedServiceBridgeState,
    project_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    vulcan_workspace_store::workspace_store::rename_project(controller_runtime, &project_id, &name)
        .await?;
    refresh_workbench_snapshot(state).await
}

/// Applies a project icon override without rebuilding the full workbench snapshot.
/// 应用项目图标覆盖，但不重建完整工作台快照。
async fn service_set_project_icon(
    state: SharedServiceBridgeState,
    project_id: String,
    icon_pack: String,
    icon_key: String,
) -> Result<(), String> {
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
                ControllerSqliteValue::String(normalized_icon_pack.clone()),
                ControllerSqliteValue::String(normalized_icon_key.clone()),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.clone()),
            ]],
        )
        .await
        .map_err(|error| format!("设置项目图标失败：{error}"))?;
    update_project_icon_local_state(
        &state,
        &project_id,
        Some(normalized_icon_pack),
        Some(normalized_icon_key),
    )?;
    Ok(())
}

/// Clears a project icon override without rebuilding the full workbench snapshot.
/// 清除项目图标覆盖，但不重建完整工作台快照。
async fn service_reset_project_icon(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<(), String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE projects SET icon_pack = NULL, icon_key = NULL, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.clone()),
            ]],
        )
        .await
        .map_err(|error| format!("还原项目图标失败：{error}"))?;
    update_project_icon_local_state(&state, &project_id, None, None)?;
    Ok(())
}

/// Persists the expanded project identifiers and returns the retained workbench snapshot.
/// 持久化展开项目标识集合并返回保留的工作台快照。
async fn service_set_expanded_task_projects(
    state: SharedServiceBridgeState,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, collapsed_project_ids, expanded_project_ids) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let visible_project_ids = guard
            .workbench
            .projects
            .iter()
            .map(|project| project.id.clone())
            .collect::<BTreeSet<_>>();
        let expanded_project_ids = project_ids
            .into_iter()
            .filter(|project_id| visible_project_ids.contains(project_id))
            .collect::<BTreeSet<_>>();
        let collapsed_project_ids = visible_project_ids
            .iter()
            .filter(|project_id| !expanded_project_ids.contains(*project_id))
            .cloned()
            .collect::<Vec<_>>();
        (
            controller_runtime,
            collapsed_project_ids,
            expanded_project_ids,
        )
    };
    vulcan_workspace_store::workspace_store::persist_collapsed_task_project_ids(
        controller_runtime,
        &collapsed_project_ids,
    )
    .await?;

    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot.expanded_task_projects = expanded_project_ids.clone();
    }
    guard.workbench.expanded_project_ids = guard
        .workbench
        .projects
        .iter()
        .filter(|project| expanded_project_ids.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    Ok(guard.workbench.clone())
}

/// Archives all visible conversations under one project and keeps the project active.
/// 归档单个项目下的全部可见对话，并保留项目本身。
async fn service_archive_project_conversations(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, project, project_sessions) =
        controller_runtime_and_active_project_from_state(&state, &project_id, "归档")?;

    if project.session_ids.is_empty() {
        return retained_workbench_snapshot_from_state(&state);
    }

    vulcan_workspace_store::workspace_store::archive_project_sessions(
        controller_runtime,
        &project_id,
        &project_sessions,
    )
    .await?;
    archive_project_sessions_in_retained_state(&state, &project_id, &project_sessions, false)
}

/// Archives all project conversations, removes the active project row, and refreshes the snapshot.
/// 归档项目全部对话、移除活动项目记录，并刷新工作台快照。
async fn service_remove_project(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, project, project_sessions) =
        controller_runtime_and_active_project_from_state(&state, &project_id, "移除")?;

    vulcan_workspace_store::workspace_store::remove_project_after_archiving_sessions(
        controller_runtime,
        &project_id,
        &project_sessions,
    )
    .await?;
    if project.session_ids.is_empty() {
        return remove_empty_project_from_retained_state(&state, &project_id);
    }
    archive_project_sessions_in_retained_state(&state, &project_id, &project_sessions, true)
}

/// Returns the controller runtime and one retained active project without reloading the workspace.
/// 返回控制器运行时与一个保留的活动项目，不重新加载工作区。
fn controller_runtime_and_active_project_from_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    action_label: &str,
) -> Result<
    (
        ControllerRuntimeHandle,
        TaskProjectSummary,
        Vec<SessionSummary>,
    ),
    String,
> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区快照尚未加载，无法处理项目。"))?;
    let project = workspace_snapshot
        .task_projects
        .iter()
        .find(|project| project.id == project_id)
        .cloned()
        .ok_or_else(|| format!("未找到要{action_label}的项目：{project_id}"))?;
    let project_sessions = project
        .session_ids
        .iter()
        .filter_map(|session_id| {
            workspace_snapshot
                .sessions
                .iter()
                .find(|session| session.id == *session_id)
                .cloned()
        })
        .collect::<Vec<_>>();
    Ok((controller_runtime, project, project_sessions))
}

/// Returns the retained workbench snapshot after a no-op project archive command.
/// 在项目归档命令无需改动时返回保留的工作台快照。
fn retained_workbench_snapshot_from_state(
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard.workbench.clone())
}

/// Removes an empty active project from retained workspace and workbench state.
/// 从保留的工作区与工作台状态中移除一个空活动项目。
fn remove_empty_project_from_retained_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot
            .task_projects
            .retain(|project| project.id != project_id);
        workspace_snapshot.expanded_task_projects.remove(project_id);
    }
    guard
        .workbench
        .projects
        .retain(|project| project.id != project_id);
    guard
        .workbench
        .expanded_project_ids
        .retain(|candidate| candidate != project_id);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    Ok(guard.workbench.clone())
}

/// Moves project sessions between retained active and archived metadata without reloading catalogs.
/// 在保留内存状态中迁移项目会话到归档元数据，不重新加载目录目录表。
fn archive_project_sessions_in_retained_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    sessions: &[SessionSummary],
    remove_project: bool,
) -> Result<WorkbenchSnapshot, String> {
    let session_ids = sessions
        .iter()
        .map(|session| session.id)
        .collect::<BTreeSet<_>>();
    let conversation_ids = session_ids
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    let mut archived_sessions = sessions
        .iter()
        .cloned()
        .map(|mut session| {
            if session.project_id.is_none() {
                session.project_id = Some(project_id.to_string());
            }
            session
        })
        .collect::<Vec<_>>();
    archived_sessions.sort_by_key(|session| -session.updated_at.parse::<i64>().unwrap_or_default());

    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot
            .sessions
            .retain(|session| !session_ids.contains(&session.id));
        workspace_snapshot
            .archived_sessions
            .retain(|session| !session_ids.contains(&session.id));
        workspace_snapshot
            .archived_sessions
            .extend(archived_sessions.clone());
        workspace_snapshot
            .archived_sessions
            .sort_by_key(|session| -session.updated_at.parse::<i64>().unwrap_or_default());
        for project in &mut workspace_snapshot.task_projects {
            project
                .session_ids
                .retain(|session_id| !session_ids.contains(session_id));
        }
        for session_id in &session_ids {
            workspace_snapshot.session_messages.remove(session_id);
            workspace_snapshot.session_compactions.remove(session_id);
            workspace_snapshot.session_runtime_state.remove(session_id);
            workspace_snapshot.session_run_phases.remove(session_id);
            workspace_snapshot.hydrated_session_ids.remove(session_id);
        }
        if remove_project {
            workspace_snapshot
                .task_projects
                .retain(|project| project.id != project_id);
            workspace_snapshot.expanded_task_projects.remove(project_id);
        }
        if session_ids.contains(&workspace_snapshot.selected_session) {
            workspace_snapshot.selected_session = workspace_snapshot
                .sessions
                .first()
                .map(|session| session.id)
                .unwrap_or_else(SessionId::nil);
        }
    }

    guard
        .workbench
        .conversations
        .retain(|conversation| !conversation_ids.contains(&conversation.id));
    guard
        .workbench
        .messages
        .retain(|message| !conversation_ids.contains(&message.conversation_id));
    guard
        .workbench
        .archived_conversations
        .retain(|conversation| !conversation_ids.contains(&conversation.id));
    if remove_project {
        guard
            .workbench
            .projects
            .retain(|project| project.id != project_id);
        guard
            .workbench
            .expanded_project_ids
            .retain(|candidate| candidate != project_id);
    } else {
        guard.workbench.archived_conversations.extend(
            archived_sessions
                .iter()
                .map(archived_conversation_summary_from_session),
        );
        guard
            .workbench
            .archived_conversations
            .sort_by_key(|conversation| {
                -conversation
                    .last_updated_at
                    .parse::<i64>()
                    .unwrap_or_default()
            });
    }
    if guard
        .workbench
        .selected_conversation_id
        .as_ref()
        .is_some_and(|selected| conversation_ids.contains(selected))
    {
        guard.workbench.selected_conversation_id = guard
            .workbench
            .conversations
            .first()
            .map(|conversation| conversation.id.clone());
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    Ok(guard.workbench.clone())
}

/// Applies a conversation display name and returns the refreshed workbench snapshot.
/// 应用会话显示名称并返回刷新后的工作台快照。
async fn service_rename_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let normalized_name = normalize_non_empty_name(&name)?;
    let parsed_session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let updated_at = current_unix_millis_text();
    let (controller_runtime, project_id, session) =
        controller_runtime_and_visible_session_project_from_state(&state, parsed_session_id)?;
    vulcan_workspace_store::workspace_store::rename_single_session_metadata(
        controller_runtime,
        &project_id,
        &session,
        &normalized_name,
        &updated_at,
    )
    .await
    .map_err(|error| format!("重命名会话失败：{error}"))?;
    rename_conversation_in_retained_state(&state, parsed_session_id, &normalized_name, &updated_at)
}

/// Renames one retained visible conversation without reloading the workspace snapshot.
/// 不重新加载工作区快照，重命名一个保留的可见会话。
fn rename_conversation_in_retained_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    title: &str,
    updated_at: &str,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut()
        && let Some(session) = workspace_snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
    {
        session.title = title.to_string();
        session.title_source = SessionTitleSource::Manual;
        session.updated_at = updated_at.to_string();
        session.last_updated_label = String::from("刚刚");
    }
    let conversation_id = session_id.to_string();
    if let Some(conversation) = guard
        .workbench
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
    {
        conversation.title = title.to_string();
        conversation.last_updated_at = updated_at.to_string();
        conversation.last_updated_label = String::from("刚刚");
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    Ok(guard.workbench.clone())
}

/// Archives one visible conversation and returns the refreshed workbench snapshot.
/// 归档单个可见会话并返回刷新后的工作台快照。
async fn service_archive_conversation(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (controller_runtime, project_id, session) =
        controller_runtime_and_visible_session_project_from_state(&state, session_id)?;
    vulcan_workspace_store::workspace_store::archive_single_session_for_project(
        controller_runtime,
        &project_id,
        &session,
    )
    .await?;
    archive_conversation_in_retained_state(&state, session_id, &project_id)
}

/// Returns the controller runtime and owning project id for one retained visible session.
/// 返回单个保留可见会话的控制器运行时与所属项目标识。
fn controller_runtime_and_visible_session_project_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(ControllerRuntimeHandle, String, SessionSummary), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; controller runtime is not ready")
    })?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区快照尚未加载，无法归档会话。"))?;
    let session = workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
        .cloned()
        .ok_or_else(|| format!("未找到要归档的会话：{session_id}"))?;
    let project_id = session
        .project_id
        .clone()
        .or_else(|| owning_project_id_for_session(workspace_snapshot, session_id))
        .ok_or_else(|| format!("会话缺少所属项目，无法归档：{session_id}"))?;
    Ok((controller_runtime, project_id, session))
}

/// Moves one archived conversation from retained active state into archived metadata state.
/// 将一个已归档会话从保留的活动状态移动到归档元数据状态。
fn archive_conversation_in_retained_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    project_id: &str,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut archived_session = {
        let workspace_snapshot = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("工作区快照尚未加载，无法更新归档状态。"))?;
        let session_index = workspace_snapshot
            .sessions
            .iter()
            .position(|candidate| candidate.id == session_id)
            .ok_or_else(|| format!("未找到要归档的会话：{session_id}"))?;
        let mut archived_session = workspace_snapshot.sessions.remove(session_index);
        if archived_session.project_id.is_none() {
            archived_session.project_id = Some(project_id.to_string());
        }
        for project in &mut workspace_snapshot.task_projects {
            project
                .session_ids
                .retain(|candidate| *candidate != session_id);
        }
        workspace_snapshot.session_messages.remove(&session_id);
        workspace_snapshot.session_compactions.remove(&session_id);
        workspace_snapshot.session_runtime_state.remove(&session_id);
        workspace_snapshot.session_run_phases.remove(&session_id);
        workspace_snapshot.hydrated_session_ids.remove(&session_id);
        workspace_snapshot
            .archived_sessions
            .retain(|candidate| candidate.id != session_id);
        workspace_snapshot
            .archived_sessions
            .push(archived_session.clone());
        workspace_snapshot
            .archived_sessions
            .sort_by_key(|session| -session.updated_at.parse::<i64>().unwrap_or_default());
        if workspace_snapshot.selected_session == session_id {
            workspace_snapshot.selected_session = workspace_snapshot
                .sessions
                .first()
                .map(|session| session.id)
                .unwrap_or_else(SessionId::nil);
        }
        archived_session
    };
    if archived_session.project_id.is_none() {
        archived_session.project_id = Some(project_id.to_string());
    }
    let archived_conversation = archived_conversation_summary_from_session(&archived_session);
    let archived_conversation_id = session_id.to_string();
    guard
        .workbench
        .conversations
        .retain(|conversation| conversation.id != archived_conversation_id);
    guard
        .workbench
        .messages
        .retain(|message| message.conversation_id != archived_conversation_id);
    guard
        .workbench
        .archived_conversations
        .retain(|conversation| conversation.id != archived_conversation_id);
    guard
        .workbench
        .archived_conversations
        .push(archived_conversation);
    guard
        .workbench
        .archived_conversations
        .sort_by_key(|conversation| {
            -conversation
                .last_updated_at
                .parse::<i64>()
                .unwrap_or_default()
        });
    if guard.workbench.selected_conversation_id.as_deref() == Some(&archived_conversation_id) {
        guard.workbench.selected_conversation_id = guard
            .workbench
            .conversations
            .first()
            .map(|conversation| conversation.id.clone());
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    Ok(guard.workbench.clone())
}

/// Builds a WebUI archived-conversation summary from persisted session metadata.
/// 根据持久化会话元数据构建 WebUI 归档会话摘要。
fn archived_conversation_summary_from_session(session: &SessionSummary) -> ConversationSummary {
    ConversationSummary {
        id: session.id.to_string(),
        title: session.title.clone(),
        status: String::from("Archived"),
        run_phase: SessionRunPhase::Idle.to_string(),
        last_updated_label: session.last_updated_label.clone(),
        last_updated_at: session.updated_at.clone(),
        unread: false,
        project_id: session.project_id.clone(),
        fork_source: conversation_fork_source_summary_from_session(session),
    }
}

/// Marks one conversation as read and returns the retained workbench snapshot.
/// 将单个会话标记为已读并返回保留的工作台快照。
async fn service_mark_conversation_read(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = conversation_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{conversation_id}': {error}"))?;
    let (controller_runtime, runtime_state_update, workbench) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let mut runtime_state_update = None;
        if let Some(runtime_state) =
            guard
                .workspace_snapshot
                .as_mut()
                .and_then(|workspace_snapshot| {
                    workspace_snapshot
                        .session_runtime_state
                        .get_mut(&session_id)
                })
        {
            runtime_state.ui_state.unread = false;
            runtime_state.updated_at = current_unix_millis_text();
            runtime_state_update = Some(runtime_state.clone());
        }
        for conversation in guard
            .workbench
            .conversations
            .iter_mut()
            .filter(|conversation| conversation.id == conversation_id)
        {
            conversation.unread = false;
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        (
            controller_runtime,
            runtime_state_update,
            guard.workbench.clone(),
        )
    };

    if let Some(runtime_state_update) = runtime_state_update {
        vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
            controller_runtime,
            session_id,
            &runtime_state_update,
        )
        .await?;
    }
    Ok(workbench)
}

/// Selects one conversation in the retained WebUI workspace state.
/// 在 WebUI 保留的工作区状态中选中一个会话。
async fn service_select_conversation(
    state: SharedServiceBridgeState,
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
        mut provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        pending_question,
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
        let pending_question =
            pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            provider_configuration,
            ai_tool_configuration,
            live_context_budgets,
            pending_question,
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
    hydrate_selected_session_runtime_if_needed(
        controller_runtime.clone(),
        &mut workspace_snapshot,
        session_id,
    )
    .await?;
    restore_webui_runtime_controls_for_session(
        &mut provider_configuration,
        workspace_snapshot.session_runtime_state.get(&session_id),
    );
    sync_selected_session_goal_summary_from_store(&state, controller_runtime.clone(), session_id)
        .await?;
    let (
        surface_messages,
        subagent_navigation,
        session_goal_summaries,
        command_session_runtime,
        hydrated_window_summaries,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.active_subagent_panel = None;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
        if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
            agent_runtime.provider_configuration = provider_configuration.clone();
        }
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            guard.session_goal_summaries.clone(),
            command_session_runtime,
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        command_session_runtime.as_deref(),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question,
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
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
    state: &SharedServiceBridgeState,
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
fn selected_session_id_from_state(state: &SharedServiceBridgeState) -> Result<SessionId, String> {
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
    state: &SharedServiceBridgeState,
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
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench.clone())
}

/// Applies one pinned-state mutation to retained project snapshots.
/// 将单个置顶状态变更应用到保留的项目快照。
fn update_project_pinned_local_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut found = false;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        for project in &mut workspace_snapshot.task_projects {
            if project.id == project_id {
                project.pinned = !project.pinned;
                found = true;
            }
        }
    }
    for project in &mut guard.workbench.projects {
        if project.id == project_id {
            project.pinned = !project.pinned;
            found = true;
        }
    }
    if found {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        Ok(())
    } else {
        Err(format!("没有找到要切换置顶状态的项目：{project_id}"))
    }
}

/// Applies one icon mutation to retained project snapshots.
/// 将单个图标变更应用到保留的项目快照。
fn update_project_icon_local_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    icon_pack: Option<String>,
    icon_key: Option<String>,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut found = false;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        for project in &mut workspace_snapshot.task_projects {
            if project.id == project_id {
                project.icon_pack = icon_pack.clone();
                project.icon_key = icon_key.clone();
                found = true;
            }
        }
    }
    for project in &mut guard.workbench.projects {
        if project.id == project_id {
            project.icon_pack = icon_pack.clone();
            project.icon_key = icon_key.clone();
            found = true;
        }
    }
    if found {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        Ok(())
    } else {
        Err(format!("没有找到要设置图标的项目：{project_id}"))
    }
}

/// Returns the retained LuaSkills handle for WebUI management commands.
/// 返回 WebUI 管理命令使用的保留 LuaSkills 句柄。
fn service_luaskills_handle(
    state: &SharedServiceBridgeState,
) -> Result<TauriLuaSkillsHandle, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard
        .luaskills_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; LuaSkills runtime is not ready"))
}

/// Stores a refreshed LuaSkills catalog in the shared service state.
/// 在共享服务状态中存储刷新后的 LuaSkills 目录。
fn store_luaskills_catalog(
    state: &SharedServiceBridgeState,
    catalog: TauriLuaSkillsCatalog,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    sync_agent_luaskills_tool_cache(&mut guard)?;
    guard.cached_luaskills_catalog = Some(catalog);
    Ok(())
}

/// Stores a refreshed LuaSkills catalog and appends a diagnostic log line.
/// 存储刷新后的 LuaSkills 目录并追加诊断日志行。
fn store_luaskills_catalog_with_log(
    state: &SharedServiceBridgeState,
    catalog: TauriLuaSkillsCatalog,
    message: String,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    sync_agent_luaskills_tool_cache(&mut guard)?;
    guard.cached_luaskills_catalog = Some(catalog);
    guard
        .startup
        .recent_logs
        .push(format!("[webui-service] luaskills: {message}"));
    Ok(())
}

/// Synchronizes the Agent Runtime LuaSkills tool cache from the shared LuaEngine.
/// 从共享 LuaEngine 同步 Agent Runtime 的 LuaSkills 工具缓存。
fn sync_agent_luaskills_tool_cache(guard: &mut ServiceBridgeState) -> Result<(), String> {
    let Some(runtime_handle) = guard
        .agent_runtime
        .as_ref()
        .and_then(|runtime| runtime.luaskills_runtime.as_ref())
        .cloned()
    else {
        return Ok(());
    };
    let entries = {
        let tauri_handle = guard.luaskills_runtime.as_ref().ok_or_else(|| {
            String::from("service is still starting; LuaSkills runtime is not ready")
        })?;
        let engine = tauri_handle.engine.lock().map_err(|_| {
            String::from("LuaSkills 引擎锁已损坏，无法同步 Agent Runtime 工具缓存。")
        })?;
        engine.list_entries_for_authority(
            vulcan_service_runtime::luaskills::SkillManagementAuthority::System,
        )
    };
    let entry_count = entries.len();
    let mut tool_entries = runtime_handle.tool_entries.lock().map_err(|_| {
        String::from("LuaSkills 工具缓存锁已损坏，无法同步 Agent Runtime 工具缓存。")
    })?;
    *tool_entries = entries;
    invalidate_runtime_prompt_luaskill_catalog_cache();
    guard.startup.recent_logs.push(format!(
        "[webui-service] luaskills: synced {entry_count} agent-runtime tool entries"
    ));
    Ok(())
}

/// Returns the current provider configuration stored by the agent runtime bridge.
/// 返回 Agent 运行时桥当前保存的 Provider 配置。
fn provider_configuration_from_state(
    state: &SharedServiceBridgeState,
) -> Result<ProviderConfigurationFile, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))
}

/// Mutates, persists, and stores provider configuration for WebUI skill commands.
/// 为 WebUI 技能命令变更、持久化并存储 Provider 配置。
fn mutate_provider_configuration<F>(
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut ProviderConfigurationFile) -> Result<(), String>,
{
    let mut configuration = provider_configuration_from_state(state)?;
    mutate(&mut configuration)?;
    vulcan_provider_config::config::save_configuration_file(&configuration)?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_mut() {
        runtime.provider_configuration = configuration;
    }
    Ok(())
}

/// Emits one workbench snapshot to the WebUI event stream.
/// 向 WebUI 事件流发送单份工作台快照。
fn emit_workbench_event(app: &ServiceEventEmitter, workbench: &WorkbenchSnapshot) {
    emit_service_event_with_trace(
        app,
        "workbench_snapshot_emit",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
}

/// Emits one session-addon namespace change event to connected UI clients.
/// 向已连接 UI 客户端发送一条会话附属状态命名空间变更事件。
///
/// # Parameters
/// 参数
/// `app` is the service event emitter.
/// `app` 是服务事件发送器。
/// `session_id` selects the changed session.
/// `session_id` 选择发生变化的会话。
/// `namespaces` are the addon namespaces that changed.
/// `namespaces` 是发生变化的附属状态命名空间。
/// `revision` is the latest known addon document revision.
/// `revision` 是当前已知的最新附属状态文档版本。
/// `updated_at` is the latest update timestamp.
/// `updated_at` 是最近更新时间戳。
fn emit_session_addon_changed_event(
    app: &ServiceEventEmitter,
    session_id: SessionId,
    namespaces: &[SessionAddonNamespace],
    revision: u64,
    updated_at: String,
) {
    let event = SessionAddonChangedEvent {
        session_id: session_id.to_string(),
        namespaces: namespaces
            .iter()
            .map(|namespace| namespace.as_key().to_string())
            .collect(),
        revision,
        updated_at,
        source_instance_id: None,
    };
    emit_service_event_with_trace(
        app,
        "session_addon_changed_emit",
        ServiceEvent::SessionAddonChanged(Box::new(event)),
    );
}

/// Builds a session addon document from a workspace snapshot and legacy runtime state.
/// 从工作区快照与旧运行态构建会话附属状态文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` supplies the legacy runtime state.
/// `workspace_snapshot` 提供旧运行态。
/// `session_id` selects the session being exported.
/// `session_id` 选择正在导出的会话。
/// `default_ai_tool_configuration` supplies the default AI capability template.
/// `default_ai_tool_configuration` 提供默认 AI 能力模板。
/// `namespaces` limits the addon namespaces exported from the runtime snapshot.
/// `namespaces` 限定从运行态快照导出的附属状态命名空间。
///
/// # Returns
/// 返回值
/// The derived addon state when the session runtime state exists.
/// 当会话运行态存在时返回派生附属状态。
fn session_addon_state_from_workspace_snapshot(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    default_ai_tool_configuration: &PersistedAiToolConfiguration,
    namespaces: &[SessionAddonNamespace],
) -> Option<SessionAddonState> {
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .or_else(|| {
            workspace_snapshot
                .archived_session_runtime_state
                .get(&session_id)
        })?;
    let updated_at = current_unix_millis_text();
    let mut addon_state = SessionAddonState::new(session_id);
    for namespace in namespaces {
        match namespace {
            SessionAddonNamespace::RuntimeControls => {
                if let Some(controls) = SessionRuntimeControls::from_runtime_state(runtime_state) {
                    let _ = addon_state.set_runtime_controls(&controls, updated_at.clone());
                }
            }
            SessionAddonNamespace::AiCapabilities => {
                let configuration = runtime_state
                    .ai_tool_configuration
                    .as_ref()
                    .unwrap_or(default_ai_tool_configuration);
                let _ = addon_state.set_ai_tool_configuration(configuration, updated_at.clone());
            }
            SessionAddonNamespace::Todolist => {
                if !runtime_state.todolist.is_empty() {
                    let _ = addon_state.set_todolist(&runtime_state.todolist, updated_at.clone());
                }
            }
            _ => {}
        }
    }
    (!addon_state.namespaces.is_empty()).then_some(addon_state)
}

/// Returns the latest update timestamp recorded in one addon document.
/// 返回一个附属状态文档中记录的最近更新时间。
///
/// # Parameters
/// 参数
/// `addon_state` is the addon document to inspect.
/// `addon_state` 是要检查的附属状态文档。
///
/// # Returns
/// 返回值
/// The latest namespace update timestamp, or the current timestamp when the document is empty.
/// 最新命名空间更新时间；文档为空时返回当前时间戳。
fn latest_session_addon_updated_at(addon_state: &SessionAddonState) -> String {
    addon_state
        .namespaces
        .values()
        .map(|entry| entry.updated_at.as_str())
        .max()
        .map(str::to_string)
        .unwrap_or_else(current_unix_millis_text)
}

/// Emits one service event and traces the serialized payload size when diagnostics are enabled.
/// 发送单条服务事件，并在诊断开启时记录序列化载荷大小。
fn emit_service_event_with_trace(app: &ServiceEventEmitter, stage: &str, event: ServiceEvent) {
    if webui_trace_enabled() {
        let payload_bytes = serde_json::to_vec(&event)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        webui_trace(stage, format!("payload_bytes={payload_bytes}"));
    }
    app.emit(event);
}

/// Refreshes settings-related projections on the retained workbench snapshot.
/// 刷新保留工作台快照上的设置相关投影。
fn refresh_settings_projection_locked(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: Option<&PersistedAiToolConfiguration>,
) {
    let ai_tool_configuration = ai_tool_configuration
        .cloned()
        .or_else(|| {
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.ai_tool_configuration.clone())
        })
        .unwrap_or_default();
    let selected_ai_tool_configuration = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&workspace.selected_session)
                .and_then(|runtime_state| runtime_state.ai_tool_configuration.clone())
        })
        .unwrap_or_else(|| ai_tool_configuration.clone());
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.providers = providers(provider_configuration);
    guard.workbench.mcp_servers = mcp_servers(provider_configuration);
    guard.workbench.reasoning_selection = reasoning_selection(provider_configuration);
    guard.workbench.git_commit = git_commit_settings(provider_configuration);
    let ai_capability_model_capabilities =
        active_provider_configuration_model_capabilities(provider_configuration);
    guard.workbench.ai_capabilities = ai_capabilities(
        &selected_ai_tool_configuration,
        ai_capability_model_capabilities.as_ref(),
    );
    guard.workbench.ai_capability_vendors = ai_capability_vendors(&selected_ai_tool_configuration);
    guard.workbench.ai_capability_vendor_catalog = ai_capability_vendor_catalog();
    guard.workbench.composer_attachment_capabilities =
        composer_attachment_capabilities(provider_configuration, &selected_ai_tool_configuration);
}

/// Returns the mutable runtime state for the currently selected regular session.
/// 返回当前选中常规会话的可变运行态。
///
/// # Parameters
/// 参数
/// `workspace` is the retained WebUI workspace snapshot.
/// `workspace` 是 WebUI 保留的工作区快照。
///
/// # Returns
/// 返回值
/// The selected session id and its mutable runtime-state entry when a regular session is selected.
/// 当当前选中常规会话时，返回会话 id 及其可变运行态条目。
fn selected_session_runtime_state_mut(
    workspace: &mut WorkspaceSnapshot,
) -> Option<(SessionId, &mut SessionRuntimeStateEntry)> {
    let session_id = workspace.selected_session;
    if session_id == SessionId::nil()
        || !workspace
            .sessions
            .iter()
            .any(|session| session.id == session_id)
    {
        return None;
    }
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: current_unix_millis_text(),
            ..SessionRuntimeStateEntry::default()
        });
    Some((session_id, runtime_state))
}

/// Returns the AI capability configuration owned by one session or the default template.
/// 返回指定会话拥有的 AI 能力配置，若会话尚无配置则返回默认模板。
///
/// # Parameters
/// 参数
/// `workspace` is the retained workspace snapshot that may contain session-scoped runtime state.
/// `workspace` 是可能包含会话级运行态的保留工作区快照。
/// `session_id` selects the session whose configuration should be used.
/// `session_id` 选择应使用其配置的会话。
/// `default_configuration` is the default AI capability template.
/// `default_configuration` 是默认 AI 能力配置模板。
///
/// # Returns
/// 返回值
/// The session-scoped AI capability configuration when present, otherwise the default template.
/// 存在时返回会话级 AI 能力配置，否则返回默认模板。
fn session_ai_tool_configuration_or_default<'a>(
    workspace: &'a WorkspaceSnapshot,
    session_id: SessionId,
    default_configuration: &'a PersistedAiToolConfiguration,
) -> &'a PersistedAiToolConfiguration {
    workspace
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.ai_tool_configuration.as_ref())
        .unwrap_or(default_configuration)
}

/// Initializes one session AI capability configuration from the default template when absent.
/// 在缺失时使用默认模板初始化单个会话的 AI 能力配置。
///
/// # Parameters
/// 参数
/// `workspace` is the retained workspace snapshot that owns the session runtime state.
/// `workspace` 是拥有会话运行态的保留工作区快照。
/// `session_id` selects the session that should receive its own configuration snapshot.
/// `session_id` 选择应接收独立配置快照的会话。
/// `default_configuration` is the default AI capability template.
/// `default_configuration` 是默认 AI 能力配置模板。
///
/// # Returns
/// 返回值
/// `true` when this call initialized the session configuration.
/// 当本次调用初始化了会话配置时返回 `true`。
fn ensure_session_ai_tool_configuration(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    default_configuration: &PersistedAiToolConfiguration,
) -> bool {
    if session_id == SessionId::nil() {
        return false;
    }
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: current_unix_millis_text(),
            ..SessionRuntimeStateEntry::default()
        });
    if runtime_state.ensure_ai_tool_configuration(default_configuration) {
        runtime_state.updated_at = current_unix_millis_text();
        return true;
    }
    false
}

/// Refreshes model, reasoning, attachment, and context-usage projections after runtime controls change.
/// 在运行控制项变化后刷新模型、思考、附件与上下文占用投影。
///
/// # Parameters
/// 参数
/// `guard` is the locked service state being updated.
/// `guard` 是正在更新的服务状态锁。
/// `provider_configuration` is the effective provider configuration for the selected session.
/// `provider_configuration` 是当前选中会话的生效 Provider 配置。
/// `ai_tool_configuration` is the effective AI tool capability configuration.
/// `ai_tool_configuration` 是生效的 AI 工具能力配置。
fn refresh_workbench_runtime_controls_locked(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) {
    let context_usage = guard.workspace_snapshot.as_ref().and_then(|workspace| {
        context_usage_snapshot_from_runtime_state(
            workspace.selected_session,
            workspace
                .session_runtime_state
                .get(&workspace.selected_session),
            &guard.live_context_budgets,
            provider_configuration,
            ai_tool_configuration,
        )
    });
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.providers = providers(provider_configuration);
    guard.workbench.reasoning_selection = reasoning_selection(provider_configuration);
    guard.workbench.composer_attachment_capabilities =
        composer_attachment_capabilities(provider_configuration, ai_tool_configuration);
    guard.workbench.context_usage = context_usage;
}

/// Restores WebUI runtime controls from one session-scoped runtime state when possible.
/// 在可行时从单个会话运行态恢复 WebUI 运行控制项。
///
/// # Parameters
/// 参数
/// `provider_configuration` is the mutable provider configuration used to build the next workbench.
/// `provider_configuration` 是用于构建下一份工作台快照的可变 Provider 配置。
/// `runtime_state` is the selected session runtime-state entry.
/// `runtime_state` 是当前选中会话的运行态条目。
fn restore_webui_runtime_controls_for_session(
    provider_configuration: &mut ProviderConfigurationFile,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) {
    let Some(runtime_state) = runtime_state else {
        return;
    };
    if let (Some(provider_id), Some(model_id)) = (
        runtime_state.active_provider_id.as_deref(),
        runtime_state.active_model_id.as_deref(),
    ) {
        let model_is_available =
            provider_config::effective_model_choices(provider_id, provider_configuration)
                .into_iter()
                .any(|choice| choice.id == model_id);
        if model_is_available {
            provider_configuration.model = Some(format!("{provider_id}/{model_id}"));
        }
    }
    if let Some(reasoning_effort) = runtime_state.active_reasoning_effort {
        provider_configuration.workbench_reasoning_effort = Some(reasoning_effort);
    }
}

/// Stores provider and MCP runtime changes after a configuration mutation.
/// 在配置变更后存储 Provider 与 MCP 运行时变化。
fn store_provider_and_mcp_configuration_after_change(
    state: &SharedServiceBridgeState,
    provider_configuration: ProviderConfigurationFile,
    next_mcp_runtime: Option<McpRuntimeAccess>,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let ai_tool_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.ai_tool_configuration.clone())
        .unwrap_or_default();
    if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
        if let Some(previous_mcp_runtime) = agent_runtime.mcp_runtime.take() {
            previous_mcp_runtime.shutdown();
        }
        agent_runtime.provider_configuration = provider_configuration.clone();
        agent_runtime.mcp_runtime = next_mcp_runtime;
    }
    refresh_settings_projection_locked(
        &mut guard,
        &provider_configuration,
        Some(&ai_tool_configuration),
    );
    Ok(guard.workbench.clone())
}

/// Returns the canonical WebUI request key for one provider field.
/// 返回单个 Provider 字段的规范 WebUI 请求键。
pub(crate) fn provider_field_request_key(field: ProviderFieldKey) -> &'static str {
    match field {
        ProviderFieldKey::ApiKey => "apiKey",
        ProviderFieldKey::BaseUrl => "baseUrl",
        ProviderFieldKey::ResourceName => "resourceName",
        ProviderFieldKey::AccountId => "accountId",
    }
}

/// Parses one MCP transport key from the WebUI request payload.
/// 从 WebUI 请求载荷解析单个 MCP 传输键。
fn parse_mcp_transport(value: &str) -> Result<PersistedMcpTransportKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stdio" => Ok(PersistedMcpTransportKind::Stdio),
        "http" => Ok(PersistedMcpTransportKind::Http),
        "sse" => Ok(PersistedMcpTransportKind::Sse),
        other => Err(format!("不支持的 MCP 传输类型：{other}")),
    }
}

/// Parses one AI tool kind key from the WebUI request payload.
/// 从 WebUI 请求载荷解析单个 AI 工具类型键。
fn parse_ai_tool_kind(value: &str) -> Result<AiToolKind, String> {
    match value.trim() {
        "web_search" => Ok(AiToolKind::WebSearch),
        "understand_image" => Ok(AiToolKind::UnderstandImage),
        "image_generation" => Ok(AiToolKind::ImageGeneration),
        "video_generation" => Ok(AiToolKind::VideoGeneration),
        "speech_synthesis" => Ok(AiToolKind::SpeechSynthesis),
        "music_generation" => Ok(AiToolKind::MusicGeneration),
        "music_cover" => Ok(AiToolKind::MusicCover),
        other => Err(format!("不支持的 AI 能力类型：{other}")),
    }
}

/// Converts a WebUI key/value list into one validated map.
/// 将 WebUI 键值列表转换为经过校验的映射。
fn key_value_inputs_to_map(
    inputs: Vec<KeyValueInput>,
    subject_label: &str,
    value_label: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for input in inputs {
        let key = input.key.trim();
        let value = input.value.trim();
        if key.is_empty() && value.is_empty() {
            continue;
        }
        if key.is_empty() {
            return Err(format!("{subject_label} 的键不能为空。"));
        }
        if value.is_empty() {
            return Err(format!("{subject_label} 的{value_label}不能为空。"));
        }
        if map.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("{subject_label} `{key}` 重复，请保留一条即可。"));
        }
    }
    Ok(map)
}

/// Converts optional text into an owned option after trimming.
/// 裁剪可选文本并转换为自有可选值。
fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Builds one persisted MCP server entry from a WebUI save request.
/// 从 WebUI 保存请求构建单个持久化 MCP 服务器条目。
fn mcp_server_entry_from_request(
    request: McpServerSaveRequest,
) -> Result<(String, PersistedMcpServerEntry), String> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(String::from("请先填写 MCP 服务器名称。"));
    }
    let server_id = request
        .server_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| stable_identifier_from_label(name));
    let transport = parse_mcp_transport(&request.transport)?;
    let oauth = mcp_oauth_config_from_request(&request);
    match transport {
        PersistedMcpTransportKind::Stdio => {
            let command = request.command.trim();
            if command.is_empty() {
                return Err(String::from("请先填写 MCP 服务器启动命令。"));
            }
            Ok((
                server_id,
                PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: request.enabled,
                    transport,
                    command: command.to_string(),
                    args: request
                        .args
                        .into_iter()
                        .map(|arg| arg.trim().to_string())
                        .filter(|arg| !arg.is_empty())
                        .collect(),
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    headers_from_env: BTreeMap::new(),
                    oauth: None,
                    cwd: trim_to_option(&request.cwd),
                    env: key_value_inputs_to_map(request.env, "环境变量", "值")?,
                    env_passthrough: request
                        .env_passthrough
                        .into_iter()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                },
            ))
        }
        PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => {
            let url = request.url.trim();
            if url.is_empty() {
                return Err(String::from("请先填写 MCP 远端服务器 URL。"));
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(String::from(
                    "MCP 远端服务器 URL 必须以 `http://` 或 `https://` 开头。",
                ));
            }
            Ok((
                server_id,
                PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: request.enabled,
                    transport,
                    command: String::new(),
                    args: Vec::new(),
                    url: Some(url.to_string()),
                    bearer_token: trim_to_option(&request.bearer_token),
                    headers: key_value_inputs_to_map(request.headers, "请求头", "值")?,
                    headers_from_env: key_value_inputs_to_map(
                        request.headers_from_env,
                        "来自环境变量的请求头",
                        "环境变量名",
                    )?,
                    oauth,
                    cwd: None,
                    env: BTreeMap::new(),
                    env_passthrough: Vec::new(),
                },
            ))
        }
    }
}

/// Builds an optional MCP OAuth block from a WebUI save request.
/// 从 WebUI 保存请求构建可选 MCP OAuth 配置块。
fn mcp_oauth_config_from_request(
    request: &McpServerSaveRequest,
) -> Option<PersistedMcpOAuthConfig> {
    let client_id = trim_to_option(&request.oauth_client_id);
    let client_secret = trim_to_option(&request.oauth_client_secret);
    let redirect_uri = trim_to_option(&request.oauth_redirect_uri);
    let scopes = request
        .oauth_scopes
        .iter()
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect::<Vec<_>>();
    if client_id.is_none() && client_secret.is_none() && redirect_uri.is_none() && scopes.is_empty()
    {
        None
    } else {
        Some(PersistedMcpOAuthConfig {
            client_id,
            client_secret,
            scopes,
            redirect_uri,
        })
    }
}

/// Builds one stable identifier from a human-readable label.
/// 从人类可读标签构建单个稳定标识。
fn stable_identifier_from_label(label: &str) -> String {
    let mut normalized = label
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while normalized.contains("--") {
        normalized = normalized.replace("--", "-");
    }
    let trimmed = normalized.trim_matches('-');
    if trimmed.is_empty() {
        format!("mcp-{}", current_unix_millis_value())
    } else {
        trimmed.to_string()
    }
}

/// Reprojects Agent Skills after a focused prompt-skill configuration change.
/// 在聚焦的提示词技能配置变更后重新投影 Agent Skills。
fn refresh_agent_skills_local_state(
    state: &SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogSummary, String> {
    let provider_configuration = provider_configuration_from_state(state)?;
    let summary = agent_skills_catalog_summary(&provider_configuration);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.agent_skills = summary.clone();
    Ok(summary)
}

/// Reprojects LuaSkills after a focused tool-registration configuration change.
/// 在聚焦的工具注册配置变更后重新投影 LuaSkills。
fn refresh_luaskills_local_state(
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let provider_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .unwrap_or_default();
    let catalog = cached_luaskills_catalog_from_state(&guard);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.lua_skills = lua_skills_catalog_summary(&catalog, &provider_configuration);
    guard.workbench.plugins = plugins_from_luaskills(&catalog);
    Ok(guard.workbench.clone())
}

/// Parses the project-level Agent Skill tri-state key from the WebUI.
/// 从 WebUI 解析项目级 Agent Skill 三态键。
fn parse_agent_skill_project_state(
    value: &str,
) -> Result<Option<PersistedPromptSkillState>, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "inherit" => Ok(None),
        "enabled" => Ok(Some(PersistedPromptSkillState::Enabled)),
        "disabled" => Ok(Some(PersistedPromptSkillState::Disabled)),
        other => Err(format!("不支持的项目级 Agent Skill 状态：{other}")),
    }
}

/// Parses a persisted LuaSkills tool registration state key from the WebUI.
/// 从 WebUI 解析持久化 LuaSkills 工具注册状态键。
fn parse_luaskill_tool_registration_state(
    value: &str,
) -> Result<PersistedLuaSkillToolRegistrationState, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "explicit" => Ok(PersistedLuaSkillToolRegistrationState::Explicit),
        "implicit" => Ok(PersistedLuaSkillToolRegistrationState::Implicit),
        "disabled" => Ok(PersistedLuaSkillToolRegistrationState::Disabled),
        other => Err(format!("不支持的 LuaSkills 工具注册状态：{other}")),
    }
}

/// Returns the selected project path for a VMM request scope.
/// 返回 VMM 请求作用域选中的项目路径。
fn vmm_project_scope_path(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区尚未载入，无法保存项目 VMM 配置。"))?;
    vmm_request_project_scope(workspace_snapshot, project_id)
        .map(|project| project.path.clone())
        .ok_or_else(|| String::from("选中的项目已不存在，请重新选择。"))
}

/// Finds the project scope requested by WebUI or implied by the selected session.
/// 查找 WebUI 请求或当前选中会话隐含的项目作用域。
fn vmm_request_project_scope<'a>(
    workspace: &'a vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    project_id: Option<&str>,
) -> Option<&'a TaskProjectSummary> {
    project_id
        .and_then(|project_id| {
            workspace
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
        })
        .or_else(|| {
            selected_workspace_session(workspace)
                .and_then(|session| session.project_id.as_deref())
                .and_then(|project_id| {
                    workspace
                        .task_projects
                        .iter()
                        .find(|project| project.id == project_id)
                })
        })
        .or_else(|| workspace.task_projects.first())
}

/// Returns the currently selected workspace session when it exists.
/// 返回当前选中的工作区会话（若存在）。
fn selected_workspace_session<'a>(
    workspace: &'a vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> Option<&'a SessionSummary> {
    workspace
        .sessions
        .iter()
        .find(|session| session.id == workspace.selected_session)
}

/// Reads all long-lived runtime handles required by a WebUI prompt submission.
/// 读取 WebUI 提示词提交所需的全部长生命周期运行时句柄。
fn runtime_parts_from_state(
    state: &SharedServiceBridgeState,
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
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
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
    let runtime_state_snapshot = if runtime_state_has_concrete_conversation_context(runtime_state) {
        sanitize_runtime_state_legacy_goal_internal_messages(runtime_state);
        Some(runtime_state.clone())
    } else {
        None
    };
    if let Some(runtime_state_snapshot) = runtime_state_snapshot.as_ref() {
        merge_runtime_state_records_into_display_messages(
            workspace_snapshot,
            session_id,
            runtime_state_snapshot,
        );
    } else {
        workspace_snapshot
            .session_messages
            .entry(session_id)
            .or_default();
    }
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
}

/// Returns whether the session has a submitted Plan artifact awaiting user confirmation.
/// 返回会话是否存在等待用户确认的已提交计划产物。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains retained session runtime state.
/// `workspace_snapshot` 包含保留的会话运行态。
/// `session_id` identifies the session to inspect.
/// `session_id` 标识需要检查的会话。
///
/// # Returns
/// 返回值
/// `true` when a Plan artifact is still pending a user decision.
/// 当计划产物仍在等待用户决策时返回 `true`。
fn session_has_pending_plan_artifact(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> bool {
    workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.plan_mode.artifact.as_ref())
        .is_some()
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

/// Stops WebUI-visible session work and restores queue state to the service stopped semantics.
/// 停止 WebUI 可见会话任务，并按服务停止语义恢复队列状态。
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
    let cleared_system_queue = clear_internal_pending_prompt_queue_for_stop(runtime_state);
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
        SessionTurnStatsKind::UserTurn,
        Vec::new(),
    );
    let runtime_state_snapshot = runtime_state.clone();
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
    merge_runtime_state_records_into_display_messages(
        workspace_snapshot,
        session_id,
        &runtime_state_snapshot,
    );
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
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
    clear_internal_pending_prompt_queue_for_stop(runtime_state);
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

/// Appends one canonical WebUI turn-statistics record using the shared WebUI turn stats builder.
/// 使用 WebUI 轮次统计构建器追加一条 WebUI 规范轮次统计记录。
fn append_webui_session_turn_stats_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    closing_record_id: &str,
    status: SessionTurnStatsStatus,
    provider_request_count_override: Option<usize>,
    turn_kind: SessionTurnStatsKind,
    usage_segments: Vec<SessionTurnUsageSegment>,
) -> Option<String> {
    let turn_stats = build_session_turn_stats_from_state_with_segments(
        &runtime_state.conversation_state,
        closing_record_id,
        status,
        provider_request_count_override,
        turn_kind,
        usage_segments,
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
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()))
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

/// Captures the service-equivalent context snapshot before a WebUI user prompt mutates context.
/// 在 WebUI 用户提示词改变上下文前捕获与服务运行时等价的上下文快照。
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

/// Applies service-equivalent outer steps after one WebUI prompt begin reducer has run.
/// 在 WebUI 提示词 begin reducer 执行后应用与服务运行时等价的外层步骤。
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
    {
        let previous_persistent_fingerprint =
            persistent_injection_fingerprint(&runtime_state.context_injection_state);
        if remove_legacy_vmm_profile_context_injection_state(
            &mut runtime_state.context_injection_state,
        ) {
            let timestamp = current_unix_millis_text();
            runtime_state.updated_at = timestamp.clone();
            let event = legacy_vmm_injection_cancelled_event(
                previous_persistent_fingerprint,
                "legacy_vmm_profile_webui_prompt_cleanup",
            );
            if let Err(error) = record_webui_runtime_injection_cancelled(
                controller_runtime,
                session_id,
                event,
                &timestamp,
            ) {
                webui_trace(
                    "runtime_injection_cancelled_audit_failed",
                    format!("session={session_id} error={error}"),
                );
            }
        }
    }
    persist_webui_context_current_document(
        controller_runtime,
        workspace_snapshot,
        session_id,
        session_title,
    );
}

/// Persists the service-equivalent current context document for a WebUI session when possible.
/// 在可行时为 WebUI 会话持久化与服务运行时等价的当前上下文文档。
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
        eprintln!("[webui-service] current context persist failed for `{session_title}`: {error}");
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
    let now = current_unix_millis_text();
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
    .with_title_source(SessionTitleSource::Generated)
    .with_created_at(now.clone())
    .with_updated_at(now);

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
        rebuilt_messages = merge_session_messages_with_conversation_state_records(
            session_messages
                .get(&session_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &runtime_state.conversation_state,
        );
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

/// Returns whether the retained workspace snapshot contains one active project.
/// 返回保留工作区快照是否包含一个活动项目。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the current retained WebUI workspace snapshot.
/// `workspace_snapshot` 是当前保留的 WebUI 工作区快照。
/// `project_id` is the project id required for a new draft submission.
/// `project_id` 是新建草稿提交所需的项目标识。
///
/// # Returns
/// 返回值
/// `true` when the active project list contains the requested project id.
/// 当活动项目列表包含请求的项目标识时返回 `true`。
fn workspace_snapshot_contains_active_project(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
) -> bool {
    workspace_snapshot
        .task_projects
        .iter()
        .any(|project| project.id == project_id)
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
            eprintln!("[webui-service] background persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
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
            eprintln!("[webui-service] background persist failed ({reason}): {error}");
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
            eprintln!("[webui-service] runtime-state persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
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
                "[webui-service] background runtime-state persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Spawns a background write for the current visible transcript without touching runtime state.
/// 为当前可见转录启动后台写入，且不触碰运行态。
///
/// # Parameters
/// # 参数
/// `state` owns the live transcript persistence version table.
/// `state` 持有运行中转录持久化版本表。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `workspace_snapshot` is the current workspace snapshot containing display rows.
/// `workspace_snapshot` 是包含显示行的当前工作区快照。
/// `session_id` selects the session transcript to persist.
/// `session_id` 选择要持久化的会话转录。
/// `reason` is an optional diagnostic label for logs.
/// `reason` 是用于日志的可选诊断标签。
fn spawn_workspace_session_transcript_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let display_messages = display_session_messages_from_workspace(&workspace_snapshot, session_id)
        .unwrap_or_default();
    if display_messages.is_empty() {
        return;
    }
    let persist_version = match next_webui_session_transcript_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[webui-service] live transcript persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_transcript_persist_version_is_current(&state, session_id, persist_version)
        {
            return;
        }
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_transcript_messages(
                controller_runtime,
                session_id,
                &display_messages,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("live_transcript");
            eprintln!(
                "[webui-service] background live transcript persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Spawns a background write for one session addon-state document.
/// 为单个会话附属状态文档启动后台写入任务。
///
/// # Parameters
/// 参数
/// `app` is the event emitter used after the file write succeeds.
/// `app` 是文件写入成功后使用的事件发送器。
/// `state` owns the addon persistence version table used to reject stale writes.
/// `state` 持有用于拒绝过期写入的附属状态持久化版本表。
/// `controller_runtime` is the data-plane controller handle.
/// `controller_runtime` 是数据平面控制器句柄。
/// `workspace_snapshot` supplies session ownership and runtime compatibility fields.
/// `workspace_snapshot` 提供会话归属与运行态兼容字段。
/// `session_id` selects the addon document to persist.
/// `session_id` 选择要持久化的附属状态文档。
/// `addon_state` is the already-derived addon document to persist.
/// `addon_state` 是已经派生完成且需要持久化的附属状态文档。
/// `changed_namespaces` are the namespace keys represented by this write.
/// `changed_namespaces` 是本次写入代表的命名空间键。
/// `reason` labels the write site for diagnostics.
/// `reason` 标记写入来源，供诊断使用。
fn spawn_workspace_session_addon_state_persist_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    addon_state: SessionAddonState,
    changed_namespaces: Vec<SessionAddonNamespace>,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let mut persist_versions = Vec::new();
    for namespace in &changed_namespaces {
        match next_webui_session_addon_persist_version(&state, session_id, *namespace) {
            Ok(version) => persist_versions.push((*namespace, version)),
            Err(error) => {
                eprintln!(
                    "[webui-service] background session-addon persist version failed: {error}"
                );
                return;
            }
        }
    }
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_addon_persist_versions_are_current(&state, session_id, &persist_versions)
        {
            return;
        }
        let Some((session, archived)) = workspace_snapshot
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| (session.clone(), false))
            .or_else(|| {
                workspace_snapshot
                    .archived_sessions
                    .iter()
                    .find(|session| session.id == session_id)
                    .map(|session| (session.clone(), true))
            })
        else {
            return;
        };
        let task_projects = workspace_snapshot
            .task_projects
            .iter()
            .cloned()
            .chain(workspace_snapshot.archived_task_projects.iter().cloned())
            .collect::<Vec<_>>();
        match vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update(
            controller_runtime,
            &task_projects,
            &session,
            archived,
            &addon_state,
        )
        .await
        {
            Ok(merged_state) => {
                emit_session_addon_changed_event(
                    &app,
                    session_id,
                    &changed_namespaces,
                    merged_state.revision,
                    latest_session_addon_updated_at(&merged_state),
                );
            }
            Err(error) => {
                let reason_label = reason.as_deref().unwrap_or("session_addon");
                eprintln!(
                    "[webui-service] background session-addon persist failed ({reason_label}): {error}"
                );
            }
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

/// Allocates the next live transcript persistence version for one WebUI session.
/// 为一个 WebUI 会话分配下一次运行中转录持久化版本。
///
/// # Parameters
/// # 参数
/// `state` owns the per-session live transcript persistence version table.
/// `state` 持有按会话归属的运行中转录持久化版本表。
/// `session_id` selects the session whose transcript is being persisted.
/// `session_id` 选择正在持久化转录的会话。
///
/// # Returns
/// # 返回值
/// The version assigned to the new live transcript persistence task.
/// 返回分配给新运行中转录持久化任务的版本。
fn next_webui_session_transcript_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let next_version = guard
        .session_transcript_persist_versions
        .get(&session_id)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_transcript_persist_versions
        .insert(session_id, next_version);
    Ok(next_version)
}

/// Returns whether a live transcript persistence task is still current for its session.
/// 判断某个运行中转录持久化任务是否仍是其会话的最新任务。
///
/// # Parameters
/// # 参数
/// `state` owns the per-session live transcript persistence version table.
/// `state` 持有按会话归属的运行中转录持久化版本表。
/// `session_id` selects the session being persisted.
/// `session_id` 选择正在持久化的会话。
/// `persist_version` is the task-local transcript persistence version.
/// `persist_version` 是任务本地转录持久化版本。
///
/// # Returns
/// # 返回值
/// `true` when the transcript task may write to disk.
/// 当该转录任务可以写入磁盘时返回 `true`。
fn webui_session_transcript_persist_version_is_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_version: u64,
) -> bool {
    state.lock().ok().and_then(|guard| {
        guard
            .session_transcript_persist_versions
            .get(&session_id)
            .copied()
    }) == Some(persist_version)
}

/// Allocates the next background addon persistence version for one session namespace.
/// 为一个会话命名空间分配下一次后台附属状态持久化版本。
///
/// # Parameters
/// 参数
/// `state` owns the per-session addon persistence version table.
/// `state` 持有按会话划分的附属状态持久化版本表。
/// `session_id` selects the session whose addon namespace is being persisted.
/// `session_id` 选择正在持久化附属状态命名空间的会话。
/// `namespace` selects the addon namespace being persisted.
/// `namespace` 选择正在持久化的附属状态命名空间。
///
/// # Returns
/// 返回值
/// The version assigned to the new background addon persistence task for that namespace.
/// 返回分配给该命名空间的新后台附属状态持久化任务版本。
fn next_webui_session_addon_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    namespace: SessionAddonNamespace,
) -> Result<u64, String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let key = (session_id, namespace);
    let next_version = guard
        .session_addon_persist_versions
        .get(&key)
        .copied()
        .unwrap_or_default()
        .saturating_add(1);
    guard
        .session_addon_persist_versions
        .insert(key, next_version);
    Ok(next_version)
}

/// Returns whether a background addon persistence task is still latest for every namespace it writes.
/// 判断某个后台附属状态持久化任务是否仍是其写入的所有命名空间的最新任务。
///
/// # Parameters
/// 参数
/// `state` owns the per-session addon persistence version table.
/// `state` 持有按会话划分的附属状态持久化版本表。
/// `session_id` selects the session whose addon namespaces are being persisted.
/// `session_id` 选择正在持久化附属状态命名空间的会话。
/// `persist_versions` are the task-local addon persistence versions keyed by namespace.
/// `persist_versions` 是按命名空间记录的任务本地附属状态持久化版本。
///
/// # Returns
/// 返回值
/// `true` when the task may write the addon document.
/// 当该任务可以写入附属状态文档时返回 `true`。
fn webui_session_addon_persist_versions_are_current(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    persist_versions: &[(SessionAddonNamespace, u64)],
) -> bool {
    let Ok(guard) = state.lock() else {
        return false;
    };
    persist_versions.iter().all(|(namespace, version)| {
        guard
            .session_addon_persist_versions
            .get(&(session_id, *namespace))
            .copied()
            == Some(*version)
    })
}

/// Invalidates older background addon persistence snapshots for one session namespace.
/// 使某个会话命名空间上较旧的后台附属状态持久化快照失效。
///
/// # Parameters
/// 参数
/// `state` owns the per-session addon persistence version table.
/// `state` 持有按会话划分的附属状态持久化版本表。
/// `session_id` selects the session whose pending addon namespace writes should be invalidated.
/// `session_id` 选择需要使待处理附属状态命名空间写入失效的会话。
/// `namespace` selects the addon namespace whose pending writes should be invalidated.
/// `namespace` 选择需要使待处理写入失效的附属状态命名空间。
fn invalidate_webui_session_addon_persist_version(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    namespace: SessionAddonNamespace,
) -> Result<(), String> {
    let _ = next_webui_session_addon_persist_version(state, session_id, namespace)?;
    Ok(())
}

/// Builds an empty LuaSkills catalog for degraded snapshot rendering.
/// 为降级快照渲染构建空 LuaSkills 目录。
fn empty_luaskills_catalog() -> TauriLuaSkillsCatalog {
    TauriLuaSkillsCatalog {
        runtime_root: PathBuf::new(),
        system_lua_lib_dir: PathBuf::new(),
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
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
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
        &state,
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
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
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
        &state,
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
    app: &ServiceEventEmitter,
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
    let (
        live_context_budgets,
        surface_messages,
        subagent_navigation,
        pending_question,
        session_goal_summaries,
        command_session_runtime,
        hydrated_window_summaries,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
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
        let pending_question =
            pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
        let session_goal_summaries = guard.session_goal_summaries.clone();
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        (
            live_context_budgets,
            surface_messages,
            subagent_navigation,
            pending_question,
            session_goal_summaries,
            command_session_runtime,
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
        )
    };
    let trace_workbench_started_at = current_unix_millis_value();
    let workbench = workbench_from_runtime(
        controller_runtime,
        command_session_runtime.as_deref(),
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question,
        logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
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
    emit_workbench_event(&app, &workbench);
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

/// Spawns one background task that forwards agent runtime stream events into the WebUI state.
/// 启动一个后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
fn spawn_agent_stream_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
) {
    spawn_agent_stream_task_with_auto_compaction_state(
        app,
        state,
        controller_runtime,
        agent_runtime,
        request,
        false,
    );
}

/// Spawns one background task that forwards agent runtime stream events with per-request auto-compaction state.
/// 启动一个携带单请求自动压缩状态的后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
///
/// # Parameters
/// # 参数
/// `app` emits workbench snapshots and runtime stream projections.
/// `app` 发出工作台快照与运行时流投影。
/// `state` owns retained service state and active stream handles.
/// `state` 持有保留的服务状态与活跃流句柄。
/// `controller_runtime` persists provider checkpoints and terminal session state.
/// `controller_runtime` 持久化 Provider 检查点与终态会话状态。
/// `agent_runtime` provides provider, tool, hook, and compaction runtimes.
/// `agent_runtime` 提供 Provider、工具、hook 与压缩运行时。
/// `request` is the prepared execution request for the current user turn.
/// `request` 是当前用户回合的待执行请求。
/// `auto_compaction_already_attempted` prevents repeated automatic compaction loops for one user request.
/// `auto_compaction_already_attempted` 防止单个用户请求反复进入自动压缩循环。
fn spawn_agent_stream_task_with_auto_compaction_state(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
    auto_compaction_already_attempted: bool,
) {
    let session_id = request.session_id;
    webui_trace("stream_task_spawn", format!("session={session_id}"));
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    let task_state = state.clone();
    let handle = spawn_service_task(async move {
        let stream_task_started_at = current_unix_millis_value();
        webui_trace("stream_task_entered", format!("session={session_id}"));
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut reached_finished_event = false;
        let prompt_ai_tool_configuration = request.ai_tool_configuration.clone();
        let prompt_intervention = build_webui_prompt_intervention(
            app.clone(),
            task_state.clone(),
            agent_runtime.provider_configuration.clone(),
            prompt_ai_tool_configuration,
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
            request.clone(),
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
                        eprintln!("[webui-service] apply preparation failed: {error}");
                    }
                    let auto_compaction_result = if auto_compaction_already_attempted {
                        Ok(None)
                    } else {
                        maybe_auto_compact_webui_session_before_provider_request(
                            &app,
                            &task_state,
                            &controller_runtime,
                            &agent_runtime,
                            &request,
                            &preparation,
                        )
                        .await
                    };
                    match auto_compaction_result {
                        Ok(Some(updated_request)) => {
                            webui_trace(
                                "stream_auto_compaction_restarting",
                                format!("session={session_id}"),
                            );
                            if let Ok(mut guard) = task_state.lock() {
                                guard.active_stream_tasks.remove(&session_id);
                            }
                            spawn_agent_stream_task_with_auto_compaction_state(
                                app,
                                task_state,
                                controller_runtime,
                                agent_runtime,
                                updated_request,
                                true,
                            );
                            return;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            eprintln!(
                                "[webui-service] automatic context compaction failed before provider request: {error}"
                            );
                            if let Ok(mut guard) = task_state.lock() {
                                guard.active_stream_tasks.remove(&session_id);
                                finish_webui_session_kernel_turn(
                                    &mut guard,
                                    session_id,
                                    TurnTerminalStatus::Failed,
                                );
                            }
                            let _ = apply_session_phase(
                                &app,
                                &task_state,
                                session_id,
                                SessionRunPhase::Failed,
                            );
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::ProviderRequestCheckpoint {
                    session_id,
                    snapshot,
                } => {
                    if let Err(error) = record_webui_provider_request_checkpoint(
                        &controller_runtime,
                        session_id,
                        &snapshot,
                    ) {
                        eprintln!("[webui-service] provider request checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
                    if let Err(error) = record_webui_provider_lowering_repair(
                        &controller_runtime,
                        session_id,
                        event,
                    ) {
                        eprintln!("[webui-service] provider lowering repair failed: {error}");
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
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] apply request-ready phase failed: {error}");
                    }
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
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] apply wire-request-ready phase failed: {error}");
                    }
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
                        eprintln!("[webui-service] apply request-started failed: {error}");
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
                        eprintln!("[webui-service] append thinking delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) =
                        collapse_streaming_thinking(&app, &task_state, session_id, &record_ids)
                    {
                        eprintln!("[webui-service] complete thinking failed: {error}");
                    }
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] restore request phase failed: {error}");
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
                        eprintln!("[webui-service] append output delta failed: {error}");
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
                            eprintln!("[webui-service] apply tool phase failed: {error}");
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
                                "[webui-service] restore request phase after tool failed: {error}"
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
                            "[webui-service] start detached subagent listener failed: {error}"
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
                            eprintln!("[webui-service] ingest subagent event failed: {error}");
                            false
                        }
                    };
                    if !subagent_event_consumed
                        && let Err(error) =
                            append_runtime_event_projection(&app, &task_state, session_id, &event)
                    {
                        eprintln!("[webui-service] append runtime event failed: {error}");
                    }
                    acknowledge_runtime_event_projection_barrier(&event);
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
                                "[webui-service] prepare pending prompt after runtime event failed: {error}"
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
                    if let Some(usage) = last_successful_provider_usage.as_ref()
                        && let Err(error) =
                            cache_live_context_budget_observed_usage(&task_state, session_id, usage)
                    {
                        eprintln!(
                            "[webui-service] live context budget usage update failed: {error}"
                        );
                    }
                    if let Err(error) = cache_session_goal_live_usage_from_checkpoint(
                        &task_state,
                        session_id,
                        &conversation_state,
                        provider_request_count,
                        true,
                    ) {
                        eprintln!("[webui-service] session goal live usage update failed: {error}");
                    }
                    if let Err(error) = apply_conversation_checkpoint(
                        &app,
                        &task_state,
                        &controller_runtime,
                        session_id,
                        conversation_state,
                        last_successful_provider_usage,
                        provider_request_count,
                    ) {
                        eprintln!("[webui-service] apply checkpoint failed: {error}");
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
                    if let Err(error) =
                        record_webui_provider_response_checkpoint(&controller_runtime, &report)
                    {
                        eprintln!(
                            "[webui-service] provider response checkpoint write failed: {error}"
                        );
                    }
                    let goal_elapsed_seconds = current_unix_millis_value()
                        .saturating_sub(stream_task_started_at)
                        .checked_div(1_000)
                        .and_then(|value| i64::try_from(value).ok())
                        .unwrap_or_default();
                    if let Err(error) = account_session_goal_from_report(
                        &task_state,
                        &controller_runtime,
                        &report,
                        goal_elapsed_seconds,
                    )
                    .await
                    {
                        eprintln!("[webui-service] session goal accounting failed: {error}");
                    }
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
                            eprintln!("[webui-service] apply final report failed: {error}");
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
                        spawn_service_task(async move {
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
                                    "[webui-service] append session terminal boundary failed: {error}"
                                );
                            }
                            webui_trace_if_slow(
                                "stream_terminal_lifecycle_finish_slow",
                                lifecycle_started_at,
                                format!("session={finished_session_id}"),
                            );
                        });
                    }
                    let started_next_prompt = maybe_start_webui_turn_chain_after_finished(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        finished_session_id,
                    )
                    .await;
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
                            eprintln!("[webui-service] persist final report failed: {error}");
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

/// Starts one detached child-task listener using the shared task runtime stream.
/// 使用共享任务运行时流启动单个分离子任务监听器。
fn maybe_start_webui_detached_subagent_task_listener(
    app: ServiceEventEmitter,
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
    spawn_service_task(async move {
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
                eprintln!("[webui-service] detached subagent event failed: {error}");
            }
        }
    });
    Ok(())
}

/// Applies one detached subagent runtime event to the shared WebUI transcript projection.
/// 将单条分离子代理运行时事件应用到共享 WebUI 转录投影。
async fn handle_webui_detached_subagent_task_event(
    app: &ServiceEventEmitter,
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
                    eprintln!("[webui-service] ingest subagent parent event failed: {error}");
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
            acknowledge_runtime_event_projection_barrier(&event);
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
                    eprintln!("[webui-service] enqueue session mailbox input failed: {error}");
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
                spawn_service_task(async move {
                    let snapshot = match current_workspace_snapshot_from_shared_state(
                        &state_for_persist,
                    ) {
                        Ok(Some(snapshot)) => snapshot,
                        Ok(None) => return,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] detached subagent persist snapshot failed: {error}"
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
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    record_ids: &mut SessionStreamingRecordIds,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
            record_webui_provider_lowering_repair(controller_runtime, session_id, event)
        }
        ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
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
            append_runtime_event_projection(app, state, session_id, &event)?;
            acknowledge_runtime_event_projection_barrier(&event);
            Ok(())
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
    app: ServiceEventEmitter,
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
                                "[webui-service] mailbox intervention prompt failed: {error}"
                            );
                            false
                        }
                    };
                pending_prompt_changed || mailbox_changed
            }
            Err(error) => {
                eprintln!("[webui-service] apply guided prompt intervention failed: {error}");
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

/// Prepares the next visible queued prompt after one WebUI-owned session run reaches a clean idle point.
/// 在 WebUI 拥有的单次会话运行到达干净空闲点后准备下一条可见排队提示词。
///
/// `allow_from_stopped_phase` lets manual insert match service behavior while automatic continuation still respects stop.
/// `allow_from_stopped_phase` 让手动插入对齐服务行为，同时自动续跑仍尊重停止状态。
/// Rebuilds and emits the retained workbench snapshot after a service-side queue mutation.
/// 在 service 侧队列变更后重建并发出保留的工作台快照。
fn store_current_workbench_snapshot(
    app: &ServiceEventEmitter,
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
    let (
        live_context_budgets,
        surface_messages,
        subagent_navigation,
        session_goal_summaries,
        hydrated_window_summaries,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
        (
            guard.live_context_budgets.clone(),
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot),
            guard.session_goal_summaries.clone(),
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
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
        Some(agent_runtime.command_session_runtime.as_ref()),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question_snapshot_from_agent_runtime(Some(agent_runtime)),
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
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
    emit_workbench_event(app, &workbench);
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
    app: &ServiceEventEmitter,
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
    emit_workbench_event(app, &workbench);
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
        workspace_snapshot,
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
            conversation_messages_from_session(conversation_id, surface_messages.as_slice(), false)
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
    app: &ServiceEventEmitter,
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
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    phase: SessionRunPhase,
) -> Result<(), String> {
    with_workspace_mutation(app, state, |workspace| {
        workspace.session_run_phases.insert(session_id, phase);
    })
    .map(|_| ())
}

/// Marks the in-flight prompt snapshot as preparing before slower outer steps run.
/// 在较慢的外层步骤执行前，将当前提示词快照标记为准备中。
fn mark_webui_prompt_snapshot_preparing(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    let now = current_unix_millis_text();
    let phase = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Preparing);
    // Preserve the explicit memory-recall phase until the precheck continuation promotes the turn to provider preparation.
    // 保留显式记忆召回阶段，直到 precheck continuation 将本轮推进到 Provider 准备阶段。
    if phase != SessionRunPhase::MemoryRecall {
        workspace_snapshot
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
    }
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
        session.focus = String::from("正在准备请求");
        session.last_updated_label = String::from("刚刚");
    }
}

/// Records one provider request checkpoint into the segmented session runtime audit log.
/// 将单个 Provider 请求检查点记录到分段会话运行时审计日志。
fn record_webui_provider_request_checkpoint(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    snapshot: &ProviderRequestSnapshot,
) -> Result<(), String> {
    let event = vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(
        vulcan_context::RuntimeProviderRequestCheckpointEvent {
            turn_id: None,
            checkpoint_id: format!(
                "provider-request-{}-{}",
                session_id,
                stable_provider_request_snapshot_hash(snapshot)
            ),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            provider_family: snapshot.provider_id.clone(),
            capability_fingerprint: None,
            prompt_fingerprint: Some(stable_provider_prompt_fingerprint(snapshot)),
            injection_fingerprint: None,
            request_fingerprint: Some(stable_provider_request_snapshot_hash(snapshot)),
            cache_policy_fingerprint: Some(stable_provider_cache_policy_fingerprint(snapshot)),
            cache_policy_diagnostic: provider_cache_policy_diagnostic(snapshot),
            converter_version: 1,
        },
    );
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        &controller_runtime.data_root,
        session_id,
        event,
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Records one provider lowering repair into the segmented session runtime audit log.
/// 将单个 Provider 转换修复记录到分段会话运行时审计日志。
fn record_webui_provider_lowering_repair(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    event: vulcan_context::RuntimeProviderLoweringRepairEvent,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        &controller_runtime.data_root,
        session_id,
        vulcan_context::RuntimeLogEvent::ProviderLoweringRepair(event),
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Records one context-injection merge event into the segmented session runtime audit log.
/// 将单个上下文注入合并事件记录到分段会话运行时审计日志。
fn record_webui_runtime_injection_merged(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionMergedEvent,
    timestamp: &str,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_injection_merged(
        &controller_runtime.data_root,
        session_id,
        event,
        timestamp,
    )
    .map(|_| ())
}

/// Records one context-injection cancellation event into the segmented session runtime audit log.
/// 将单个上下文注入取消事件记录到分段会话运行时审计日志。
fn record_webui_runtime_injection_cancelled(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    event: vulcan_context::RuntimeInjectionCancelledEvent,
    timestamp: &str,
) -> Result<(), String> {
    vulcan_workspace_store::workspace_store::record_session_runtime_injection_cancelled(
        &controller_runtime.data_root,
        session_id,
        event,
        timestamp,
    )
    .map(|_| ())
}

/// Records one provider response checkpoint into the segmented session runtime audit log.
/// 将单个 Provider 响应检查点记录到分段会话运行时审计日志。
fn record_webui_provider_response_checkpoint(
    controller_runtime: &ControllerRuntimeHandle,
    report: &ExecutionRunReport,
) -> Result<(), String> {
    let Some(preparation) = report.preparation.as_ref() else {
        return Ok(());
    };
    let finish_reason = if report.error.is_some() {
        "failed"
    } else {
        "completed"
    };
    let native_state = latest_provider_native_state_envelope_from_conversation_state(
        report.conversation_state.as_ref(),
        &preparation.model_blueprint.provider_id,
        &preparation.model_blueprint.model_id,
    );
    let event = vulcan_context::RuntimeLogEvent::ProviderResponseCheckpoint(
        vulcan_context::RuntimeProviderResponseCheckpointEvent {
            turn_id: preparation
                .boundary_events
                .first()
                .and_then(|event| event.turn_id.clone()),
            checkpoint_id: format!(
                "provider-response-{}-{}",
                report.session_id,
                stable_provider_response_checkpoint_hash(
                    preparation,
                    report.provider_request_count,
                    report.last_successful_provider_usage.as_ref(),
                    report.error.as_ref(),
                )
            ),
            provider_id: preparation.model_blueprint.provider_id.clone(),
            model_id: preparation.model_blueprint.model_id.clone(),
            provider_family: preparation.model_blueprint.api_family_key().to_string(),
            finish_reason: Some(finish_reason.to_string()),
            native_state,
            usage: report.last_successful_provider_usage.clone(),
        },
    );
    vulcan_workspace_store::workspace_store::record_session_runtime_audit_event(
        &controller_runtime.data_root,
        report.session_id,
        event,
        &current_unix_millis_text(),
    )
    .map(|_| ())
}

/// Builds a stable fingerprint for one provider request snapshot without persisting raw prompt text.
/// 为单个 Provider 请求快照构建稳定指纹，同时不持久化原始提示词正文。
fn stable_provider_request_snapshot_hash(snapshot: &ProviderRequestSnapshot) -> String {
    let serialized = serde_json::to_string(snapshot).unwrap_or_else(|_| {
        format!(
            "{}:{}:{}:{}",
            snapshot.provider_id,
            snapshot.model_id,
            snapshot.history.len(),
            snapshot.tool_schemas.len()
        )
    });
    let mut hasher = DefaultHasher::new();
    serialized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Builds a stable prompt fingerprint for provider request audit records.
/// 为 Provider 请求审计记录构建稳定提示词指纹。
fn stable_provider_prompt_fingerprint(snapshot: &ProviderRequestSnapshot) -> String {
    let mut hasher = DefaultHasher::new();
    snapshot.provider_id.hash(&mut hasher);
    snapshot.model_id.hash(&mut hasher);
    snapshot.system_prompt.hash(&mut hasher);
    snapshot.prompt.hash(&mut hasher);
    snapshot.history.len().hash(&mut hasher);
    snapshot.tool_schemas.len().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Builds a stable cache-policy fingerprint for one request without storing prompt bodies.
/// 为单个请求构建稳定缓存策略指纹，同时不存储提示词正文。
fn stable_provider_cache_policy_fingerprint(snapshot: &ProviderRequestSnapshot) -> String {
    let mut hasher = DefaultHasher::new();
    snapshot.provider_id.hash(&mut hasher);
    snapshot.model_id.hash(&mut hasher);
    (!snapshot.system_prompt.trim().is_empty()).hash(&mut hasher);
    snapshot.history.len().hash(&mut hasher);
    snapshot.tool_schemas.len().hash(&mut hasher);
    (!snapshot.prompt.trim().is_empty()).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Returns the cache-policy diagnostic for providers that cannot represent cache hints.
/// 返回无法表达缓存提示的 Provider 对应缓存策略诊断。
fn provider_cache_policy_diagnostic(snapshot: &ProviderRequestSnapshot) -> Option<String> {
    let has_cacheable_boundary = !snapshot.system_prompt.trim().is_empty()
        || !snapshot.prompt.trim().is_empty()
        || !snapshot.tool_schemas.is_empty()
        || !snapshot.history.is_empty();
    if has_cacheable_boundary
        && !snapshot
            .provider_id
            .to_ascii_lowercase()
            .contains("anthropic")
    {
        Some(String::from("cache_hints_unsupported"))
    } else {
        None
    }
}

/// Builds a stable hash for a provider response checkpoint without storing response bodies.
/// 为 Provider 响应检查点构建稳定哈希，且不存储响应正文。
fn stable_provider_response_checkpoint_hash(
    preparation: &ExecutionPreparation,
    provider_request_count: usize,
    usage: Option<&ProviderUsageSnapshot>,
    error: Option<&String>,
) -> String {
    let mut hasher = DefaultHasher::new();
    preparation.model_blueprint.provider_id.hash(&mut hasher);
    preparation.model_blueprint.model_id.hash(&mut hasher);
    preparation
        .model_blueprint
        .api_family_key()
        .hash(&mut hasher);
    preparation
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.as_deref())
        .hash(&mut hasher);
    provider_request_count.hash(&mut hasher);
    error.is_some().hash(&mut hasher);
    if let Some(error) = error {
        error.hash(&mut hasher);
    }
    if let Some(usage) = usage
        && let Ok(serialized_usage) = serde_json::to_string(usage)
    {
        serialized_usage.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Extracts the latest scoped provider-native state envelope from one terminal conversation state.
/// 从单个终态会话状态中提取最新的带作用域 Provider 原生状态 envelope。
fn latest_provider_native_state_envelope_from_conversation_state(
    state: Option<&vulcan_agent_runtime::conversation_protocol::SessionConversationState>,
    provider_id: &str,
    model_id: &str,
) -> Option<vulcan_context::RuntimeProviderNativeStateEnvelope> {
    state?
        .messages
        .iter()
        .rev()
        .filter_map(|message| {
            match &message.payload {
            vulcan_agent_runtime::conversation_protocol::SessionConversationPayload::Assistant {
                provider_message_state: Some(provider_message_state),
                ..
            } => serde_json::from_value::<vulcan_context::RuntimeProviderNativeStateEnvelope>(
                provider_message_state.clone(),
            )
            .ok(),
            _ => None,
        }
        })
        .find(|envelope| envelope.provider_id == provider_id && envelope.model_id == model_id)
}

/// Builds a stable fingerprint for persistent injection blocks without storing their content.
/// 为长期注入块构建稳定指纹，同时不保存注入正文。
fn persistent_injection_fingerprint(state: &ContextInjectionState) -> Option<String> {
    let view = state.resolve_view();
    if view.persistent_blocks.is_empty() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    view.persistent_blocks.hash(&mut hasher);
    Some(format!("{:016x}", hasher.finish()))
}

/// Returns stable non-content source identifiers for persistent injection items.
/// 返回长期注入内容项的稳定非正文来源标识。
fn persistent_injection_source_node_ids(state: &ContextInjectionState) -> Vec<String> {
    state
        .groups
        .values()
        .filter(|group| group.kind == InjectionKind::Persistent)
        .flat_map(|group| {
            group
                .items
                .values()
                .map(move |item| format!("{}:{}", group.tag, item.content_hash))
        })
        .collect()
}

/// Builds one runtime audit event for legacy VMM injection cleanup.
/// 为旧版 VMM 注入清理构建一条运行时审计事件。
fn legacy_vmm_injection_cancelled_event(
    previous_persistent_fingerprint: Option<String>,
    reason: &str,
) -> vulcan_context::RuntimeInjectionCancelledEvent {
    vulcan_context::RuntimeInjectionCancelledEvent {
        turn_id: None,
        cancellation_id: format!("{reason}-{}", current_unix_millis_text()),
        source_node_ids: vec![String::from("persistent:vmm_profile")],
        previous_persistent_fingerprint,
        previous_transient_fingerprint: None,
        reason: Some(reason.to_string()),
    }
}

/// Appends or extends one streaming transcript record.
/// 追加或扩展单条流式转录记录。
fn append_stream_delta_record(
    app: &ServiceEventEmitter,
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
    let conversation_id = session_id.to_string();
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let now = current_unix_millis_text();
        let is_new_message = record_id_slot.is_none();
        let run_phase = match record_kind {
            SessionMessageRecordKind::ThinkingOutput => SessionRunPhase::Thinking,
            _ => SessionRunPhase::Requesting,
        };
        let projected_entry = {
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
            let Some(message_id) = record_id_slot.as_deref() else {
                return Err(String::from("streaming record id was not materialized"));
            };
            let projected_entry = runtime_state
                .conversation_state
                .records
                .iter()
                .find(|record| record.id == message_id)
                .map(session_message_entry_from_conversation_record);
            runtime_state.updated_at = now.clone();
            projected_entry
        };
        if let Some(projected_entry) = projected_entry.as_ref()
            && projected_entry.is_visible_to_user()
            && !projected_entry.body.trim().is_empty()
        {
            let messages = workspace.session_messages.entry(session_id).or_default();
            if let Some(existing) = messages
                .iter_mut()
                .find(|message| message.id == projected_entry.id)
            {
                *existing = projected_entry.clone();
            } else {
                messages.push(projected_entry.clone());
            }
        }
        workspace.session_run_phases.insert(session_id, run_phase);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
        let Some(projected_entry) = projected_entry else {
            return Ok(());
        };
        let projected_message =
            conversation_message_from_session_entry(&conversation_id, &projected_entry);
        let initial_message = is_new_message.then(|| projected_message.clone());
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        if let Some(existing) = guard
            .workbench
            .messages
            .iter_mut()
            .find(|message| message.id == projected_message.id)
        {
            *existing = projected_message.clone();
        } else if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            guard.workbench.messages.push(projected_message.clone());
        }
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = SessionState::Running.label().to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = now.clone();
                break;
            }
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::AppendMessageBody(AppendMessageBodyDelta {
                conversation_id,
                message_id: projected_entry.id,
                body_delta: delta.to_string(),
                current_body: Some(projected_message.body.clone()),
                initial_message,
                run_phase: Some(run_phase.to_string()),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Returns whether the retained workbench currently displays one conversation surface.
/// 返回保留工作台当前是否正在展示指定会话界面。
fn workbench_surface_matches_conversation(
    workbench: &WorkbenchSnapshot,
    conversation_id: &str,
) -> bool {
    workbench.selected_conversation_id.as_deref() == Some(conversation_id)
        || workbench
            .subagent_navigation
            .as_ref()
            .is_some_and(|navigation| navigation.active_conversation_id == conversation_id)
}

/// Converts one canonical conversation record into the transcript entry projection.
/// 将单条规范会话记录转换为转录项投影。
fn session_message_entry_from_conversation_record(
    record: &SessionConversationRecord,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: record.id.clone(),
        kind: record.kind,
        record_kind: record.record_kind,
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Converts one session transcript entry into a WebUI conversation message.
/// 将单条会话转录项转换为 WebUI 会话消息。
fn conversation_message_from_session_entry(
    conversation_id: &str,
    message: &SessionMessageEntry,
) -> ConversationMessage {
    ConversationMessage {
        id: if message.id.is_empty() {
            format!("{}-{}", conversation_id, message.created_at)
        } else {
            message.id.clone()
        },
        conversation_id: conversation_id.to_string(),
        created_at: message.created_at.clone(),
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
        editable_user_prompt: false,
    }
}

/// Keeps the thinking row collapsed after its streaming phase completes.
/// 在思考流阶段完成后保持思考行折叠。
fn collapse_streaming_thinking(
    app: &ServiceEventEmitter,
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
    app: &ServiceEventEmitter,
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
    if let Some(goal_update) = goal_summary_from_tool_finished_event(event) {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        match goal_update {
            Some(summary) => {
                if let Ok(summary_session_id) = summary.session_id.parse::<SessionId>() {
                    guard
                        .session_goal_summaries
                        .insert(summary_session_id, summary);
                }
            }
            None => {
                guard.session_goal_summaries.remove(&session_id);
            }
        }
    }
    let mut changed_todolist: Option<SessionTodolistState> = None;
    let mut changed_plan_mode: Option<SessionPlanModeState> = None;
    let mut transcript_changed = false;
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let now = current_unix_millis_text();
            // marker_sync_required defers workspace-level marker updates until the runtime-state borrow ends.
            // marker_sync_required 将工作区级标记更新延后到运行态借用结束之后。
            let mut marker_sync_required = false;
            {
                let runtime_state = workspace
                    .session_runtime_state
                    .entry(session_id)
                    .or_insert_with(|| SessionRuntimeStateEntry {
                        updated_at: now.clone(),
                        ..SessionRuntimeStateEntry::default()
                    });
                if append_runtime_event_projection_to_state(runtime_state, session_id, &now, event)
                {
                    workspace.session_messages.insert(
                        session_id,
                        merge_session_messages_with_conversation_state_records(
                            workspace
                                .session_messages
                                .get(&session_id)
                                .map(Vec::as_slice)
                                .unwrap_or(&[]),
                            &runtime_state.conversation_state,
                        ),
                    );
                    marker_sync_required = true;
                    transcript_changed = true;
                }
                if let Some(todolist) = todolist_state_from_tool_finished_event(event, &now)
                    && runtime_state.todolist != todolist
                {
                    runtime_state.todolist = todolist.clone();
                    runtime_state.updated_at = now.clone();
                    changed_todolist = Some(todolist);
                }
                if let Some(artifact) = plan_artifact_state_from_tool_finished_event(event) {
                    let mut plan_mode = runtime_state.plan_mode.clone();
                    plan_mode.artifact = Some(artifact);
                    if runtime_state.plan_mode != plan_mode {
                        runtime_state.plan_mode = plan_mode.clone();
                        runtime_state.updated_at = now.clone();
                        changed_plan_mode = Some(plan_mode);
                    }
                }
            }
            if marker_sync_required {
                sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            }
        })?;
    if transcript_changed {
        let controller_runtime = controller_runtime_from_state(state)?;
        spawn_workspace_session_transcript_persist_task(
            state.clone(),
            controller_runtime,
            workspace_snapshot.clone(),
            session_id,
            Some(String::from("runtime_event_projection")),
        );
    }
    if let Some(todolist) = changed_todolist {
        let controller_runtime = controller_runtime_from_state(state)?;
        let updated_at = todolist
            .updated_at
            .clone()
            .unwrap_or_else(current_unix_millis_text);
        let mut addon_state = SessionAddonState::new(session_id);
        let _ = addon_state.set_todolist(&todolist, updated_at);
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot.clone(),
            session_id,
            addon_state,
            vec![SessionAddonNamespace::Todolist],
            Some(String::from("todolist_tool")),
        );
    }
    if let Some(plan_mode) = changed_plan_mode {
        let controller_runtime = controller_runtime_from_state(state)?;
        let updated_at = plan_mode
            .artifact
            .as_ref()
            .map(|artifact| artifact.updated_at.clone())
            .unwrap_or_else(current_unix_millis_text);
        let mut addon_state = SessionAddonState::new(session_id);
        let _ = addon_state.set_work_mode(
            &SessionWorkMode {
                execution_mode: String::from("plan"),
                plan_mode,
            },
            updated_at,
        );
        spawn_workspace_session_addon_state_persist_task(
            app.clone(),
            state.clone(),
            controller_runtime,
            workspace_snapshot,
            session_id,
            addon_state,
            vec![SessionAddonNamespace::WorkMode],
            Some(String::from("submit_plan_tool")),
        );
    }
    Ok(())
}

/// Caches the latest provider usage into the live context-budget snapshot.
/// 将最近一次 Provider 用量缓存进实时上下文预算快照。
fn cache_live_context_budget_observed_usage(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    usage: &ProviderUsageSnapshot,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(context_budget) = guard.live_context_budgets.get_mut(&session_id) else {
        return Ok(());
    };
    apply_observed_usage_to_context_budget(context_budget, usage);
    Ok(())
}

/// Applies one observed provider usage sample to an existing budget snapshot.
/// 将一份 Provider 实测用量应用到既有预算快照。
fn apply_observed_usage_to_context_budget(
    context_budget: &mut ContextBudgetSnapshot,
    usage: &ProviderUsageSnapshot,
) {
    let Some(input_tokens) =
        vulcan_context::observed_input_tokens_from_usage(Some(usage)).or(usage.total_tokens)
    else {
        context_budget.observed_usage = Some(usage.clone());
        return;
    };
    context_budget.estimated_input_tokens = input_tokens;
    context_budget.estimated_total_tokens = usage.total_tokens.unwrap_or_else(|| {
        input_tokens
            .saturating_add(usage.output_tokens.unwrap_or_default())
            .saturating_add(usage.reasoning_tokens.unwrap_or_default())
    });
    context_budget.usage_ratio = context_budget
        .effective_input_limit
        .map(|limit| input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or_else(|| if input_tokens > 0 { 0.01 } else { 0.0 });
    context_budget.severity = historical_context_budget_severity(context_budget.usage_ratio);
    context_budget.observed_usage = Some(usage.clone());
}

/// Builds the current Plan-mode developer segment and marks the first activation as injected.
/// 构建当前计划模式开发者提示段，并在首次激活时标记为已注入。
///
/// # Parameters
/// 参数
/// `app` emits addon updates if the marker changes.
/// `app` 在标记变化时发出附属状态更新。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `controller_runtime` persists the updated addon namespace.
/// `controller_runtime` 持久化更新后的附属命名空间。
/// `session_id` identifies the session being prepared.
/// `session_id` 标识正在准备的会话。
///
/// # Returns
/// 返回值
/// The Plan-mode developer segment whenever the session is currently in Plan mode.
/// 会话当前处于计划模式时返回计划模式开发者提示段。
fn session_plan_context_for_prompt_from_state(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<String>, String> {
    let updated_at = current_unix_millis_text();
    let mut context: Option<String> = None;
    let mut next_plan_mode: Option<SessionPlanModeState> = None;
    let mut next_execution_mode = String::from("build");
    let workspace_snapshot =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) else {
                return;
            };
            let is_plan = runtime_state
                .active_work_mode
                .as_deref()
                .is_some_and(|mode| mode.eq_ignore_ascii_case("plan"));
            if !is_plan {
                if runtime_state.plan_mode.pending_execution_notice {
                    runtime_state.plan_mode.pending_execution_notice = false;
                    runtime_state.updated_at = updated_at.clone();
                    next_plan_mode = Some(runtime_state.plan_mode.clone());
                    next_execution_mode = String::from("build");
                    context = Some(render_plan_mode_exit_developer_context());
                }
                return;
            }
            next_execution_mode = String::from("plan");
            context = Some(render_plan_mode_developer_context());
            if runtime_state.plan_mode.pending_execution_notice {
                runtime_state.plan_mode.pending_execution_notice = false;
                runtime_state.updated_at = updated_at.clone();
                next_plan_mode = Some(runtime_state.plan_mode.clone());
            }
            if runtime_state.plan_mode.prompt_injected {
                return;
            }
            runtime_state.plan_mode.prompt_injected = true;
            runtime_state.updated_at = updated_at.clone();
            next_plan_mode = Some(runtime_state.plan_mode.clone());
            next_execution_mode = String::from("plan");
        })?;
    let Some(plan_mode) = next_plan_mode else {
        return Ok(context);
    };
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_work_mode(
        &SessionWorkMode {
            execution_mode: next_execution_mode,
            plan_mode,
        },
        updated_at,
    );
    spawn_workspace_session_addon_state_persist_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        workspace_snapshot,
        session_id,
        addon_state,
        vec![SessionAddonNamespace::WorkMode],
        Some(String::from("plan_mode_prompt_injected")),
    );
    Ok(context)
}

/// Renders the Plan-mode developer guidance attached to each active Plan request.
/// 渲染附加到每次活跃计划模式请求的开发者引导。
///
/// # Returns
/// 返回值
/// English model-facing Plan-mode guidance adapted for Vulcan.
/// 面向模型的英文 Vulcan 计划模式引导。
fn render_plan_mode_developer_context() -> String {
    String::from(
        r#"# Plan Mode (Conversational)

You are in Plan Mode for this request. Work in three phases and chat your way to a decision-complete plan before finalizing it. A strong plan is detailed enough that another engineer or agent can implement it immediately without making additional product or architecture decisions.

## Mode Rules (Strict)

Plan Mode remains active for the current request until the host explicitly sends a non-plan request. User intent, imperative language, or phrases such as "start", "fix", "implement", or "do it" do not end Plan Mode.

If the user asks for execution while this request is still in Plan Mode, treat it as a request to plan the execution, not to perform it.

## Plan Mode vs. Progress Tools

Plan Mode is a collaboration mode for producing a decision-complete plan. It is not a checklist or progress state.

The `todolist` tool, when present, is a progress/todolist tool. It does not enter or exit Plan Mode. Do not use `todolist` while in Plan Mode. If you try to use it in Plan Mode, the host may reject the call.

## Plan Artifact Tool

When the `submit_plan` tool is available, use it as the official way to finalize a Plan Mode request. The tool saves the complete plan as a Markdown artifact and returns the final path.

Call `submit_plan` only when the plan is decision complete. Provide a short stable name and the full Markdown plan content. If you are revising a previously submitted plan, pass the original returned path, set `replace` to true, and provide the full replacement content. Do not submit partial patches or append-only updates.

After `submit_plan` succeeds, briefly tell the user that the plan was saved. Do not repeat the entire plan in assistant text unless the tool is unavailable.

## Execution vs. Mutation in Plan Mode

You may explore and execute non-mutating actions that improve the plan. You must not perform mutating actions.

Allowed non-mutating actions include:

* Reading or searching files, configs, schemas, types, manifests, and docs.
* Static analysis, inspection, and repository exploration.
* One-shot or interactive command-line exploration through tools such as `shell_command` or `terminal_session`, when the command is used only to inspect, test, or validate the plan. Before using `terminal_session`, call `skill("terminal-session")` and follow that skill.
* Dry-run style commands when they do not edit repo-tracked files.
* Tests, builds, or checks that may write to caches or build artifacts, as long as they do not edit repo-tracked files.

Not allowed mutating actions include:

* Editing, creating, deleting, moving, or writing files.
* Applying patches.
* Running formatters or linters that rewrite files.
* Running migrations, code generation, or scripts that update repo-tracked files.
* Staging, unstaging, reverting, committing, pushing, or otherwise changing Git state.
* Calling tools whose purpose is to perform the implementation instead of refining the plan.

When in doubt, if an action would reasonably be described as "doing the work" rather than "planning the work", do not do it.

The host may still expose the normal tool schema in Plan Mode to preserve request stability. Tool visibility does not mean mutation is allowed. If you call a mutating tool in Plan Mode, the host may reject the call and ask you to continue planning.

## Phase 1 - Ground in the Environment

Begin by grounding yourself in the actual environment. Resolve unknowns by discovering facts before asking the user. Perform at least one targeted non-mutating exploration pass when a local repository or relevant environment is available.

Use searches, file reads, manifests, configs, schemas, types, and existing implementation entry points to understand the current shape. Ask the user only after reasonable non-mutating exploration cannot answer a meaningful ambiguity.

Exception: you may ask before exploring only when the prompt itself contains an obvious contradiction or a preference question that cannot be discovered from the environment.

## Phase 2 - Intent Chat

Keep clarifying until you can state the goal, success criteria, audience, in-scope and out-of-scope boundaries, constraints, current state, and important preferences or tradeoffs.

Prefer questions over guessing when a high-impact ambiguity remains.

## Phase 3 - Implementation Chat

Once intent is stable, refine the implementation shape until the plan is decision complete. Cover the approach, interfaces, data flow, edge cases, failure modes, testing, acceptance criteria, compatibility constraints, and migration needs when relevant.

## Asking Questions

Ask only questions that materially change the plan, confirm an important assumption, or choose between meaningful tradeoffs.

Prefer using the available structured question tool when it is registered. In this environment, that tool may be named `ask`. If no structured question tool is available or the question cannot be expressed cleanly through it, ask directly in plain text.

Do not ask questions that can be answered by non-mutating exploration.

## Two Kinds of Unknowns

Treat discoverable facts and preferences differently.

Discoverable facts are repository or system truth. Explore first. Ask only if multiple plausible candidates remain, nothing relevant can be found, or the ambiguity is actually product intent.

Preferences and tradeoffs are not discoverable. Ask early when they materially affect the plan. If the user does not answer and a reasonable default exists, proceed with that default and record it as an assumption in the final plan.

## Finalization Rule

Only finalize the plan when it is decision complete and leaves no implementation decisions unresolved.

If `submit_plan` is available, finalize by calling `submit_plan` with the complete Markdown plan. Treat `<proposed_plan>` as a fallback only when the plan artifact tool is unavailable.

When presenting the official plan without `submit_plan`, wrap it in exactly one `<proposed_plan>` block:

1. Put the opening tag on its own line.
2. Start the plan content on the next line.
3. Put the closing tag on its own line.
4. Use Markdown inside the block.
5. Keep the tags exactly as `<proposed_plan>` and `</proposed_plan>`.

The final plan should be concise by default and include:

* A clear title.
* A brief summary.
* Key implementation changes or additions to public APIs, interfaces, or types.
* Test cases and acceptance scenarios.
* Explicit assumptions and defaults chosen where needed.

Prefer grouped implementation bullets by subsystem or behavior over file-by-file inventories. Mention files only when needed to disambiguate a non-obvious change. Avoid excessive detail unless it is needed to prevent implementation mistakes.

Do not ask "should I proceed?" in the final plan. The user can leave Plan Mode or request implementation after the plan artifact is submitted.

Only produce at most one `<proposed_plan>` block per request. If the user asks for revisions after a prior `<proposed_plan>`, the new `<proposed_plan>` must be a complete replacement."#,
    )
}

/// Renders the one-shot developer notice used after leaving Plan mode.
/// 渲染离开计划模式后使用的一次性开发者提示。
///
/// # Returns
/// 返回值
/// English model-facing guidance that normal execution has resumed.
/// 面向模型的英文提示，说明已恢复常规执行。
fn render_plan_mode_exit_developer_context() -> String {
    String::from(
        "Plan Mode has ended. You are now in execution mode. The previous Plan-mode mutation restrictions no longer apply; respond to the current user request using the normal execution workflow.",
    )
}

/// Updates the retained goal cache and emits a refreshed workbench snapshot.
/// 更新保留的目标缓存并发出刷新后的工作台快照。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` provides projection configuration.
/// `agent_runtime` 提供投影配置。
/// `luaskills_catalog` is the retained LuaSkills projection cache.
/// `luaskills_catalog` 是保留的 LuaSkills 投影缓存。
/// `token_budget_runtime` is the retained token-budget projection runtime.
/// `token_budget_runtime` 是保留的 Token 预算投影运行时。
/// `mcp_catalog` is the retained MCP projection cache.
/// `mcp_catalog` 是保留的 MCP 投影缓存。
/// `logs` is the startup log slice included in snapshots.
/// `logs` 是快照包含的启动日志片段。
/// `session_id` selects the goal cache row.
/// `session_id` 选择目标缓存行。
/// `goal` is the optional latest goal.
/// `goal` 是可选的最新目标。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot.
/// 刷新后的工作台快照。
fn refresh_session_goal_summary_and_emit(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    logs: &[String],
    session_id: SessionId,
    goal: Option<ThreadGoal>,
) -> Result<WorkbenchSnapshot, String> {
    let workspace_snapshot = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        match goal.as_ref().map(session_goal_summary_from_thread_goal) {
            Some(summary) => {
                guard.session_goal_summaries.insert(session_id, summary);
            }
            None => {
                guard.session_goal_summaries.remove(&session_id);
            }
        }
        guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?
            .clone()
    };
    store_and_emit_workbench_snapshot(
        &app,
        &state,
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        logs,
    )
}

/// Builds session todolist state from one completed `todolist` tool event.
/// 从单条已完成的 `todolist` 工具事件构建会话 todolist 状态。
///
/// # Parameters
/// 参数
/// `event` is the runtime event emitted after a tool call finishes.
/// `event` 是工具调用结束后发出的运行时事件。
/// `updated_at` is the timestamp assigned to the projected todolist.
/// `updated_at` 是分配给投影 todolist 的时间戳。
///
/// # Returns
/// 返回值
/// The parsed todolist when the event carries a matching host result.
/// 当事件携带匹配的宿主结果时返回已解析 todolist。
fn todolist_state_from_tool_finished_event(
    event: &RuntimeEvent,
    updated_at: &str,
) -> Option<SessionTodolistState> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    let payload = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| {
            (host_result.get("kind").and_then(Value::as_str) == Some("todolist"))
                .then(|| host_result.get("payload"))
                .flatten()
        })?;
    let items = payload
        .get("items")
        .and_then(Value::as_array)?
        .iter()
        .map(todolist_item_from_value)
        .collect::<Option<Vec<_>>>()?;
    let explanation = payload
        .get("explanation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(SessionTodolistState {
        explanation,
        items,
        updated_at: Some(updated_at.to_string()),
    })
}

/// Parses one host-result JSON value into a session todolist item.
/// 将单个宿主结果 JSON 值解析为会话 todolist 条目。
///
/// # Parameters
/// 参数
/// `value` is one raw todolist item emitted by the native tool.
/// `value` 是原生工具发出的单个原始 todolist 条目。
///
/// # Returns
/// 返回值
/// The parsed todolist item when both step and status are valid.
/// 当步骤与状态均有效时返回已解析 todolist 条目。
fn todolist_item_from_value(value: &Value) -> Option<SessionTodolistItem> {
    let step = value
        .get("step")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let status = match value.get("status").and_then(Value::as_str)? {
        "pending" => SessionTodolistItemStatus::Pending,
        "in_progress" => SessionTodolistItemStatus::InProgress,
        "completed" => SessionTodolistItemStatus::Completed,
        _ => return None,
    };
    Some(SessionTodolistItem { step, status })
}

/// Builds a Plan artifact state from one completed submit_plan tool event.
/// 从单条已完成 submit_plan 工具事件构建计划产物状态。
///
/// # Parameters
/// 参数
/// `event` is the runtime event emitted after a tool call finishes.
/// `event` 是工具调用结束后发出的运行时事件。
///
/// # Returns
/// 返回值
/// The parsed plan artifact when the event carries a valid host payload.
/// 当事件携带有效宿主载荷时返回解析后的计划产物。
fn plan_artifact_state_from_tool_finished_event(
    event: &RuntimeEvent,
) -> Option<SessionPlanArtifactState> {
    if event.kind != RuntimeEventKind::ToolFinished {
        return None;
    }
    let payload = event
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("host_result"))
        .and_then(|host_result| {
            (host_result.get("kind").and_then(Value::as_str) == Some("plan_artifact"))
                .then(|| host_result.get("payload"))
                .flatten()
        })?;
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let updated_at = payload
        .get("updatedAt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .filter(|value| !value.trim().is_empty());
    Some(SessionPlanArtifactState {
        name,
        path,
        content,
        updated_at,
    })
}

/// Acknowledges that one runtime event has been projected into the emitted WebUI snapshot.
/// 确认单条运行时事件已经投影进已发送的 WebUI 快照。
fn acknowledge_runtime_event_projection_barrier(event: &RuntimeEvent) {
    let acknowledged =
        vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(event);
    if acknowledged {
        webui_trace(
            "runtime_event_projection_barrier_ack",
            runtime_event_trace_detail(event),
        );
    }
}

/// Consumes one delegate runtime event into the WebUI subagent cache and child transcript state.
/// 将一条委派运行时事件吸收到 WebUI 子代理缓存与子会话转录状态中。
fn ingest_webui_subagent_runtime_event(
    app: &ServiceEventEmitter,
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
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
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
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
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

/// Loads parent task mirrors with a project-scoped lookup when the project is already known.
/// 在已知项目时使用项目范围查询加载父级任务镜像。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `project_id` optionally narrows the lookup to one known parent project.
/// `project_id` 可选将查询收窄到一个已知父项目。
/// `session_id` identifies the parent session whose task mirrors should be read.
/// `session_id` 标识需要读取任务镜像的父会话。
///
/// # Returns
/// 返回值
/// Lightweight task mirrors for the parent session, or an empty list when no segmented session exists.
/// 父会话的轻量任务镜像；没有分段会话时返回空列表。
fn load_segmented_session_task_mirrors_for_service(
    controller_runtime: &ControllerRuntimeHandle,
    project_id: Option<&str>,
    session_id: SessionId,
) -> Result<Vec<SessionTaskMirrorSnapshot>, String> {
    if let Some(project_id) = project_id.filter(|project_id| !project_id.trim().is_empty()) {
        return vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors_for_project(
            &controller_runtime.data_root,
            project_id,
            session_id,
        );
    }
    vulcan_workspace_store::workspace_store::load_segmented_session_task_mirrors(
        &controller_runtime.data_root,
        session_id,
    )
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
    let task_mirrors = load_segmented_session_task_mirrors_for_service(
        controller_runtime,
        owning_project_id_for_session(workspace_snapshot, parent_session_id).as_deref(),
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
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    messages: &mut [SessionMessageEntry],
) {
    if messages.is_empty() {
        return;
    }
    if !surface_contains_task_session_tool_card(messages.iter(), None) {
        return;
    }
    let Some(controller_runtime) = guard.controller_runtime.as_ref() else {
        return;
    };
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return;
    };
    let Ok(task_mirrors) = load_segmented_session_task_mirrors_for_service(
        controller_runtime,
        Some(project_id.as_str()),
        session_id,
    ) else {
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

/// Returns whether the current surface contains any visible task-session tool card.
/// 返回当前界面是否包含任何可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `messages` yields projected transcript messages that may contain tool-display metadata.
/// `messages` 产出可能包含工具展示元数据的投影转录消息。
/// `runtime_state` optionally carries canonical conversation records for the same surface.
/// `runtime_state` 可选携带同一界面的规范会话记录。
///
/// # Returns
/// 返回值
/// `true` when a visible tool row has parseable task-session metadata.
/// 当可见工具行拥有可解析的 task-session 元数据时返回 `true`。
fn surface_contains_task_session_tool_card<'a>(
    messages: impl Iterator<Item = &'a SessionMessageEntry>,
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> bool {
    messages
        .into_iter()
        .any(session_message_is_task_session_tool_card)
        || runtime_state.is_some_and(|runtime_state| {
            runtime_state
                .conversation_state
                .records
                .iter()
                .any(session_record_is_task_session_tool_card)
        })
}

/// Returns whether one projected message is a visible task-session tool card.
/// 返回一条投影消息是否是可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `message` is the projected transcript message to inspect.
/// `message` 是待检查的投影转录消息。
///
/// # Returns
/// 返回值
/// `true` when the message kind can display status and metadata has a task-session payload.
/// 当消息类型可展示状态且元数据包含 task-session 载荷时返回 `true`。
fn session_message_is_task_session_tool_card(message: &SessionMessageEntry) -> bool {
    task_session_record_kind_can_display_status(message.record_kind)
        && message
            .metadata
            .as_ref()
            .is_some_and(|metadata| task_session_payload_from_tool_metadata(metadata).is_some())
}

/// Returns whether one canonical record is a visible task-session tool card.
/// 返回一条规范记录是否是可见 task-session 工具卡片。
///
/// # Parameters
/// 参数
/// `record` is the canonical conversation record to inspect.
/// `record` 是待检查的规范会话记录。
///
/// # Returns
/// 返回值
/// `true` when the record kind can display status and metadata has a task-session payload.
/// 当记录类型可展示状态且元数据包含 task-session 载荷时返回 `true`。
fn session_record_is_task_session_tool_card(record: &SessionConversationRecord) -> bool {
    task_session_record_kind_can_display_status(record.record_kind)
        && record
            .metadata
            .as_ref()
            .is_some_and(|metadata| task_session_payload_from_tool_metadata(metadata).is_some())
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
    let runtime_state_snapshot = runtime_state.clone();
    merge_runtime_state_records_into_display_messages(
        workspace_snapshot,
        task_id,
        &runtime_state_snapshot,
    );
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, task_id);
}

/// Hides the selected parent subagent navigation round once service-equivalent blocking work has settled.
/// 当与服务运行时等价的阻塞工作全部收敛后，隐藏该轮子代理导航。
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
    app: &ServiceEventEmitter,
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
    app: &ServiceEventEmitter,
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
    app: &ServiceEventEmitter,
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
        pending_question,
        logs,
        live_context_budgets,
        session_goal_summaries,
        command_session_runtime,
        hydrated_window_summaries,
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
        let pending_question =
            pending_question_snapshot_from_agent_runtime(guard.agent_runtime.as_ref());
        let command_session_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| Arc::clone(&agent_runtime.command_session_runtime));
        let live_context_budgets = guard.live_context_budgets.clone();
        let session_goal_summaries = guard.session_goal_summaries.clone();
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
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &returned_workspace_snapshot,
        );
        let hydrated_window_summaries =
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows);
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
            pending_question,
            logs,
            live_context_budgets,
            session_goal_summaries,
            command_session_runtime,
            hydrated_window_summaries,
        )
    };
    let trace_workbench_started_at = current_unix_millis_value();
    let workbench = workbench_from_runtime(
        &controller_runtime,
        command_session_runtime.as_deref(),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question,
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
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
        emit_service_event_with_trace(
            app,
            "workspace_mutation_emit",
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
    let messages = display_session_messages_from_workspace(workspace_snapshot, task_state.task_id)
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

/// Refreshes the WebUI workbench after a focused workspace metadata update.
/// 在一次聚焦的工作区元数据更新后刷新 WebUI 工作台。
async fn refresh_workbench_snapshot(
    state: SharedServiceBridgeState,
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

    let mut refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );
    let selected_session_id = refreshed_snapshot.selected_session;
    if selected_session_id != SessionId::nil() {
        hydrate_selected_session_runtime_if_needed(
            controller_runtime.clone(),
            &mut refreshed_snapshot,
            selected_session_id,
        )
        .await?;
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
    sync_selected_session_goal_summary_from_store(
        &state,
        controller_runtime.clone(),
        selected_session_id,
    )
    .await?;
    let (surface_messages, subagent_navigation, session_goal_summaries, hydrated_window_summaries) = {
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
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &refreshed_snapshot,
        );
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &refreshed_snapshot),
            guard.session_goal_summaries.clone(),
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows),
        )
    };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        agent_runtime_for_status
            .as_ref()
            .map(|agent_runtime| agent_runtime.command_session_runtime.as_ref()),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &refreshed_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question_snapshot_from_agent_runtime(agent_runtime_for_status.as_ref()),
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
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
        "[webui-service][webui-trace] timestamp={} unix_ms={} stage={} {}",
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
            ExecutionStreamEvent::ProviderLoweringRepair { session_id, event },
        ) => format!(
            "session={session_id} provider={} model={} action={} affected_items={}",
            event.provider_id, event.model_id, event.action, event.affected_items
        ),
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
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderRequestCheckpoint { .. },
        ) => "child.provider_request_checkpoint",
        DetachedTaskRuntimeEvent::ChildExecution(
            ExecutionStreamEvent::ProviderLoweringRepair { .. },
        ) => "child.provider_lowering_repair",
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

/// Converts runtime state into a WebUI workbench snapshot.
/// 将运行时状态转换为 WebUI 工作台快照。
fn workbench_from_runtime(
    controller: &ControllerRuntimeHandle,
    command_session_runtime: Option<&CommandSessionRuntime>,
    luaskills_catalog: &TauriLuaSkillsCatalog,
    token_budget_runtime: &TauriTokenBudgetRuntime,
    mcp_catalog: &TauriMcpRuntimeCatalog,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    surface_messages: &[SessionMessageEntry],
    subagent_navigation: Option<SubagentNavigationSnapshot>,
    pending_question: Option<PendingQuestionSnapshot>,
    logs: &[String],
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
    session_goal_summaries: &BTreeMap<SessionId, SessionGoalSummary>,
    hydrated_conversation_windows: &[HydratedConversationWindowSummary],
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
    let mut messages = active_conversation_id
        .map(|conversation_id| {
            let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
                workspace,
                workspace.selected_session,
                subagent_navigation.as_ref(),
            );
            conversation_messages_from_session(
                conversation_id,
                surface_messages,
                allow_user_prompt_editing,
            )
        })
        .unwrap_or_default();
    append_hydrated_workspace_messages(&mut messages, workspace);
    let selected_runtime_state = workspace
        .session_runtime_state
        .get(&workspace.selected_session);
    let selected_ai_tool_configuration = selected_runtime_state
        .and_then(|runtime_state| runtime_state.ai_tool_configuration.as_ref())
        .unwrap_or(ai_tool_configuration);
    let ai_capability_model_capabilities =
        active_provider_configuration_model_capabilities(provider_configuration);
    let restorable_project_ids = workspace
        .task_projects
        .iter()
        .chain(workspace.archived_task_projects.iter())
        .map(|project| project.id.as_str())
        .collect::<BTreeSet<_>>();
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
                    fork_source: conversation_fork_source_summary_from_session(session),
                }
            })
            .collect(),
        archived_projects: workspace
            .archived_task_projects
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
        archived_conversations: workspace
            .archived_sessions
            .iter()
            .filter(|session| {
                session
                    .project_id
                    .as_deref()
                    .is_some_and(|project_id| restorable_project_ids.contains(project_id))
            })
            .filter(|session| session_segmented_metadata_exists(session))
            .map(|session| ConversationSummary {
                id: session.id.to_string(),
                title: session.title.clone(),
                status: String::from("Archived"),
                run_phase: SessionRunPhase::Idle.to_string(),
                last_updated_label: session.last_updated_label.clone(),
                last_updated_at: session.updated_at.clone(),
                unread: false,
                project_id: session.project_id.clone(),
                fork_source: conversation_fork_source_summary_from_session(session),
            })
            .collect(),
        selected_conversation_id,
        messages,
        hydrated_conversation_windows: hydrated_conversation_windows.to_vec(),
        conversation_hydration: None,
        subagent_navigation,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_luaskills(luaskills_catalog),
        lua_skills: lua_skills_catalog_summary(luaskills_catalog, provider_configuration),
        agent_skills: agent_skills_catalog_summary(provider_configuration),
        settings: settings(),
        providers: providers(provider_configuration),
        mcp_servers: mcp_servers(provider_configuration),
        reasoning_selection: reasoning_selection(provider_configuration),
        git_commit: git_commit_settings(provider_configuration),
        ai_capabilities: ai_capabilities(
            selected_ai_tool_configuration,
            ai_capability_model_capabilities.as_ref(),
        ),
        default_ai_capabilities: ai_capabilities(
            ai_tool_configuration,
            ai_capability_model_capabilities.as_ref(),
        ),
        ai_capability_vendors: ai_capability_vendors(selected_ai_tool_configuration),
        ai_capability_vendor_catalog: ai_capability_vendor_catalog(),
        composer_attachment_capabilities: composer_attachment_capabilities(
            provider_configuration,
            selected_ai_tool_configuration,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: selected_runtime_state
            .map(pending_prompt_summaries_from_runtime_state)
            .unwrap_or_default(),
        pending_question,
        session_todolist: session_todolist_summary_from_runtime_state(selected_runtime_state),
        session_goal: session_goal_summaries
            .get(&workspace.selected_session)
            .cloned(),
        session_plan_mode: session_plan_mode_summary_from_runtime_state(selected_runtime_state),
        memories: memories(workspace),
        vmm_memory: vmm_memory_snapshot(workspace, provider_configuration, selected_runtime_state),
        command_sessions: command_sessions(controller, command_session_runtime),
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
            provider_configuration,
            ai_tool_configuration,
        ),
    }
}

/// Projects session todolist state into the WebUI snapshot.
/// 将会话 todolist 状态投影到 WebUI 快照。
///
/// # Parameters
/// 参数
/// `runtime_state` is the selected session runtime state when available.
/// `runtime_state` 是可用时的当前选中会话运行态。
///
/// # Returns
/// 返回值
/// A todolist summary when the selected session has visible items.
/// 当前选中会话存在可见条目时返回 todolist 摘要。
fn session_todolist_summary_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<SessionTodolistSummary> {
    let todolist = &runtime_state?.todolist;
    if todolist.is_empty() || todolist.items.is_empty() {
        return None;
    }
    let items = todolist
        .items
        .iter()
        .map(|item| SessionTodolistItemSummary {
            step: item.step.clone(),
            status: session_todolist_item_status_key(item.status).to_string(),
        })
        .collect::<Vec<_>>();
    let completed_count = todolist
        .items
        .iter()
        .filter(|item| item.status == SessionTodolistItemStatus::Completed)
        .count();
    Some(SessionTodolistSummary {
        explanation: todolist.explanation.clone(),
        total_count: items.len(),
        completed_count,
        items,
        updated_at: todolist.updated_at.clone(),
    })
}

/// Converts selected-session Plan mode runtime state into a WebUI summary.
/// 将当前选中会话的计划模式运行态转换为 WebUI 摘要。
///
/// # Parameters
/// 参数
/// `runtime_state` is the optional selected-session runtime entry.
/// `runtime_state` 是可选的当前选中会话运行态条目。
///
/// # Returns
/// 返回值
/// The Plan mode summary when the selected session is in Plan mode or has a plan artifact.
/// 当选中会话处于计划模式或拥有计划产物时返回计划模式摘要。
fn session_plan_mode_summary_from_runtime_state(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> Option<SessionPlanModeSummary> {
    let runtime_state = runtime_state?;
    let execution_mode = runtime_state
        .active_work_mode
        .as_deref()
        .unwrap_or("build")
        .trim()
        .to_string();
    let is_plan = execution_mode.eq_ignore_ascii_case("plan");
    if !is_plan && runtime_state.plan_mode.is_empty() {
        return None;
    }
    Some(SessionPlanModeSummary {
        execution_mode,
        prompt_injected: runtime_state.plan_mode.prompt_injected,
        artifact: runtime_state
            .plan_mode
            .artifact
            .as_ref()
            .map(session_plan_artifact_summary_from_state),
    })
}

/// Converts one persisted Plan artifact state into a WebUI summary.
/// 将单个持久化计划产物状态转换为 WebUI 摘要。
///
/// # Parameters
/// 参数
/// `artifact` is the persisted artifact payload.
/// `artifact` 是持久化的产物载荷。
///
/// # Returns
/// 返回值
/// A WebUI-facing artifact summary with stable field names.
/// 字段名称稳定的 WebUI 产物摘要。
fn session_plan_artifact_summary_from_state(
    artifact: &SessionPlanArtifactState,
) -> SessionPlanArtifactSummary {
    SessionPlanArtifactSummary {
        name: artifact.name.clone(),
        path: artifact.path.clone(),
        content: artifact.content.clone(),
        updated_at: artifact.updated_at.clone(),
    }
}

/// Returns the stable WebUI key for one todolist item status.
/// 返回单个 todolist 条目状态的稳定 WebUI 键。
///
/// # Parameters
/// 参数
/// `status` is the internal todolist item lifecycle status.
/// `status` 是内部 todolist 条目生命周期状态。
///
/// # Returns
/// 返回值
/// The snake_case status key consumed by WebUI.
/// WebUI 消费的 snake_case 状态键。
fn session_todolist_item_status_key(status: SessionTodolistItemStatus) -> &'static str {
    match status {
        SessionTodolistItemStatus::Pending => "pending",
        SessionTodolistItemStatus::InProgress => "in_progress",
        SessionTodolistItemStatus::Completed => "completed",
    }
}

/// Projects the oldest pending ask-tool request from the shared runtime for WebUI display.
/// 从共享运行时投影最早的待处理 ask 工具请求供 WebUI 展示。
fn pending_question_snapshot_from_agent_runtime(
    agent_runtime: Option<&AgentRuntimeBridge>,
) -> Option<PendingQuestionSnapshot> {
    let question_runtime = agent_runtime.map(|runtime| runtime.question_runtime.as_ref())?;
    let mut pending = question_runtime.list_pending().ok()?;
    let request = pending.drain(..).next()?;
    Some(PendingQuestionSnapshot {
        id: request.id,
        session_id: request.session_id.to_string(),
        sequence: request.sequence,
        questions: request
            .questions
            .into_iter()
            .map(|question| {
                let multiple = question.allows_multiple();
                PendingQuestionPromptSummary {
                    question: question.question,
                    header: question.header,
                    options: question
                        .options
                        .into_iter()
                        .map(|option| PendingQuestionOptionSummary {
                            label: option.label,
                            description: option.description,
                        })
                        .collect(),
                    multiple,
                }
            })
            .collect(),
    })
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
    let static_profile = static_vmm_profile_snapshot(runtime_state);
    let mut static_cards = Vec::new();
    static_cards.extend(context_injection_cards_to_summary(
        context_injection_panel_cards_for_kind(
            &runtime_state.context_injection_state,
            Some(InjectionKind::Persistent),
        ),
    ));

    Some(ContextInjectionSnapshot {
        dynamic_cards,
        static_profile,
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
                    dedupe_key: item.dedupe_key,
                    memory_id: item
                        .memory_id
                        .or_else(|| context_injection_memory_id_from_detail(&item.detail)),
                    turn_id: item.turn_id,
                    score_label: item.score_label,
                    has_dialogue: item.has_dialogue,
                    created_datetime: item.created_datetime,
                    updated_at: item.updated_at,
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

/// Extracts a durable VMM memory id from rendered VMM panel detail text.
/// 从已渲染的 VMM 面板详情文本中提取长期 VMM 记忆标识。
fn context_injection_memory_id_from_detail(detail: &str) -> Option<u64> {
    detail.lines().find_map(|line| {
        let value = line.trim().strip_prefix("记忆 ID：")?;
        value
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|memory_id| *memory_id > 0)
    })
}

/// Builds the same compact multi-line preview used by the WebUI panel.
/// 构建与 WebUI 面板一致的多行紧凑预览。
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
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ContextUsageSnapshot> {
    let runtime_state = runtime_state?;
    let historical_context_budget;
    let context_budget = if let Some(context_budget) = live_context_budgets.get(&session_id) {
        Some(context_budget)
    } else {
        historical_context_budget = historical_context_budget_snapshot_from_runtime_state(
            runtime_state,
            provider_configuration,
            ai_tool_configuration,
        );
        historical_context_budget.as_ref()
    };
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
        tooltip_rows.push(String::from("请求拆分：暂无历史用量，等待预算引擎计算"));
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

/// Builds one historical context-budget snapshot from the last persisted provider usage.
/// 根据最近一次持久化 Provider 用量构建历史上下文预算快照。
///
/// # Parameters
/// 参数
/// `runtime_state` is the selected session runtime-state entry.
/// `runtime_state` 是当前选中会话的运行态条目。
/// `provider_configuration` is used to resolve the session model catalog entry.
/// `provider_configuration` 用于解析会话模型目录项。
/// `ai_tool_configuration` is used to resolve effective model capability overrides.
/// `ai_tool_configuration` 用于解析生效模型能力覆盖。
///
/// # Returns
/// 返回值
/// A best-effort context budget snapshot when historical usage exists.
/// 当存在历史用量时返回尽力而为的上下文预算快照。
fn historical_context_budget_snapshot_from_runtime_state(
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ContextBudgetSnapshot> {
    let observed_usage = runtime_state
        .last_successful_provider_usage
        .as_ref()
        .or(runtime_state.last_provider_usage.as_ref())
        .filter(|usage| usage.has_any_tokens())?;
    let provider_id = runtime_state.active_provider_id.as_deref()?;
    let model_id = runtime_state.active_model_id.as_deref()?;
    let capabilities = runtime_state_model_capabilities(
        runtime_state,
        provider_configuration,
        ai_tool_configuration,
    );
    let effective_input_limit = capabilities
        .as_ref()
        .and_then(|capabilities| capabilities.input_limit.or(capabilities.context_limit));
    let input_tokens = vulcan_context::observed_input_tokens_from_usage(Some(observed_usage))
        .or(observed_usage.total_tokens)
        .unwrap_or_default();
    let mut usage_ratio = effective_input_limit
        .map(|limit| input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or(0.0);
    if effective_input_limit.is_none() && input_tokens > 0 {
        usage_ratio = 0.01;
    }

    Some(ContextBudgetSnapshot {
        fingerprint: runtime_state
            .live_context_budget_fingerprint
            .clone()
            .unwrap_or_else(|| format!("historical:{provider_id}:{model_id}")),
        context_limit: capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.context_limit),
        effective_input_limit,
        reserved_output_tokens: capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.output_limit)
            .unwrap_or_default(),
        reserved_reasoning_tokens: 0,
        reserved_compaction_tokens: 0,
        compaction_auto_enabled: false,
        baseline_matches_active_runtime: true,
        baseline_budget_fingerprint_matches: true,
        baseline_assembly_fingerprint_matches: true,
        baseline_mismatch_reason: None,
        estimated_system_tokens: 0,
        estimated_history_tokens: input_tokens,
        estimated_prompt_tokens: 0,
        estimated_tool_tokens: 0,
        estimated_input_tokens: input_tokens,
        estimated_total_tokens: observed_usage.total_tokens.unwrap_or_else(|| {
            input_tokens
                .saturating_add(observed_usage.output_tokens.unwrap_or_default())
                .saturating_add(observed_usage.reasoning_tokens.unwrap_or_default())
        }),
        usage_ratio,
        observed_usage: Some(observed_usage.clone()),
        severity: historical_context_budget_severity(usage_ratio),
    })
}

/// Resolves model capabilities for the model recorded by one session runtime state.
/// 为单个会话运行态记录的模型解析模型能力。
///
/// # Parameters
/// 参数
/// `runtime_state` supplies the provider and model identity.
/// `runtime_state` 提供供应商与模型身份。
/// `provider_configuration` supplies enabled model catalog data.
/// `provider_configuration` 提供已启用模型目录数据。
/// `ai_tool_configuration` supplies capability overrides.
/// `ai_tool_configuration` 提供能力覆盖配置。
///
/// # Returns
/// 返回值
/// Effective model capabilities when the provider can be resolved.
/// 当供应商可解析时返回生效模型能力。
fn runtime_state_model_capabilities(
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ResolvedModelCapabilities> {
    let provider_id = runtime_state.active_provider_id.as_ref()?;
    let model_id = runtime_state.active_model_id.as_ref()?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .find(|model| model.id == *model_id);
    Some(resolve_effective_model_capabilities(
        definition,
        model.as_ref(),
        ai_tool_configuration,
    ))
}

/// Resolves base model capabilities for the model recorded by one session runtime state.
/// 为单个会话运行态记录的模型解析基础模型能力。
///
/// # Parameters
/// 参数
/// `runtime_state` supplies the provider and model identity.
/// `runtime_state` 提供供应商与模型身份。
/// `provider_configuration` supplies enabled model catalog data.
/// `provider_configuration` 提供已启用模型目录数据。
///
/// # Returns
/// 返回值
/// Base model capabilities before user AI-extension routing overlays are applied.
/// 返回应用用户 AI 扩展路由覆盖之前的基础模型能力。
fn runtime_state_base_model_capabilities(
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ResolvedModelCapabilities> {
    let provider_id = runtime_state.active_provider_id.as_ref()?;
    let model_id = runtime_state.active_model_id.as_ref()?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .find(|model| model.id == *model_id);
    Some(resolve_model_capabilities(definition, model.as_ref()))
}

/// Resolves base model capabilities for the globally selected provider model.
/// 为全局选中的供应商模型解析基础模型能力。
fn active_provider_configuration_model_capabilities(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ResolvedModelCapabilities> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .find(|model| model.id == model_id);
    Some(resolve_model_capabilities(definition, model.as_ref()))
}

/// Resolves one historical usage ratio into the shared context-budget severity buckets.
/// 将历史占用比例解析为共享上下文预算严重级别。
///
/// # Parameters
/// 参数
/// `usage_ratio` is the input-token usage ratio against the effective window.
/// `usage_ratio` 是输入 token 相对有效窗口的占用比例。
///
/// # Returns
/// 返回值
/// The broad context-budget severity bucket.
/// 返回粗粒度上下文预算严重级别。
fn historical_context_budget_severity(usage_ratio: f32) -> ContextBudgetSeverity {
    if usage_ratio >= vulcan_context::CONTEXT_BUDGET_OVERFLOW_RATIO {
        ContextBudgetSeverity::Overflow
    } else if usage_ratio >= vulcan_context::CONTEXT_BUDGET_COMPACTION_RATIO {
        ContextBudgetSeverity::CompactSoon
    } else if usage_ratio >= vulcan_context::CONTEXT_BUDGET_WARNING_RATIO {
        ContextBudgetSeverity::Warning
    } else {
        ContextBudgetSeverity::Healthy
    }
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
    allow_user_prompt_editing: bool,
) -> Vec<ConversationMessage> {
    let editable_user_prompt_ids = if allow_user_prompt_editing {
        editable_user_prompt_message_ids_from_entries(messages)
    } else {
        BTreeSet::new()
    };
    messages
        .iter()
        .filter(|message| message.is_visible_to_user())
        .filter(|message| {
            !message.body.trim().is_empty()
                || matches!(
                    message.record_kind,
                    SessionMessageRecordKind::ToolCallOutput
                        | SessionMessageRecordKind::ToolResultOutput
                        | SessionMessageRecordKind::TurnStats
                        | SessionMessageRecordKind::CompactionStatus
                )
        })
        .map(|message| {
            let mut projected = conversation_message_from_session_entry(conversation_id, message);
            projected.editable_user_prompt = editable_user_prompt_ids.contains(&message.id);
            projected
        })
        .collect()
}

/// Computes visible user prompt ids that have no later successful compaction or applied change-set blocker.
/// 计算后续没有成功压缩或已应用 change_set 阻断的可见用户提示词标识。
fn editable_user_prompt_message_ids_from_entries(
    messages: &[SessionMessageEntry],
) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for (index, message) in messages.iter().enumerate() {
        if !session_entry_allows_user_prompt_edit(message) {
            continue;
        }
        if messages[index + 1..]
            .iter()
            .any(session_entry_blocks_user_prompt_edit)
        {
            continue;
        }
        result.insert(message.id.clone());
    }
    result
}

/// Returns whether one workspace snapshot allows editing the selected user prompt.
/// 返回单个工作区快照是否允许编辑所选用户提示词。
fn workspace_user_prompt_message_allows_destructive_edit(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    message_id: &str,
) -> bool {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .is_some_and(|messages| {
            editable_user_prompt_message_ids_from_entries(messages).contains(message_id)
        })
}

/// Returns whether one user transcript entry is a normal editable turn opener.
/// 返回单条用户转录项是否是常规且可编辑的轮次开启项。
fn session_entry_allows_user_prompt_edit(message: &SessionMessageEntry) -> bool {
    if message.kind != SessionMessageKind::User
        || message.record_kind != SessionMessageRecordKind::Transcript
        || !message.visible
        || message.id.trim().is_empty()
    {
        return false;
    }
    message
        .metadata
        .as_ref()
        .is_none_or(|metadata| metadata_allows_user_prompt_edit(metadata))
}

/// Returns whether transcript metadata permits destructive user-prompt editing.
/// 返回转录元数据是否允许破坏性用户提示词编辑。
fn metadata_allows_user_prompt_edit(metadata: &Value) -> bool {
    let starts_user_turn = metadata
        .get("starts_user_turn")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let editable = metadata
        .get("editable")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    starts_user_turn && editable
}

/// Returns whether one later transcript entry makes earlier user-prompt editing unsafe.
/// 返回单条后续转录项是否会让更早的用户提示词编辑不安全。
fn session_entry_blocks_user_prompt_edit(message: &SessionMessageEntry) -> bool {
    message.record_kind == SessionMessageRecordKind::CompactionSummary
        || message
            .metadata
            .as_ref()
            .is_some_and(metadata_has_applied_change_set)
        || message
            .metadata
            .as_ref()
            .is_some_and(|metadata| task_session_payload_from_tool_metadata(metadata).is_some())
}

/// Returns whether transcript metadata contains an applied canonical `change_set` host result.
/// 返回转录 metadata 是否包含已应用的 canonical `change_set` 宿主结果。
fn metadata_has_applied_change_set(metadata: &Value) -> bool {
    let nested = metadata
        .get("host_result")
        .or_else(|| metadata.get("hostResult"))
        .filter(|host_result| {
            host_result
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "change_set")
        });
    let flattened =
        (metadata.get("kind").and_then(Value::as_str) == Some("change_set")).then_some(metadata);
    nested
        .or(flattened)
        .and_then(|host_result| host_result.get("payload").or(Some(host_result)))
        .and_then(|payload| payload.get("mode").and_then(Value::as_str))
        .is_some_and(|mode| mode == "applied")
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

/// Builds the WebUI fork-source summary for one session when metadata carries it.
/// 当元数据携带分叉来源时，为单个会话构建 WebUI 来源摘要。
fn conversation_fork_source_summary_from_session(
    session: &vulcan_workbench_model::SessionSummary,
) -> Option<ConversationForkSourceSummary> {
    let project_id = session.project_id.as_deref()?;
    let source = load_segmented_session_fork_source_for_project(
        &vulcan_workspace_store::data_plane::application_data_root(),
        project_id,
        session.id,
    )
    .ok()
    .flatten()?;
    Some(ConversationForkSourceSummary {
        source_project_tag: source.source_project_tag,
        source_conversation_id: source.source_session_id,
        source_title: source.source_title,
        anchor_message_id: source.anchor_message_id,
        created_at: source.created_at,
    })
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
            path: normalized_workspace_path_text_from_path(&workspace_root),
            pinned: false,
            icon_pack: None,
            icon_key: None,
        }],
        expanded_project_ids: vec![String::from("current-workspace")],
        conversations: Vec::new(),
        archived_projects: Vec::new(),
        archived_conversations: Vec::new(),
        selected_conversation_id: None,
        messages: Vec::new(),
        hydrated_conversation_windows: Vec::new(),
        conversation_hydration: None,
        subagent_navigation: None,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: Vec::new(),
        lua_skills: lua_skills_catalog_summary(
            &empty_luaskills_catalog(),
            &ProviderConfigurationFile::default(),
        ),
        agent_skills: agent_skills_catalog_summary(&ProviderConfigurationFile::default()),
        settings: settings(),
        providers: providers(&ProviderConfigurationFile::default()),
        mcp_servers: mcp_servers(&ProviderConfigurationFile::default()),
        reasoning_selection: reasoning_selection(&ProviderConfigurationFile::default()),
        git_commit: git_commit_settings(&ProviderConfigurationFile::default()),
        ai_capabilities: ai_capabilities(&ProviderConfigurationFile::default().ai_tools, None),
        default_ai_capabilities: ai_capabilities(
            &ProviderConfigurationFile::default().ai_tools,
            None,
        ),
        ai_capability_vendors: ai_capability_vendors(
            &ProviderConfigurationFile::default().ai_tools,
        ),
        ai_capability_vendor_catalog: ai_capability_vendor_catalog(),
        composer_attachment_capabilities: composer_attachment_capabilities(
            &ProviderConfigurationFile::default(),
            &ProviderConfigurationFile::default().ai_tools,
        ),
        composer_slash_commands: vulcan_service_api::composer_slash_command_catalog(),
        pending_prompts: Vec::new(),
        pending_question: None,
        session_todolist: None,
        session_goal: None,
        session_plan_mode: None,
        memories: Vec::new(),
        vmm_memory: VmmMemorySnapshot::default(),
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

/// Appends retained hydrated conversation bodies that are not already present in one WebUI snapshot.
/// 追加尚未出现在 WebUI 快照中的已保留水合会话正文。
fn append_hydrated_workspace_messages(
    messages: &mut Vec<ConversationMessage>,
    workspace: &WorkspaceSnapshot,
) {
    let mut projected_conversation_ids = messages
        .iter()
        .map(|message| message.conversation_id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    for session_id in &workspace.hydrated_session_ids {
        let conversation_id = session_id.to_string();
        if projected_conversation_ids.contains(&conversation_id) {
            continue;
        }
        let Some(session_messages) = workspace.session_messages.get(session_id) else {
            continue;
        };
        messages.extend(conversation_messages_from_session(
            &conversation_id,
            session_messages,
            false,
        ));
        projected_conversation_ids.insert(conversation_id);
    }
}

/// Creates feature areas matching the WebUI navigation surface.
/// 创建与 WebUI 导航面相匹配的功能区域。
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

/// Projects the retained LuaSkills catalog into the WebUI service API shape.
/// 将保留的 LuaSkills 目录投影为 WebUI service API 结构。
fn lua_skills_catalog_summary(
    catalog: &TauriLuaSkillsCatalog,
    provider_configuration: &ProviderConfigurationFile,
) -> LuaSkillsCatalogSummary {
    LuaSkillsCatalogSummary {
        runtime_root: normalized_workspace_path_text_from_path(&catalog.runtime_root),
        system_lua_lib: normalized_workspace_path_text_from_path(&catalog.system_lua_lib_dir),
        entry_count: catalog.entry_count,
        notice: if catalog.runtime_root.as_os_str().is_empty() {
            String::from("LuaSkills 技能目录尚未载入。")
        } else {
            format!(
                "可见技能 {} 个，运行时入口 {} 个。",
                catalog.skills.len(),
                catalog.entry_count
            )
        },
        skills: catalog
            .skills
            .iter()
            .map(|skill| LuaSkillSummary {
                skill_id: skill.skill_id.clone(),
                display_name: skill.display_name.clone(),
                version: skill.version.clone(),
                root_name: skill.root_name.clone(),
                skill_dir: normalized_workspace_path_text_from_path(&skill.skill_dir),
                enabled: skill.enabled,
                disabled_reason: skill.disabled_reason.clone(),
                managed: skill.managed,
                source_type: skill.source_type.clone(),
                source_locator: skill.source_locator.clone(),
                entry_count: skill.entry_count,
                system_owned: skill.system_owned,
                tools: skill
                    .tools
                    .iter()
                    .map(|tool| {
                        let state = provider_configuration
                            .luaskills
                            .tool_registration_state(&skill.skill_id, &tool.local_name);
                        LuaSkillToolSummary {
                            canonical_name: tool.canonical_name.clone(),
                            local_name: tool.local_name.clone(),
                            description: tool.description.clone(),
                            registration_state: luaskill_tool_registration_state_key(state)
                                .to_string(),
                            registration_label: state.to_string(),
                            registration_help: luaskill_tool_registration_state_help(state)
                                .to_string(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Projects prompt-based Agent Skills into the WebUI service API shape.
/// 将基于提示词的 Agent Skills 投影为 WebUI service API 结构。
fn agent_skills_catalog_summary(
    provider_configuration: &ProviderConfigurationFile,
) -> AgentSkillsCatalogSummary {
    let runtime_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &provider_configuration.prompt_skills,
    );
    let catalog = vulcan_prompt_skills::load_prompt_skills(&runtime_config);
    let global_enabled = provider_configuration.prompt_skills.enabled;
    AgentSkillsCatalogSummary {
        global_enabled,
        system_root: normalized_workspace_path_text_from_path(&catalog.system_root),
        user_root: normalized_workspace_path_text_from_path(&catalog.user_root),
        notice: format!(
            "系统技能 {}，用户技能 {}，加载警告 {}。",
            catalog
                .skills
                .iter()
                .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::System)
                .count(),
            catalog
                .skills
                .iter()
                .filter(|skill| skill.scope == vulcan_prompt_skills::PromptSkillScope::User)
                .count(),
            catalog.errors.len()
        ),
        skills: catalog
            .skills
            .iter()
            .map(|skill| agent_skill_summary(skill, provider_configuration, global_enabled))
            .collect(),
        errors: catalog
            .errors
            .iter()
            .map(|error| AgentSkillLoadIssueSummary {
                path: normalized_workspace_path_text_from_path(&error.path),
                message: error.message.clone(),
            })
            .collect(),
    }
}

/// Projects one prompt skill into a compact WebUI card row.
/// 将单个提示词技能投影为紧凑 WebUI 卡片行。
fn agent_skill_summary(
    skill: &vulcan_prompt_skills::PromptSkill,
    provider_configuration: &ProviderConfigurationFile,
    global_enabled: bool,
) -> AgentSkillSummary {
    let local_enabled = provider_configuration
        .prompt_skills
        .skill_overrides
        .get(&skill.name)
        .copied()
        .unwrap_or(provider_configuration.prompt_skills.default_skill_state)
        .is_enabled();
    AgentSkillSummary {
        name: skill.name.clone(),
        display_name: prompt_skill_display_name(skill).to_string(),
        description: prompt_skill_short_description(skill).to_string(),
        scope: skill.scope.zh_label().to_string(),
        enabled: global_enabled && local_enabled,
        skill_file: normalized_workspace_path_text_from_path(&skill.skill_file),
        base_dir: normalized_workspace_path_text_from_path(&skill.base_dir),
        icon_path: prompt_skill_icon_path(skill),
        body: skill.body.trim().to_string(),
    }
}

/// Returns the display name for one prompt skill.
/// 返回单个提示词技能的显示名称。
fn prompt_skill_display_name(skill: &vulcan_prompt_skills::PromptSkill) -> &str {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.display_name.as_deref())
        .unwrap_or(skill.name.as_str())
}

/// Returns the short description for one prompt skill.
/// 返回单个提示词技能的简短说明。
fn prompt_skill_short_description(skill: &vulcan_prompt_skills::PromptSkill) -> &str {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.short_description.as_deref())
        .filter(|description| !description.trim().is_empty())
        .or_else(|| (!skill.description.trim().is_empty()).then_some(skill.description.as_str()))
        .unwrap_or("该技能没有提供描述。")
}

/// Resolves an optional prompt skill icon path when it exists on disk.
/// 在图标文件存在时解析可选提示词技能图标路径。
fn prompt_skill_icon_path(skill: &vulcan_prompt_skills::PromptSkill) -> Option<String> {
    skill
        .interface
        .as_ref()
        .and_then(|interface| interface.icon_small.as_ref())
        .map(|path| skill.base_dir.join(path))
        .filter(|path| path.is_file())
        .map(|path| normalized_workspace_path_text_from_path(&path))
}

/// Returns a stable key for one persisted LuaSkills tool registration state.
/// 返回单个持久化 LuaSkills 工具注册状态的稳定键。
fn luaskill_tool_registration_state_key(
    state: PersistedLuaSkillToolRegistrationState,
) -> &'static str {
    match state {
        PersistedLuaSkillToolRegistrationState::Explicit => "explicit",
        PersistedLuaSkillToolRegistrationState::Implicit => "implicit",
        PersistedLuaSkillToolRegistrationState::Disabled => "disabled",
    }
}

/// Returns the help text shown for one LuaSkills tool registration state.
/// 返回单个 LuaSkills 工具注册状态展示的说明文本。
fn luaskill_tool_registration_state_help(
    state: PersistedLuaSkillToolRegistrationState,
) -> &'static str {
    match state {
        PersistedLuaSkillToolRegistrationState::Explicit => {
            "显式启用：工具会作为独立函数 Schema 暴露给模型。"
        }
        PersistedLuaSkillToolRegistrationState::Implicit => {
            "隐式启用：工具保留宿主调用能力，但不暴露独立 Schema。"
        }
        PersistedLuaSkillToolRegistrationState::Disabled => {
            "禁用：工具不会注册给模型，也不应被宿主调度调用。"
        }
    }
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
    let mut providers = provider_config::supported_providers()
        .iter()
        .map(|definition| {
            let configured = provider_configuration.provider.contains_key(&definition.id);
            let enabled_ids =
                provider_config::enabled_model_ids(&definition.id, provider_configuration);
            let catalog_models =
                provider_config::catalog_model_choices(&definition.id, provider_configuration)
                    .into_iter()
                    .map(|model_choice| {
                        let catalog_model_id = model_choice.id;
                        let reasoning_profile_json = model_choice
                            .reasoning_profile
                            .as_ref()
                            .and_then(|profile| serde_json::to_string_pretty(profile).ok());
                        let active = active_model_reference.as_ref().is_some_and(
                            |(active_provider_id, active_model_id)| {
                                active_provider_id == &definition.id
                                    && active_model_id == &catalog_model_id
                            },
                        );
                        let secondary_route = (definition.id == "openrouter")
                            .then(|| {
                                provider_config::openrouter_endpoint_route_for_model(
                                    provider_configuration,
                                    &catalog_model_id,
                                )
                            })
                            .flatten()
                            .map(|selection| {
                                provider_secondary_route_summary_from_openrouter_selection(
                                    &definition.id,
                                    &catalog_model_id,
                                    selection,
                                    active,
                                )
                            });
                        ProviderModelSummary {
                            enabled: configured && enabled_ids.contains(&catalog_model_id),
                            id: catalog_model_id.clone(),
                            name: model_choice.name,
                            active,
                            status: model_choice.status,
                            family: model_choice.family,
                            context_limit: model_choice.context_limit,
                            input_limit: model_choice.input_limit,
                            output_limit: model_choice.output_limit,
                            reasoning: model_choice.reasoning,
                            attachment: model_choice.attachment,
                            input_modalities: model_choice.input_modalities,
                            output_modalities: model_choice.output_modalities,
                            reasoning_profile_json,
                            supports_secondary_routes: definition.id == "openrouter",
                            secondary_route,
                        }
                    })
                    .collect::<Vec<_>>();
            let models = catalog_models
                .iter()
                .filter(|model| model.enabled)
                .cloned()
                .collect::<Vec<_>>();
            let active = models.iter().any(|model| model.active);
            ProviderSummary {
                id: definition.id.clone(),
                name: definition.name.clone(),
                status: if configured { "已配置" } else { "未配置" }.to_string(),
                configured,
                enabled_models: format!("{} 个已启用模型", models.len()),
                active,
                models,
                catalog_models,
                fields: provider_fields(definition, provider_configuration),
            }
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    providers
}

/// Creates editable provider field summaries for one provider definition.
/// 为单个供应商定义创建可编辑字段摘要。
fn provider_fields(
    definition: &provider_config::SupportedProviderDefinition,
    provider_configuration: &ProviderConfigurationFile,
) -> Vec<ProviderFieldSummary> {
    let options = provider_configuration
        .provider
        .get(&definition.id)
        .map(|entry| entry.options.clone())
        .unwrap_or_default();
    definition
        .input_fields()
        .into_iter()
        .map(|field| ProviderFieldSummary {
            key: provider_field_request_key(field).to_string(),
            label: definition.field_label(field).to_string(),
            placeholder: definition.field_placeholder(field),
            value: options.get(field),
            secret: field.is_secret(),
            required: true,
        })
        .collect()
}

/// Creates MCP server summaries from the shared provider configuration.
/// 从共享 Provider 配置创建 MCP 服务器摘要。
fn mcp_servers(provider_configuration: &ProviderConfigurationFile) -> Vec<McpServerSummary> {
    provider_configuration
        .mcp
        .servers
        .iter()
        .map(|(server_id, entry)| McpServerSummary {
            id: server_id.clone(),
            name: entry.name.clone(),
            enabled: entry.enabled,
            transport: entry.transport.label().to_string(),
            command: entry.command.clone(),
            args: entry.args.clone(),
            url: entry.url.clone().unwrap_or_default(),
            bearer_token: entry.bearer_token.clone().unwrap_or_default(),
            headers: key_value_summaries(&entry.headers),
            headers_from_env: key_value_summaries(&entry.headers_from_env),
            oauth_client_id: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_id.clone())
                .unwrap_or_default(),
            oauth_client_secret: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_secret.clone())
                .unwrap_or_default(),
            oauth_scopes: entry
                .oauth
                .as_ref()
                .map(|oauth| oauth.scopes.clone())
                .unwrap_or_default(),
            oauth_redirect_uri: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.redirect_uri.clone())
                .unwrap_or_default(),
            cwd: entry.cwd.clone().unwrap_or_default(),
            env: key_value_summaries(&entry.env),
            env_passthrough: entry.env_passthrough.clone(),
        })
        .collect()
}

/// Converts one ordered map into WebUI key/value summaries.
/// 将单个有序映射转换为 WebUI 键值摘要。
fn key_value_summaries(values: &BTreeMap<String, String>) -> Vec<KeyValueSummary> {
    values
        .iter()
        .map(|(key, value)| KeyValueSummary {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
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
            detail: String::from("WebUI 可读取真实项目、会话索引与插件信息。"),
        },
    ]
}

/// Creates command-session summaries for currently retained service backends.
/// 为当前保留的服务后端创建命令会话摘要。
fn command_sessions(
    controller: &ControllerRuntimeHandle,
    command_session_runtime: Option<&CommandSessionRuntime>,
) -> Vec<CommandSessionSummary> {
    let mut summaries = command_session_runtime
        .map(|runtime| {
            runtime
                .try_snapshot_sessions()
                .into_iter()
                .map(command_session_summary_from_snapshot)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    summaries.sort_by(|left, right| {
        left.name.cmp(&right.name).then(
            left.command
                .cmp(&right.command)
                .then(left.id.cmp(&right.id)),
        )
    });
    summaries.push(CommandSessionSummary {
        id: String::from("vldb-controller"),
        name: Some(String::from("VLDB Controller")),
        command: controller.executable.clone(),
        status: String::from("running"),
        preview: format!(
            "endpoint={} sqlite={}",
            controller.endpoint,
            controller.sqlite_path.display()
        ),
        owner_session_id: None,
        cwd: None,
        shell_label: None,
        backend: None,
        screen_frame_seq: None,
        screen_rows: None,
        screen_cols: None,
        screen_text: None,
        exit_code: None,
        failure_reason: None,
        lost: false,
    });
    summaries
}

/// Converts one live command-session snapshot into the WebUI summary contract.
/// 将单条实时命令会话快照转换为 WebUI 摘要契约。
fn command_session_summary_from_snapshot(
    snapshot: vulcan_agent_runtime::tooling::command_session_runtime::CommandSessionSnapshot,
) -> CommandSessionSummary {
    let preview = format!(
        "stdout={} stderr={} last_seq={}",
        snapshot.stdout_bytes, snapshot.stderr_bytes, snapshot.last_seq
    );
    CommandSessionSummary {
        id: snapshot.command_session_id.to_string(),
        name: snapshot.display_name,
        command: snapshot.command,
        status: snapshot.status.as_str().to_string(),
        preview,
        owner_session_id: Some(snapshot.owner_session_id.to_string()),
        cwd: Some(snapshot.cwd.display().to_string()),
        shell_label: Some(snapshot.shell_label),
        backend: Some(snapshot.backend.as_str().to_string()),
        screen_frame_seq: snapshot.screen_frame_seq,
        screen_rows: snapshot.screen_rows,
        screen_cols: snapshot.screen_cols,
        screen_text: snapshot.screen_text,
        exit_code: snapshot.exit_code,
        failure_reason: snapshot.failure_reason,
        lost: false,
    }
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

/// Default unified-diff context lines used by Git when no expansion is requested.
/// 未请求展开时 Git 默认使用的 unified-diff 上下文行数。
const WEBUI_DIFF_DEFAULT_CONTEXT_LINES: usize = 3;

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
    use crate::git::{
        WebuiWorkspaceDiffMode, run_webui_git_command, webui_diff_hunk_scoped_unified_diff,
        webui_unified_diff_file_summaries, webui_unified_diff_line_count, webui_unified_diff_stats,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use vulcan_agent_runtime::conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload,
        conversation_state_to_compaction_source_messages,
    };
    use vulcan_agent_runtime::provider_capabilities::ResolvedAiToolCapability;
    use vulcan_provider_config::config::{
        AiToolPolicy, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    };
    use vulcan_workbench_model::SessionTurnUsageSegmentKind;

    /// Verifies todolist host results are projected into session runtime todolist state.
    /// 验证 todolist 宿主结果会投影为会话运行态 todolist 状态。
    #[test]
    fn todolist_state_from_tool_finished_event_parses_host_result() {
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "todolist finished",
            "Todolist updated",
        )
        .with_metadata(Some(json!({
            "tool_name": "todolist",
            "tool_call_id": "call-todo",
            "host_result": {
                "kind": "todolist",
                "payload": {
                    "explanation": "同步任务列表",
                    "items": [
                        { "step": "阅读参考实现", "status": "completed" },
                        { "step": "接入右侧面板", "status": "in_progress" },
                        { "step": "补充验证", "status": "pending" }
                    ]
                }
            }
        })));

        let state = todolist_state_from_tool_finished_event(&event, "2026-06-17T00:00:00Z")
            .expect("todo host result should parse");

        assert_eq!(state.explanation.as_deref(), Some("同步任务列表"));
        assert_eq!(state.updated_at.as_deref(), Some("2026-06-17T00:00:00Z"));
        assert_eq!(state.items.len(), 3);
        assert_eq!(state.items[0].status, SessionTodolistItemStatus::Completed);
        assert_eq!(state.items[1].status, SessionTodolistItemStatus::InProgress);
        assert_eq!(state.items[2].status, SessionTodolistItemStatus::Pending);
    }

    /// Verifies user-prompt edit gating blocks applied file changes and child TaskSession cards.
    /// 验证用户输入编辑门禁会阻断已应用文件变更与子代理 TaskSession 卡片。
    #[test]
    fn user_prompt_edit_gate_blocks_applied_changes_and_task_sessions() {
        let first_user = SessionMessageEntry::new(SessionMessageKind::User, "", "first")
            .with_runtime_metadata("user-1", "1780000000000");
        let mut applied_change = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "调用 vulcan_file_edit 工具",
            "applied",
        )
        .with_runtime_metadata("tool-change", "1780000000001");
        applied_change.record_kind = SessionMessageRecordKind::ToolResultOutput;
        applied_change.metadata = Some(json!({
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "mode": "applied"
                }
            }
        }));
        let second_user = SessionMessageEntry::new(SessionMessageKind::User, "", "second")
            .with_runtime_metadata("user-2", "1780000000002");
        let task_message = test_spawn_agent_tool_message(
            SessionId::new_v4(),
            "completed",
            SessionMessageRecordKind::ToolResultOutput,
        );
        let third_user = SessionMessageEntry::new(SessionMessageKind::User, "", "third")
            .with_runtime_metadata("user-3", "1780000000003");

        let ids = editable_user_prompt_message_ids_from_entries(&[
            first_user,
            applied_change,
            second_user,
            task_message,
            third_user,
        ]);

        assert!(!ids.contains("user-1"));
        assert!(!ids.contains("user-2"));
        assert!(ids.contains("user-3"));
    }

    /// Verifies preview-only change sets do not block user-prompt editing.
    /// 验证仅预览的变更集不会阻断用户输入编辑。
    #[test]
    fn user_prompt_edit_gate_allows_preview_only_change_sets() {
        let user = SessionMessageEntry::new(SessionMessageKind::User, "", "first")
            .with_runtime_metadata("user-preview", "1780000000000");
        let mut preview_change = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "调用 vulcan_file_edit 工具",
            "preview",
        )
        .with_runtime_metadata("tool-preview", "1780000000001");
        preview_change.record_kind = SessionMessageRecordKind::ToolResultOutput;
        preview_change.metadata = Some(json!({
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "mode": "preview"
                }
            }
        }));

        let ids = editable_user_prompt_message_ids_from_entries(&[user, preview_change]);

        assert!(ids.contains("user-preview"));
    }

    /// Verifies interrupt prompt records are folded into the turn and cannot be edited.
    /// 验证插队提示词记录会折叠进轮次过程且不可编辑。
    #[test]
    fn user_prompt_edit_gate_rejects_interrupt_prompt_records() {
        let regular_user = SessionMessageEntry::new(SessionMessageKind::User, "", "regular")
            .with_runtime_metadata("user-regular", "1780000000000");
        let mut interrupt_user =
            SessionMessageEntry::new(SessionMessageKind::User, "", "interrupt")
                .with_runtime_metadata("user-interrupt", "1780000000001");
        interrupt_user.metadata = Some(json!({
            "source": "user_interrupt",
            "starts_user_turn": false,
            "turn_process": true,
            "collapsed_by_default": true,
            "editable": false,
        }));

        let ids = editable_user_prompt_message_ids_from_entries(&[regular_user, interrupt_user]);

        assert!(ids.contains("user-regular"));
        assert!(!ids.contains("user-interrupt"));
    }

    /// Verifies malformed todolist host results are rejected as a whole.
    /// 验证格式错误的 todolist 宿主结果会整体拒绝。
    #[test]
    fn todolist_state_from_tool_finished_event_rejects_invalid_items() {
        let event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "todolist finished",
            "Todolist updated",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "todolist",
                "payload": {
                    "items": [
                        { "step": "有效条目", "status": "completed" },
                        { "step": "", "status": "pending" }
                    ]
                }
            }
        })));

        assert!(todolist_state_from_tool_finished_event(&event, "2026-06-17T00:00:00Z").is_none());
    }

    /// Verifies WebUI todolist summaries preserve status keys and completed counts.
    /// 验证 WebUI todolist 摘要会保留状态键与完成数量。
    #[test]
    fn session_todolist_summary_counts_completed_items() {
        let runtime_state = SessionRuntimeStateEntry {
            todolist: SessionTodolistState {
                explanation: Some(String::from("审核修复")),
                items: vec![
                    SessionTodolistItem {
                        step: String::from("完成菜单接入"),
                        status: SessionTodolistItemStatus::Completed,
                    },
                    SessionTodolistItem {
                        step: String::from("补充测试"),
                        status: SessionTodolistItemStatus::InProgress,
                    },
                ],
                updated_at: Some(String::from("2026-06-17T00:00:00Z")),
            },
            ..SessionRuntimeStateEntry::default()
        };

        let summary = session_todolist_summary_from_runtime_state(Some(&runtime_state))
            .expect("summary should exist");

        assert_eq!(summary.explanation.as_deref(), Some("审核修复"));
        assert_eq!(summary.total_count, 2);
        assert_eq!(summary.completed_count, 1);
        assert_eq!(summary.items[0].status, "completed");
        assert_eq!(summary.items[1].status, "in_progress");
        assert_eq!(summary.updated_at.as_deref(), Some("2026-06-17T00:00:00Z"));
    }

    /// Verifies Goal tool host results preserve updated, cleared, and unrelated event states.
    /// 验证 Goal 工具宿主结果会保留更新、清理与无关事件三种状态。
    #[test]
    fn goal_summary_from_tool_finished_event_preserves_three_states() {
        let session_id = SessionId::new_v4();
        let updated_event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "goal finished",
            "Goal updated",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "session_goal",
                "payload": {
                    "goal": {
                        "sessionId": session_id.to_string(),
                        "goalId": "goal-three-state",
                        "objective": "完成三态测试",
                        "status": "active",
                        "tokenBudget": 1200,
                        "tokensUsed": 34,
                        "usageTokens": 35,
                        "apiCallCount": 2,
                        "toolCallCount": 3,
                        "inputTokens": 100,
                        "cachedInputTokens": 10,
                        "outputTokens": 24,
                        "totalTokens": 124,
                        "timeUsedSeconds": 7,
                        "createdAtMs": 1000,
                        "updatedAtMs": 2000
                    }
                }
            }
        })));
        let cleared_event = RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "goal cleared",
            "Goal cleared",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "session_goal",
                "payload": {
                    "goal": null
                }
            }
        })));
        let unrelated_event = RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "goal started",
            "Goal started",
        )
        .with_metadata(Some(json!({
            "host_result": {
                "kind": "session_goal",
                "payload": {
                    "goal": null
                }
            }
        })));

        let Some(Some(summary)) = goal_summary_from_tool_finished_event(&updated_event) else {
            panic!("updated goal event should parse to a summary");
        };

        assert_eq!(summary.session_id, session_id.to_string());
        assert_eq!(summary.goal_id, "goal-three-state");
        assert_eq!(summary.objective, "完成三态测试");
        assert_eq!(summary.status, "active");
        assert_eq!(summary.token_budget, Some(1200));
        assert_eq!(summary.tokens_used, 34);
        assert_eq!(summary.usage_tokens, 35);
        assert_eq!(
            goal_summary_from_tool_finished_event(&cleared_event),
            Some(None)
        );
        assert_eq!(
            goal_summary_from_tool_finished_event(&unrelated_event),
            None
        );
    }

    /// Verifies the active-goal continuation prompt keeps Codex's full completion and blocked audits.
    /// 验证活跃目标续跑提示词保留 Codex 的完整完成审计与阻塞审计。
    #[test]
    fn goal_continuation_prompt_keeps_codex_audit_details() {
        let prompt = render_goal_continuation_context(&SessionGoalSummary {
            session_id: SessionId::new_v4().to_string(),
            goal_id: String::from("goal-1"),
            objective: String::from("完成目标模式"),
            status: String::from("active"),
            token_budget: Some(10_000),
            tokens_used: 1_234,
            usage_tokens: 1_234,
            api_call_count: 0,
            tool_call_count: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            time_used_seconds: 56,
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
        });

        assert!(prompt.contains("<objective>\n完成目标模式\n</objective>"));
        assert!(prompt.contains("Tokens remaining: 8766"));
        assert!(prompt.contains("Derive concrete requirements"));
        assert!(prompt.contains("The audit must prove completion"));
        assert!(prompt.contains("same blocking condition"));
        assert!(prompt.contains("original/user-triggered turn"));
        assert!(prompt.contains("truly at an impasse"));
        assert!(prompt.contains("call update_goal with status \"blocked\""));
    }

    /// Verifies objective-updated steering mirrors Codex's superseding-objective prompt.
    /// 验证目标更新引导会对齐 Codex 的新目标覆盖旧目标提示词。
    #[test]
    fn goal_objective_updated_prompt_supersedes_previous_goal() {
        let prompt = render_goal_objective_updated_context(&SessionGoalSummary {
            session_id: SessionId::new_v4().to_string(),
            goal_id: String::from("goal-1"),
            objective: String::from("完成修改后的目标"),
            status: String::from("active"),
            token_budget: Some(10_000),
            tokens_used: 1_234,
            usage_tokens: 1_234,
            api_call_count: 0,
            tool_call_count: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            time_used_seconds: 56,
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
        });

        assert!(prompt.contains("edited by the user"));
        assert!(prompt.contains("supersedes any previous thread goal objective"));
        assert!(prompt.contains("<untrusted_objective>\n完成修改后的目标\n</untrusted_objective>"));
        assert!(prompt.contains("Tokens remaining: 8766"));
        assert!(
            prompt
                .contains("Do not call update_goal unless the updated goal is actually complete.")
        );
    }

    /// Verifies budget-limited goals render a stage wrap-up instead of a completion instruction.
    /// 验证预算耗尽目标会渲染阶段收尾提示而不是完成指令。
    #[test]
    fn goal_budget_limited_prompt_waits_for_user_resume() {
        let prompt = render_goal_budget_limited_context(&SessionGoalSummary {
            session_id: SessionId::new_v4().to_string(),
            goal_id: String::from("goal-budget"),
            objective: String::from("完成预算测试"),
            status: String::from("budget_limited"),
            token_budget: Some(2_000),
            tokens_used: 2_345,
            usage_tokens: 2_345,
            api_call_count: 1,
            tool_call_count: 2,
            input_tokens: 1_000,
            cached_input_tokens: 100,
            output_tokens: 1_445,
            total_tokens: 2_445,
            time_used_seconds: 60,
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
        });

        assert!(prompt.contains("stage wrap-up, not goal completion"));
        assert!(prompt.contains("Wait for the user to decide whether to continue"));
        assert!(prompt.contains("Do not continue the next stage automatically"));
        assert!(prompt.contains("Do not call update_goal unless the goal is actually complete."));
    }

    /// Verifies stopped budget-limited goals do not inject ordinary prompt goal context.
    /// 验证已预算耗尽的停止目标不会注入普通提示词目标上下文。
    #[test]
    fn budget_limited_goal_is_not_regular_prompt_context() {
        let goal = SessionGoalSummary {
            session_id: SessionId::new_v4().to_string(),
            goal_id: String::from("goal-budget"),
            objective: String::from("完成预算测试"),
            status: String::from("budget_limited"),
            token_budget: Some(2_000),
            tokens_used: 2_345,
            usage_tokens: 2_345,
            api_call_count: 0,
            tool_call_count: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            time_used_seconds: 60,
            created_at_ms: 1_000,
            updated_at_ms: 2_000,
        };

        assert!(render_session_goal_context_for_prompt(&goal).is_none());
        assert_eq!(
            saved_goal_status_after_edit(ThreadGoalStatus::BudgetLimited),
            ThreadGoalStatus::BudgetLimited
        );
    }

    /// Verifies legacy hidden Goal prompts are stripped from restored user-visible messages.
    /// 验证历史隐藏 Goal 提示会从恢复后的用户可见消息中剥离。
    #[test]
    fn filters_legacy_goal_internal_user_prompts_from_session_messages() {
        let messages = vec![
            SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户指令",
                "Continue working toward the active thread goal.",
            ),
            SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户指令",
                "The active thread goal has reached its token budget.\n\nBudget:",
            ),
            SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户指令",
                "The active thread goal objective was edited by the user.",
            ),
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "真实用户消息"),
        ];

        let filtered = filter_legacy_goal_internal_user_messages(messages);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].body, "真实用户消息");
    }

    /// Verifies image understanding remains available through the native model path when no host target is configured.
    /// 验证未配置宿主目标时，图片理解仍会通过模型原生路径显示为可用。
    #[test]
    fn ai_capabilities_marks_understand_image_native_for_vision_model() {
        let configuration = PersistedAiToolConfiguration::default();
        let model_capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text"), String::from("image")],
            ..ResolvedModelCapabilities::default()
        };
        let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
        let image_capability = summaries
            .iter()
            .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
            .expect("image-understanding capability summary should exist");

        assert!(image_capability.available);
        assert!(image_capability.enabled);
        assert!(image_capability.native);
        assert!(!image_capability.bridged);
        assert!(image_capability.targets.is_empty());
    }

    /// Verifies configured bridge targets do not mark image understanding as bridged when native image input is active.
    /// 验证原生图片输入启用时，已配置桥接目标不会让图片理解被标记为桥接态。
    #[test]
    fn ai_capabilities_marks_understand_image_native_not_bridged_for_vision_model() {
        let configuration = test_understand_image_ai_tool_configuration();
        let model_capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text"), String::from("image")],
            ..ResolvedModelCapabilities::default()
        };
        let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
        let image_capability = summaries
            .iter()
            .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
            .expect("image-understanding capability summary should exist");

        assert!(image_capability.available);
        assert!(image_capability.enabled);
        assert!(image_capability.native);
        assert!(!image_capability.bridged);
        assert_eq!(image_capability.targets.len(), 1);
    }

    /// Verifies disabling the bridge does not disable native image-understanding capability summaries.
    /// 验证关闭桥接不会禁用原生图片理解能力摘要。
    #[test]
    fn ai_capabilities_keeps_understand_image_enabled_when_native_bridge_disabled() {
        let mut configuration = test_understand_image_ai_tool_configuration();
        configuration
            .disabled_capability_kinds
            .insert(AiToolKind::UnderstandImage);
        let model_capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text"), String::from("image")],
            ..ResolvedModelCapabilities::default()
        };
        let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
        let image_capability = summaries
            .iter()
            .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
            .expect("image-understanding capability summary should exist");

        assert!(image_capability.available);
        assert!(image_capability.enabled);
        assert!(image_capability.native);
        assert!(!image_capability.bridged);
    }

    /// Verifies image understanding reports the host bridge path when a configured target exists for a text-only model.
    /// 验证纯文本模型存在已配置目标时，图片理解会报告宿主桥接路径。
    #[test]
    fn ai_capabilities_marks_understand_image_bridged_for_text_model() {
        let configuration = test_understand_image_ai_tool_configuration();
        let model_capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text")],
            ..ResolvedModelCapabilities::default()
        };
        let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
        let image_capability = summaries
            .iter()
            .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
            .expect("image-understanding capability summary should exist");

        assert!(image_capability.available);
        assert!(image_capability.enabled);
        assert!(!image_capability.native);
        assert!(image_capability.bridged);
        assert_eq!(image_capability.targets.len(), 1);
    }

    /// Verifies reordering non-default targets does not create a model-facing runtime notice.
    /// 验证仅重排非默认目标时不会创建面向模型的运行时通知。
    #[test]
    fn ai_capability_update_injection_ignores_non_default_route_reorder() {
        let previous_configuration = test_web_search_ai_tool_configuration(&[
            "search/minimax",
            "search/ollama",
            "search/tavily",
        ]);
        let next_configuration = test_web_search_ai_tool_configuration(&[
            "search/minimax",
            "search/tavily",
            "search/ollama",
        ]);

        let injection = session_ai_capability_configuration_update_injection(
            &previous_configuration,
            &next_configuration,
            None,
        );

        assert!(
            injection.is_none(),
            "non-default route swaps should not affect model-facing tool guidance"
        );
    }

    /// Verifies bridge setting changes do not create notices when the active model has native image input.
    /// 验证当前模型具备原生图片输入时，桥接设置变化不会创建通知。
    #[test]
    fn ai_capability_update_injection_ignores_image_bridge_changes_for_native_model() {
        let previous_configuration = test_understand_image_ai_tool_configuration();
        let mut next_configuration = previous_configuration.clone();
        next_configuration
            .disabled_capability_kinds
            .insert(AiToolKind::UnderstandImage);
        let model_capabilities = ResolvedModelCapabilities {
            input_modalities: vec![String::from("text"), String::from("image")],
            ..ResolvedModelCapabilities::default()
        };

        let injection = session_ai_capability_configuration_update_injection(
            &previous_configuration,
            &next_configuration,
            Some(&model_capabilities),
        );

        assert!(
            injection.is_none(),
            "native image input should make bridge toggles model-invisible"
        );
    }

    /// Verifies default-route changes do not create notices when the active model has native web search.
    /// 验证当前模型具备原生联网搜索时，默认路由变化不会创建通知。
    #[test]
    fn ai_capability_update_injection_ignores_web_search_route_changes_for_native_model() {
        let previous_configuration =
            test_web_search_ai_tool_configuration(&["search/minimax", "search/ollama"]);
        let next_configuration =
            test_web_search_ai_tool_configuration(&["search/ollama", "search/minimax"]);
        let model_capabilities = ResolvedModelCapabilities {
            ai_tools: vec![ResolvedAiToolCapability {
                kind: AiToolKind::WebSearch,
                policy: AiToolPolicy::ForceNative,
                native_provider_tool: true,
                native_tool_type: Some(String::from("web_search_preview")),
                native_request_patch: None,
                fallback_tool_id: None,
                requires_approval: false,
                result_modalities: Vec::new(),
                citation_support: true,
                source_label: String::from("test-native"),
            }],
            ..ResolvedModelCapabilities::default()
        };

        let injection = session_ai_capability_configuration_update_injection(
            &previous_configuration,
            &next_configuration,
            Some(&model_capabilities),
        );

        assert!(
            injection.is_none(),
            "native web search should make bridge route changes model-invisible"
        );
    }

    /// Verifies changing the first enabled target creates one default-route notice.
    /// 验证首个已启用目标变化时会创建一条默认路由通知。
    #[test]
    fn ai_capability_update_injection_reports_default_route_change() {
        let previous_configuration =
            test_web_search_ai_tool_configuration(&["search/minimax", "search/ollama"]);
        let next_configuration =
            test_web_search_ai_tool_configuration(&["search/ollama", "search/minimax"]);

        let injection = session_ai_capability_configuration_update_injection(
            &previous_configuration,
            &next_configuration,
            None,
        )
        .expect("default route change should create injection");
        let content = &injection.items[0].content;

        assert!(content.contains("default route changed"));
        assert!(content.contains("`search/minimax` using model `minimax-model`"));
        assert!(content.contains("`search/ollama` using model `ollama-model`"));
        assert_eq!(
            content
                .matches("- `web_search` (网页搜索): default route changed")
                .count(),
            1
        );
    }

    /// Verifies one capability combines switch and default-route changes into one compaction-scoped persistent injection.
    /// 验证同一能力会将开关与默认路由变化合并进一条压缩作用域持久注入。
    #[test]
    fn ai_capability_update_injection_combines_state_and_default_route_changes_until_compaction() {
        let previous_configuration =
            test_web_search_ai_tool_configuration(&["search/minimax", "search/ollama"]);
        let mut next_configuration =
            test_web_search_ai_tool_configuration(&["search/ollama", "search/minimax"]);
        next_configuration
            .disabled_capability_kinds
            .insert(AiToolKind::WebSearch);

        let injection = session_ai_capability_configuration_update_injection(
            &previous_configuration,
            &next_configuration,
            None,
        )
        .expect("combined change should create injection");
        let content = &injection.items[0].content;

        assert_eq!(injection.kind, InjectionKind::Persistent);
        assert_eq!(injection.ttl_turns, None);
        assert_eq!(
            injection.compaction_policy,
            InjectionCompactionPolicy::Clear
        );
        assert_eq!(content.matches("- `web_search`").count(), 1);
        assert!(content.contains("state changed: DISABLED"));
        assert!(content.contains("default route changed"));

        let mut state = ContextInjectionState::default();
        state.apply_insert(injection, "1");
        let group = state
            .groups
            .get(SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG)
            .expect("injection group should exist before compaction");
        let item = group.items.values().next().expect("injection item");
        assert_eq!(group.kind, InjectionKind::Persistent);
        assert_eq!(item.ttl_turns, None);
        assert_eq!(item.compaction_policy, InjectionCompactionPolicy::Clear);

        state.apply_compaction();

        assert!(
            state
                .groups
                .get(SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG)
                .is_none(),
            "context compaction should clear runtime capability update notices"
        );
    }

    /// Builds a minimal usable image-understanding host route for WebUI capability summary tests.
    /// 为 WebUI 能力摘要测试构建一条最小可用的图片理解宿主路由。
    fn test_understand_image_ai_tool_configuration() -> PersistedAiToolConfiguration {
        PersistedAiToolConfiguration {
            vendors: vec![PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::UnderstandImage,
                    "minimax",
                    "minimax",
                )],
            }],
            understand_image:
                vulcan_provider_config::config::PersistedUnderstandImageConfiguration {
                    target_order: vec![String::from("minimax/minimax")],
                    ..vulcan_provider_config::config::PersistedUnderstandImageConfiguration::default(
                    )
                },
            ..PersistedAiToolConfiguration::default()
        }
    }

    /// Builds a web-search configuration with ordered enabled host targets.
    /// 构建一份带有有序启用宿主目标的网页搜索配置。
    fn test_web_search_ai_tool_configuration(
        target_order: &[&str],
    ) -> PersistedAiToolConfiguration {
        PersistedAiToolConfiguration {
            vendors: vec![PersistedAiCapabilityVendorEntry {
                id: String::from("search"),
                vendor_id: String::from("search"),
                name: String::from("Search"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::WebSearch,
                        "minimax",
                        "minimax-model",
                    ),
                    PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::WebSearch,
                        "ollama",
                        "ollama-model",
                    ),
                    PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::WebSearch,
                        "tavily",
                        "tavily-model",
                    ),
                ],
            }],
            web_search: vulcan_provider_config::config::PersistedWebSearchConfiguration {
                target_order: target_order
                    .iter()
                    .map(|target| target.to_string())
                    .collect(),
                ..vulcan_provider_config::config::PersistedWebSearchConfiguration::default()
            },
            ..PersistedAiToolConfiguration::default()
        }
    }

    /// Ensures every frontend business RPC method has a kernel dispatcher arm.
    /// 确保每个前端业务 RPC 方法都有服务内核分发分支。
    #[test]
    fn frontend_service_rpc_methods_are_dispatched_by_kernel() {
        let frontend_sources = [
            include_str!(
                "../../../apps/vulcan-code-web/src/service/conversation-actions-service.ts"
            ),
            include_str!(
                "../../../apps/vulcan-code-web/src/service/conversation-tool-detail-service.ts"
            ),
            include_str!("../../../apps/vulcan-code-web/src/service/memory-actions-service.ts"),
            include_str!("../../../apps/vulcan-code-web/src/service/project-actions-service.ts"),
            include_str!("../../../apps/vulcan-code-web/src/service/settings-actions-service.ts"),
            include_str!("../../../apps/vulcan-code-web/src/service/skills-actions-service.ts"),
            include_str!("../../../apps/vulcan-code-web/src/service/workbench-snapshot-service.ts"),
        ];
        let kernel_source = include_str!("lib.rs");
        let methods = frontend_service_rpc_methods(&frontend_sources);
        assert!(
            !methods.is_empty(),
            "frontend service RPC method extraction should not be empty"
        );
        for method in methods {
            let needle = format!("\"{method}\"");
            assert!(
                kernel_source.contains(&needle),
                "frontend RPC method `{method}` is not present in kernel dispatcher"
            );
        }
    }

    /// Extracts literal serviceClient.request method names from frontend source.
    /// 从前端源码中提取字面量 serviceClient.request 方法名。
    fn frontend_service_rpc_methods(sources: &[&str]) -> BTreeSet<String> {
        let mut methods = BTreeSet::new();
        for source in sources {
            let mut remaining = *source;
            while let Some(index) = remaining.find("serviceClient.request") {
                remaining = &remaining[index + "serviceClient.request".len()..];
                if let Some(method) = first_double_quoted_literal(remaining) {
                    methods.insert(method);
                }
            }
        }
        methods
    }

    /// Returns the first double-quoted literal in one source fragment.
    /// 返回单个源码片段中的第一个双引号字面量。
    fn first_double_quoted_literal(source: &str) -> Option<String> {
        let start = source.find('"')?;
        let rest = &source[start + 1..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    /// Verifies backend diff summary scanning handles CRLF and mode tokens without line arrays.
    /// 验证后端 diff 摘要扫描无需行数组即可处理 CRLF 与 mode token。
    #[test]
    fn webui_diff_summary_scanner_handles_crlf_and_mode_tokens() {
        let unified_diff = concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\r\n",
            "old mode 100644\r\n",
            "new mode 100755\r\n",
            "index 1111111..2222222 100755\r\n",
            "--- a/src/lib.rs\r\n",
            "+++ b/src/lib.rs\r\n",
            "@@ -1,2 +1,2 @@\r\n",
            "-old value\r\n",
            "+new value\r\n",
            " context value\r\n",
        );
        let summaries = webui_unified_diff_file_summaries(unified_diff);
        let stats = webui_unified_diff_stats(unified_diff);

        assert_eq!(webui_unified_diff_line_count(unified_diff), 10);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].file_mode.as_deref(), Some("100755"));
        assert_eq!(summaries[0].row_count, 4);
        assert_eq!(summaries[0].hunk_count, 1);
        assert_eq!(stats.files, 1);
        assert_eq!(stats.additions, 1);
        assert_eq!(stats.deletions, 1);
    }

    /// Verifies Git quoted UTF-8 path escapes are decoded for WebUI file summaries.
    /// 验证 WebUI 文件摘要会解码 Git 引号内的 UTF-8 路径转义。
    #[test]
    fn webui_diff_summary_scanner_decodes_git_quoted_utf8_paths() {
        let unified_diff = concat!(
            "diff --git \"a/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/old.txt\" \"b/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/new.txt\"\n",
            "similarity index 80%\n",
            "rename from \"\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/old.txt\"\n",
            "rename to \"\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/new.txt\"\n",
            "--- \"a/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/old.txt\"\n",
            "+++ \"b/\\344\\270\\255\\346\\226\\207 \\347\\233\\256\\345\\275\\225/new.txt\"\n",
            "@@ -1 +1 @@\n",
            "-old value\n",
            "+new value\n",
        );
        let summaries = webui_unified_diff_file_summaries(unified_diff);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].old_path, "中文 目录/old.txt");
        assert_eq!(summaries[0].new_path, "中文 目录/new.txt");
        assert_eq!(summaries[0].status, "renamed");
    }

    /// Verifies the backend WebUI diff service paths work against a real temporary Git repository.
    /// 验证后端 WebUI diff 服务路径可在真实临时 Git 仓库中工作。
    #[test]
    fn webui_diff_service_paths_cover_real_git_modes() {
        let repository = WebuiDiffTestRepository::new();
        write_webui_diff_test_file(
            repository.path(),
            "src/lib.rs",
            concat!(
                "pub fn marker() -> &'static str {\n",
                "    \"main\"\n",
                "}\n",
                "\n",
                "pub const STABLE_CONTEXT_01: &str = \"one\";\n",
                "pub const STABLE_CONTEXT_02: &str = \"two\";\n",
                "pub const STABLE_CONTEXT_03: &str = \"three\";\n",
                "pub const STABLE_CONTEXT_04: &str = \"four\";\n",
                "pub const STABLE_CONTEXT_05: &str = \"five\";\n",
                "pub const STABLE_CONTEXT_06: &str = \"six\";\n",
                "pub const STABLE_CONTEXT_07: &str = \"seven\";\n",
                "pub const STABLE_CONTEXT_08: &str = \"eight\";\n",
                "pub const STABLE_CONTEXT_09: &str = \"nine\";\n",
                "pub const STABLE_CONTEXT_10: &str = \"ten\";\n",
                "pub const STABLE_CONTEXT_11: &str = \"eleven\";\n",
                "pub const STABLE_CONTEXT_12: &str = \"twelve\";\n",
                "pub const STABLE_CONTEXT_13: &str = \"thirteen\";\n",
                "pub const STABLE_CONTEXT_14: &str = \"fourteen\";\n",
                "pub const STABLE_CONTEXT_15: &str = \"fifteen\";\n",
                "pub const STABLE_CONTEXT_16: &str = \"sixteen\";\n",
                "pub const STABLE_CONTEXT_17: &str = \"seventeen\";\n",
                "pub const STABLE_CONTEXT_18: &str = \"eighteen\";\n",
                "pub const STABLE_CONTEXT_19: &str = \"nineteen\";\n",
                "pub const STABLE_CONTEXT_20: &str = \"twenty\";\n",
                "\n",
                "pub fn distant() -> &'static str {\n",
                "    \"stable\"\n",
                "}\n"
            ),
        );
        run_webui_diff_test_git(repository.path(), &["init"]);
        run_webui_diff_test_git(
            repository.path(),
            &["config", "user.email", "webui@example.test"],
        );
        run_webui_diff_test_git(
            repository.path(),
            &["config", "user.name", "WebUI Diff Test"],
        );
        run_webui_diff_test_git(repository.path(), &["add", "src/lib.rs"]);
        run_webui_diff_test_git(repository.path(), &["commit", "-m", "initial"]);
        run_webui_diff_test_git(repository.path(), &["branch", "-M", "main"]);
        run_webui_diff_test_git(repository.path(), &["checkout", "-b", "feature/webui-diff"]);
        write_webui_diff_test_file(
            repository.path(),
            "src/lib.rs",
            concat!(
                "pub fn marker() -> &'static str {\n",
                "    \"branch target\"\n",
                "}\n",
                "\n",
                "pub const STABLE_CONTEXT_01: &str = \"one\";\n",
                "pub const STABLE_CONTEXT_02: &str = \"two\";\n",
                "pub const STABLE_CONTEXT_03: &str = \"three\";\n",
                "pub const STABLE_CONTEXT_04: &str = \"four\";\n",
                "pub const STABLE_CONTEXT_05: &str = \"five\";\n",
                "pub const STABLE_CONTEXT_06: &str = \"six\";\n",
                "pub const STABLE_CONTEXT_07: &str = \"seven\";\n",
                "pub const STABLE_CONTEXT_08: &str = \"eight\";\n",
                "pub const STABLE_CONTEXT_09: &str = \"nine\";\n",
                "pub const STABLE_CONTEXT_10: &str = \"ten\";\n",
                "pub const STABLE_CONTEXT_11: &str = \"eleven\";\n",
                "pub const STABLE_CONTEXT_12: &str = \"twelve\";\n",
                "pub const STABLE_CONTEXT_13: &str = \"thirteen\";\n",
                "pub const STABLE_CONTEXT_14: &str = \"fourteen\";\n",
                "pub const STABLE_CONTEXT_15: &str = \"fifteen\";\n",
                "pub const STABLE_CONTEXT_16: &str = \"sixteen\";\n",
                "pub const STABLE_CONTEXT_17: &str = \"seventeen\";\n",
                "pub const STABLE_CONTEXT_18: &str = \"eighteen\";\n",
                "pub const STABLE_CONTEXT_19: &str = \"nineteen\";\n",
                "pub const STABLE_CONTEXT_20: &str = \"twenty\";\n",
                "\n",
                "pub fn distant() -> &'static str {\n",
                "    \"stable\"\n",
                "}\n"
            ),
        );
        run_webui_diff_test_git(repository.path(), &["add", "src/lib.rs"]);
        run_webui_diff_test_git(
            repository.path(),
            &["commit", "-m", "feature branch marker"],
        );
        let feature_commit = webui_diff_test_git_stdout(repository.path(), &["rev-parse", "HEAD"]);
        write_webui_diff_test_file(
            repository.path(),
            "src/lib.rs",
            concat!(
                "pub fn marker() -> &'static str {\n",
                "    \"unstaged top marker\"\n",
                "}\n",
                "\n",
                "pub const STABLE_CONTEXT_01: &str = \"one\";\n",
                "pub const STABLE_CONTEXT_02: &str = \"two\";\n",
                "pub const STABLE_CONTEXT_03: &str = \"three\";\n",
                "pub const STABLE_CONTEXT_04: &str = \"four\";\n",
                "pub const STABLE_CONTEXT_05: &str = \"five\";\n",
                "pub const STABLE_CONTEXT_06: &str = \"six\";\n",
                "pub const STABLE_CONTEXT_07: &str = \"seven\";\n",
                "pub const STABLE_CONTEXT_08: &str = \"eight\";\n",
                "pub const STABLE_CONTEXT_09: &str = \"nine\";\n",
                "pub const STABLE_CONTEXT_10: &str = \"ten\";\n",
                "pub const STABLE_CONTEXT_11: &str = \"eleven\";\n",
                "pub const STABLE_CONTEXT_12: &str = \"twelve\";\n",
                "pub const STABLE_CONTEXT_13: &str = \"thirteen\";\n",
                "pub const STABLE_CONTEXT_14: &str = \"fourteen\";\n",
                "pub const STABLE_CONTEXT_15: &str = \"fifteen\";\n",
                "pub const STABLE_CONTEXT_16: &str = \"sixteen\";\n",
                "pub const STABLE_CONTEXT_17: &str = \"seventeen\";\n",
                "pub const STABLE_CONTEXT_18: &str = \"eighteen\";\n",
                "pub const STABLE_CONTEXT_19: &str = \"nineteen\";\n",
                "pub const STABLE_CONTEXT_20: &str = \"twenty\";\n",
                "\n",
                "pub fn distant() -> &'static str {\n",
                "    \"unstaged marker\"\n",
                "}\n"
            ),
        );
        write_webui_diff_test_file(
            repository.path(),
            "src/staged.rs",
            "pub const STAGED_MARKER: &str = \"staged marker\";\n",
        );
        run_webui_diff_test_git(repository.path(), &["add", "src/staged.rs"]);

        let repository_text = repository.path().to_string_lossy().to_string();
        let unstaged = load_webui_workspace_diff_snapshot(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Unstaged,
                base_branch: None,
                compare_branch: None,
                commit_hash: None,
            }),
        )
        .expect("unstaged snapshot should load");
        let staged = load_webui_workspace_diff_snapshot(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Staged,
                base_branch: None,
                compare_branch: None,
                commit_hash: None,
            }),
        )
        .expect("staged snapshot should load");
        let commit = load_webui_workspace_diff_snapshot(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Commit,
                base_branch: None,
                compare_branch: None,
                commit_hash: Some(feature_commit.clone()),
            }),
        )
        .expect("commit snapshot should load");
        let branch = load_webui_workspace_diff_snapshot(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Branch,
                base_branch: Some(String::from("feature/webui-diff")),
                compare_branch: Some(String::from("main")),
                commit_hash: None,
            }),
        )
        .expect("branch snapshot should load");

        assert!(unstaged.unified_diff.contains("unstaged marker"));
        assert!(unstaged.unified_diff.contains("unstaged top marker"));
        assert!(staged.unified_diff.contains("staged marker"));
        assert!(commit.unified_diff.contains("branch target"));
        assert!(branch.unified_diff.contains("branch target"));
        assert_eq!(staged.files.len(), 1);
        assert_eq!(staged.files[0].new_path, "src/staged.rs");

        let detail = load_webui_workspace_diff_file_detail(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Staged,
                base_branch: None,
                compare_branch: None,
                commit_hash: None,
            }),
            WebuiWorkspaceDiffFileDetailRequest {
                key: staged.files[0].key.clone(),
                old_path: staged.files[0].old_path.clone(),
                new_path: staged.files[0].new_path.clone(),
                context_lines: Some(WEBUI_DIFF_DEFAULT_CONTEXT_LINES),
                hunk_index: None,
                hunk_expansion_direction: None,
            },
        )
        .expect("staged file detail should load");
        assert!(detail.unified_diff.contains("staged marker"));
        assert_eq!(
            detail.file.as_ref().map(|file| file.new_path.as_str()),
            Some("src/staged.rs")
        );

        let expanded_hunk_detail = load_webui_workspace_diff_file_detail(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Unstaged,
                base_branch: None,
                compare_branch: None,
                commit_hash: None,
            }),
            WebuiWorkspaceDiffFileDetailRequest {
                key: unstaged.files[0].key.clone(),
                old_path: unstaged.files[0].old_path.clone(),
                new_path: unstaged.files[0].new_path.clone(),
                context_lines: Some(8),
                hunk_index: Some(0),
                hunk_expansion_direction: Some(String::from("down")),
            },
        )
        .expect("unstaged hunk-scoped file detail should load");
        assert_eq!(expanded_hunk_detail.hunk_index, Some(0));
        assert_eq!(
            expanded_hunk_detail.hunk_expansion_direction.as_deref(),
            Some("down")
        );
        assert_eq!(expanded_hunk_detail.context_lines, 8);
        assert!(
            expanded_hunk_detail
                .unified_diff
                .contains("unstaged top marker")
        );
        assert!(
            !expanded_hunk_detail
                .unified_diff
                .contains("unstaged marker")
        );
        assert_eq!(
            expanded_hunk_detail
                .file
                .as_ref()
                .map(|file| file.hunk_count),
            Some(1)
        );

        let search = load_webui_workspace_diff_search(
            &repository_text,
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Branch,
                base_branch: Some(String::from("feature/webui-diff")),
                compare_branch: Some(String::from("main")),
                commit_hash: None,
            }),
            WebuiWorkspaceDiffSearchRequest {
                query: String::from("branch target"),
                limit: 10,
            },
        )
        .expect("branch search should load");
        assert_eq!(search.matches.len(), 1);
        assert_eq!(search.matches[0].new_path, "src/lib.rs");
        assert!(search.matches[0].first_preview.contains("branch target"));
    }

    /// Verifies the WebUI Git command layer preserves UTF-8 paths from real Git output.
    /// 验证 WebUI Git 命令层会保留真实 Git 输出中的 UTF-8 路径。
    #[test]
    fn webui_diff_service_preserves_utf8_paths_from_git_output() {
        let repository = WebuiDiffTestRepository::new();
        run_webui_diff_test_git(repository.path(), &["init"]);
        run_webui_diff_test_git(
            repository.path(),
            &["config", "user.email", "webui@example.test"],
        );
        run_webui_diff_test_git(
            repository.path(),
            &["config", "user.name", "WebUI Diff Test"],
        );
        write_webui_diff_test_file(
            repository.path(),
            "中文目录/中文文件.txt",
            "initial value\n",
        );
        run_webui_diff_test_git(repository.path(), &["add", "中文目录/中文文件.txt"]);
        run_webui_diff_test_git(repository.path(), &["commit", "-m", "initial utf8 path"]);
        write_webui_diff_test_file(
            repository.path(),
            "中文目录/中文文件.txt",
            "updated value\n",
        );

        let snapshot = load_webui_workspace_diff_snapshot(
            &repository.path().to_string_lossy(),
            Some(WebuiWorkspaceDiffRequest {
                mode: WebuiWorkspaceDiffMode::Unstaged,
                base_branch: None,
                compare_branch: None,
                commit_hash: None,
            }),
        )
        .expect("utf8 path snapshot should load");

        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].new_path, "中文目录/中文文件.txt");
        assert!(
            snapshot
                .unified_diff
                .contains("diff --git a/中文目录/中文文件.txt b/中文目录/中文文件.txt")
        );
    }

    /// Verifies hunk-scoped detail keeps only the expanded target hunk.
    /// 验证 hunk 级详情裁剪只保留目标扩展 hunk。
    #[test]
    fn webui_diff_hunk_scoped_unified_diff_keeps_only_target_hunk() {
        let base_unified_diff = concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "index 1111111..2222222 100644\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -10,2 +10,2 @@\n",
            " context\n",
            "-old target\n",
            "+new target\n",
            "@@ -30,1 +30,1 @@\n",
            "-old other\n",
            "+new other\n",
        );
        let expanded_unified_diff = concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "index 1111111..2222222 100644\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -5,12 +5,12 @@\n",
            " pre context\n",
            " context\n",
            "-old target\n",
            "+new target\n",
            " post context\n",
            "@@ -28,5 +28,5 @@\n",
            " other pre\n",
            "-old other\n",
            "+new other\n",
            " other post\n",
        );

        let scoped =
            webui_diff_hunk_scoped_unified_diff(base_unified_diff, expanded_unified_diff, 0)
                .expect("target hunk should be found");

        assert!(scoped.contains("@@ -5,12 +5,12 @@"));
        assert!(scoped.contains("new target"));
        assert!(!scoped.contains("@@ -28,5 +28,5 @@"));
        assert!(!scoped.contains("new other"));
    }

    /// Test shell host that rejects interactive shell operations.
    /// 拒绝交互式 shell 操作的测试宿主壳。
    struct TestServiceShellHost;

    impl ServiceShellHost for TestServiceShellHost {
        /// Rejects host file-manager use in unit tests.
        /// 在单元测试中拒绝使用宿主文件管理器。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Err(String::from(
                "test shell host does not support opening paths",
            ))
        }

        /// Rejects host browser use in unit tests.
        /// 在单元测试中拒绝使用宿主浏览器。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Err(String::from(
                "test shell host does not support opening URLs",
            ))
        }
    }

    /// Builds a shared test shell host.
    /// 构建共享测试宿主壳。
    fn test_service_shell_host() -> Arc<dyn ServiceShellHost> {
        Arc::new(TestServiceShellHost)
    }

    /// Temporary Git repository removed when a WebUI diff test finishes.
    /// WebUI diff 测试结束后自动移除的临时 Git 仓库。
    struct WebuiDiffTestRepository {
        /// Absolute repository path under the system temporary directory.
        /// 系统临时目录下的绝对仓库路径。
        path: PathBuf,
    }

    impl WebuiDiffTestRepository {
        /// Creates one empty temporary repository directory.
        /// 创建一个空的临时仓库目录。
        fn new() -> Self {
            static NEXT_WEBUI_DIFF_TEST_REPOSITORY_ID: AtomicU64 = AtomicU64::new(1);
            let repository_id = NEXT_WEBUI_DIFF_TEST_REPOSITORY_ID.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "vulcan-webui-diff-test-{}-{}-{}",
                std::process::id(),
                current_unix_millis_value(),
                repository_id
            ));
            fs::create_dir_all(&path).expect("temporary WebUI diff repository should be created");
            Self { path }
        }

        /// Returns the temporary repository path.
        /// 返回临时仓库路径。
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for WebuiDiffTestRepository {
        /// Removes the temporary repository directory.
        /// 移除临时仓库目录。
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// Writes one file inside the temporary WebUI diff repository.
    /// 在临时 WebUI diff 仓库中写入单个文件。
    fn write_webui_diff_test_file(repository_root: &Path, relative_path: &str, body: &str) {
        let path = repository_root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("test file parent should be created");
        }
        fs::write(path, body).expect("test file should be written");
    }

    /// Runs one Git command for a temporary WebUI diff repository and requires success.
    /// 为临时 WebUI diff 仓库运行单个 Git 命令并要求成功。
    fn run_webui_diff_test_git(repository_root: &Path, args: &[&str]) {
        let output =
            run_webui_git_command(repository_root, args).expect("test git command should run");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Runs one Git command for a temporary WebUI diff repository and returns stdout.
    /// 为临时 WebUI diff 仓库运行单个 Git 命令并返回 stdout。
    fn webui_diff_test_git_stdout(repository_root: &Path, args: &[&str]) -> String {
        let output =
            run_webui_git_command(repository_root, args).expect("test git command should run");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Verifies the initial WebUI prompt snapshot keeps memory recall visible instead of downgrading to a generic request cue.
    /// 验证 WebUI 提示词初始快照会保留记忆召回显示，而不是降级成通用请求提示。
    #[test]
    fn webui_prompt_snapshot_preparing_preserves_memory_recall_phase() {
        let session_id = SessionId::new_v4();
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "记忆召回测试",
                "",
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
            session_runtime_state: BTreeMap::from([(
                session_id,
                SessionRuntimeStateEntry::default(),
            )]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::MemoryRecall)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);

        assert_eq!(
            workspace_snapshot.session_run_phases.get(&session_id),
            Some(&SessionRunPhase::MemoryRecall)
        );
    }

    /// Verifies WebUI restores precheck metadata without fabricating dialogue context from display rows.
    /// 验证 WebUI 会恢复 precheck 元数据，但不会从显示行伪造对话层上下文。
    #[test]
    fn ensure_webui_session_runtime_state_keeps_display_rows_out_of_dialogue_context() {
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
            .expect("runtime state shell should be rebuilt");
        assert_eq!(
            runtime_state.active_precheck_turn_id.as_deref(),
            Some("turn-1")
        );
        assert!(!runtime_state_has_concrete_conversation_context(
            runtime_state
        ));
        assert!(
            !workspace_snapshot
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

    /// Verifies display transcript rows alone never satisfy prompt-submission dialogue context.
    /// 验证仅有显示层转录行时不能满足提示词提交所需的对话层上下文。
    #[test]
    fn retained_submission_context_rejects_display_only_messages() {
        // session_id identifies the existing conversation being submitted.
        // session_id 标识正在提交的既有会话。
        let session_id = SessionId::new_v4();
        // workspace_snapshot simulates a selected conversation whose UI window was loaded but runtime context is absent.
        // workspace_snapshot 模拟 UI 窗口已加载但运行态上下文缺失的选中会话。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "显示层会话",
                "",
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
                    "只存在于显示层的正文",
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

        assert!(!retained_submission_snapshot_has_session_context(
            &workspace_snapshot,
            session_id
        ));
    }

    /// Verifies display records alone never satisfy provider-visible dialogue readiness.
    /// 验证仅有显示记录时不能满足 Provider 可见对话就绪条件。
    #[test]
    fn retained_submission_context_rejects_record_only_runtime_state() {
        // session_id identifies the existing conversation being submitted.
        // session_id 标识正在提交的既有会话。
        let session_id = SessionId::new_v4();
        // runtime_state simulates a projection-only state whose canonical API messages were not restored.
        // runtime_state 模拟只恢复了投影记录但未恢复规范 API 消息的状态。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state
            .conversation_state
            .records
            .push(SessionConversationRecord {
                id: String::from("display-only-record"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("只存在于 records 的显示正文"),
                metadata: None,
                created_at: String::from("1000"),
            });
        // workspace_snapshot carries runtime records but no canonical messages.
        // workspace_snapshot 携带运行态 records，但不携带规范 messages。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "仅显示记录会话",
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
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::from([session_id]),
        };

        assert!(!retained_submission_snapshot_has_session_context(
            &workspace_snapshot,
            session_id
        ));
    }

    /// Verifies prompt submission accepts retained snapshots only when runtime dialogue context exists.
    /// 验证提示词提交仅在运行态对话层上下文存在时接受保留快照。
    #[test]
    fn retained_submission_context_accepts_runtime_dialogue_context() {
        // session_id identifies the existing conversation being submitted.
        // session_id 标识正在提交的既有会话。
        let session_id = SessionId::new_v4();
        // runtime_state stores the concrete dialogue context used for future API requests.
        // runtime_state 保存后续 API 请求使用的具体对话层上下文。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state =
            conversation_state_from_session_messages(&[SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户",
                "继续之前的问题",
            )]);
        // workspace_snapshot simulates a retained snapshot whose dialogue layer is ready.
        // workspace_snapshot 模拟对话层已就绪的保留快照。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "对话层会话",
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
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        assert!(retained_submission_snapshot_has_session_context(
            &workspace_snapshot,
            session_id
        ));
    }

    /// Verifies WebUI display projection prefers retained display rows over dialogue context rows.
    /// 验证 WebUI 显示投影优先使用保留的显示层行而不是对话层上下文行。
    #[test]
    fn display_session_messages_prefer_display_window_over_dialogue_context() {
        // session_id identifies the selected conversation.
        // session_id 标识当前选中的会话。
        let session_id = SessionId::new_v4();
        // display_entry represents the UI transcript window loaded from segmented turn rows.
        // display_entry 表示从分段 turn 行加载的 UI 转录窗口。
        let display_entry =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "显示层正文");
        // runtime_state represents the compact dialogue context used for API requests.
        // runtime_state 表示 API 请求使用的压缩对话层上下文。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state =
            conversation_state_from_session_messages(&[SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "对话层正文",
            )]);
        // workspace_snapshot carries both layers so the projection priority can be checked directly.
        // workspace_snapshot 同时携带两层状态，以便直接检查投影优先级。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(session_id, vec![display_entry])]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::from([session_id]),
        };

        // display_messages should preserve the UI-visible transcript body.
        // display_messages 应保留 UI 可见转录正文。
        let display_messages =
            display_session_messages_from_workspace(&workspace_snapshot, session_id)
                .expect("display messages should exist");
        // compaction_messages should preserve the runtime context body.
        // compaction_messages 应保留运行态上下文正文。
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should exist");
        let compaction_messages =
            conversation_state_to_compaction_source_messages(&runtime_state.conversation_state);

        assert_eq!(
            display_messages
                .first()
                .map(|message| message.body.as_str()),
            Some("显示层正文")
        );
        assert_eq!(
            compaction_messages
                .first()
                .map(|message| message.body.as_str()),
            Some("对话层正文")
        );
    }

    /// Verifies runtime-log hydrated messages can back compaction without display records.
    /// 验证运行时日志水合消息在没有显示记录时也能支撑压缩。
    #[test]
    fn compaction_source_accepts_runtime_log_messages_without_records() {
        let session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![
            SessionConversationMessage {
                id: String::from("runtime-user-1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("运行时日志里的用户问题"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("runtime-assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("运行时日志里的助手回答"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
        ];
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should exist");
        let messages =
            conversation_state_to_compaction_source_messages(&runtime_state.conversation_state);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "runtime-user-1");
        assert!(messages[0].body.contains("运行时日志里的用户问题"));
        assert!(messages[1].body.contains("运行时日志里的助手回答"));
    }

    /// Verifies live goal-accounting turn stats include embedded auto-compaction usage.
    /// 验证目标结算使用的实时轮次统计会包含嵌入式自动压缩用量。
    #[test]
    fn latest_session_turn_stats_includes_embedded_compaction_usage_segments() {
        let conversation_state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("session-user-2000"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("继续当前任务"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("session-assistant-2001"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("已继续处理"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: Some(ProviderUsageSnapshot {
                            input_tokens: Some(40),
                            output_tokens: Some(12),
                            total_tokens: Some(52),
                            cached_input_tokens: Some(16),
                            ..ProviderUsageSnapshot::default()
                        }),
                        tool_calls: Vec::new(),
                    },
                },
            ],
            records: vec![
                SessionConversationRecord {
                    id: String::from("record-user-2000"),
                    kind: SessionMessageKind::User,
                    record_kind: SessionMessageRecordKind::Transcript,
                    visible: true,
                    title: String::new(),
                    body: String::from("继续当前任务"),
                    metadata: None,
                    created_at: String::from("2000"),
                },
                SessionConversationRecord {
                    id: String::from("record-summary-2002"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::SummaryOutput,
                    visible: true,
                    title: String::new(),
                    body: String::from("已继续处理"),
                    metadata: None,
                    created_at: String::from("2002"),
                },
            ],
            continuation: Default::default(),
        };
        let mut compaction_segment = SessionTurnUsageSegment::from_provider_usage(
            SessionTurnUsageSegmentKind::ContextCompaction,
            1,
            Some(&ProviderUsageSnapshot {
                input_tokens: Some(20),
                output_tokens: Some(4),
                total_tokens: Some(24),
                cached_input_tokens: Some(5),
                ..ProviderUsageSnapshot::default()
            }),
        );
        compaction_segment.trigger = Some(String::from("auto_budget"));

        let stats = latest_session_turn_stats_from_state(
            &conversation_state,
            Some(1),
            vec![compaction_segment],
        )
        .expect("latest turn stats should be built");

        assert_eq!(stats.provider_call_count, 2);
        assert_eq!(stats.input_tokens, 60);
        assert_eq!(stats.output_tokens, 16);
        assert_eq!(stats.total_tokens, 76);
        assert_eq!(stats.cached_input_tokens, Some(21));
        assert_eq!(goal_token_delta_for_turn_stats(&stats), Some(55));
        assert_eq!(stats.usage_segments.len(), 2);
        assert_eq!(
            stats.usage_segments[0].kind,
            SessionTurnUsageSegmentKind::AssistantResponse
        );
        assert_eq!(
            stats.usage_segments[1].kind,
            SessionTurnUsageSegmentKind::ContextCompaction
        );
    }

    /// Verifies runtime-only compaction sources do not become display fallback rows.
    /// 验证仅运行态压缩来源不会变成显示层回退行。
    #[test]
    fn display_session_messages_do_not_project_runtime_only_context() {
        // session_id identifies a runtime-only hydrated conversation.
        // session_id 标识只有运行态已水合的会话。
        let session_id = SessionId::new_v4();
        // runtime_state carries provider-visible messages but no display records.
        // runtime_state 携带 Provider 可见消息但不携带显示记录。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![SessionConversationMessage {
            id: String::from("runtime-user-1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("只能用于运行态的正文"),
                attachments: Vec::new(),
            },
        }];
        // workspace_snapshot intentionally has no session_messages row for the display layer.
        // workspace_snapshot 有意不携带显示层 session_messages 行。
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        assert!(display_session_messages_from_workspace(&workspace_snapshot, session_id).is_none());
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should exist");
        assert!(
            !conversation_state_to_compaction_source_messages(&runtime_state.conversation_state)
                .is_empty()
        );
    }

    /// Verifies draft submissions can reuse the retained workspace instead of reloading every workspace session.
    /// 验证草稿提交可以复用保留工作区，而不是重载所有工作区会话。
    #[test]
    fn retained_submission_snapshot_accepts_draft_submission() {
        let project_id = String::from("project-a");
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                project_id.clone(),
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

        assert!(retained_submission_snapshot_is_usable(
            &workspace_snapshot,
            Some(project_id.as_str()),
            None
        ));
        assert!(!retained_submission_snapshot_is_usable(
            &workspace_snapshot,
            Some("missing-project"),
            None
        ));
    }

    /// Verifies retained old-session submissions require only the target session to exist in memory.
    /// 验证保留的老会话提交只要求目标会话存在于内存快照中。
    #[test]
    fn retained_submission_snapshot_accepts_existing_session_only_when_present() {
        let session_id = SessionId::new_v4();
        let missing_session_id = SessionId::new_v4();
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "已有会话",
                "",
                SessionState::Draft,
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
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };

        assert!(retained_submission_snapshot_is_usable(
            &workspace_snapshot,
            None,
            Some(session_id)
        ));
        assert!(!retained_submission_snapshot_is_usable(
            &workspace_snapshot,
            None,
            Some(missing_session_id)
        ));
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

    /// Verifies hydrated idle sessions keep their retained transcript window across index refreshes.
    /// 验证已水合的空闲会话在索引刷新后保留其转录窗口。
    #[test]
    fn preserve_active_session_run_state_keeps_hydrated_idle_session_context() {
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
                "已加载会话",
                "已有正文",
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
                    "已保留的正文",
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
                vec![session_id],
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "已加载会话",
                "刷新索引",
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

        assert_eq!(
            refreshed_snapshot
                .session_messages
                .get(&session_id)
                .map(Vec::len),
            Some(1)
        );
        assert!(
            refreshed_snapshot
                .hydrated_session_ids
                .contains(&session_id)
        );
    }

    /// Verifies concrete runtime dialogue context survives an index refresh even when the marker is absent.
    /// 验证即使水合标记缺失，具体运行态对话上下文也会在索引刷新后保留。
    #[test]
    fn preserve_active_session_run_state_keeps_runtime_context_without_hydrated_marker() {
        let session_id = SessionId::new_v4();
        let project_id = String::from("project-a");
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.conversation_state.messages = vec![SessionConversationMessage {
            id: String::from("runtime-user-1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("新对话的运行态正文"),
                attachments: Vec::new(),
            },
        }];
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
                "新对话",
                "已有运行态",
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
            session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
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
                vec![session_id],
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "新对话",
                "刷新索引",
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

        let runtime_state = refreshed_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime context should survive refresh");
        assert_eq!(runtime_state.conversation_state.messages.len(), 1);
        assert!(
            refreshed_snapshot
                .hydrated_session_ids
                .contains(&session_id)
        );
    }

    /// Verifies WebUI snapshots include every Rust-retained hydrated conversation body.
    /// 验证 WebUI 快照会包含 Rust 保留的全部已水合会话正文。
    #[test]
    fn append_hydrated_workspace_messages_keeps_idle_loaded_conversations_visible() {
        let active_session_id = SessionId::new_v4();
        let idle_session_id = SessionId::new_v4();
        let mut messages = conversation_messages_from_session(
            &active_session_id.to_string(),
            &[SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "活动正文",
            )],
            false,
        );
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([
                (
                    active_session_id,
                    vec![SessionMessageEntry::new(
                        SessionMessageKind::Assistant,
                        "助手",
                        "活动正文",
                    )],
                ),
                (
                    idle_session_id,
                    vec![SessionMessageEntry::new(
                        SessionMessageKind::Assistant,
                        "助手",
                        "空闲正文",
                    )],
                ),
            ]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: active_session_id,
            hydrated_session_ids: BTreeSet::from([active_session_id, idle_session_id]),
        };

        append_hydrated_workspace_messages(&mut messages, &workspace_snapshot);
        let conversation_ids = messages
            .iter()
            .map(|message| message.conversation_id.as_str())
            .collect::<BTreeSet<_>>();

        assert!(conversation_ids.contains(active_session_id.to_string().as_str()));
        assert!(conversation_ids.contains(idle_session_id.to_string().as_str()));
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
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: Some(workspace_snapshot),
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
        }));
        let report = ExecutionRunReport {
            session_id: task_session_id,
            preparation: None,
            assistant_output: Some(String::from("子代理完成")),
            thinking_output: None,
            conversation_state: None,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            embedded_turn_usage_segments: Vec::new(),
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
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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

    /// Verifies live transcript persistence versioning does not invalidate session persistence.
    /// 验证运行中转录持久化版本不会使会话持久化版本失效。
    #[test]
    fn live_transcript_persist_version_is_independent_from_session_persist_version() {
        // session_id identifies the session whose persistence lanes are compared.
        // session_id 标识需要比较持久化通道的会话。
        let session_id = SessionId::new_v4();
        // state carries independent version tables for session and live transcript writes.
        // state 携带会话写入与运行中转录写入的独立版本表。
        let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
            session_id,
        )));

        // session_version belongs to the full session/runtime persistence lane.
        // session_version 属于完整会话与运行态持久化通道。
        let session_version = next_webui_session_persist_version(&state, session_id)
            .expect("session persist version should allocate");
        // transcript_version belongs to the live transcript-only persistence lane.
        // transcript_version 属于仅运行中转录持久化通道。
        let transcript_version = next_webui_session_transcript_persist_version(&state, session_id)
            .expect("transcript persist version should allocate");

        assert!(webui_session_persist_version_is_current(
            &state,
            session_id,
            session_version
        ));
        assert!(webui_session_transcript_persist_version_is_current(
            &state,
            session_id,
            transcript_version
        ));
        assert!(!webui_session_persist_version_is_current(
            &state,
            session_id,
            session_version.saturating_add(1)
        ));
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
        assert!(
            runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry))
        );
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
        assert!(
            runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry))
        );
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
        assert!(
            runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry))
        );
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
            shell_host: test_service_shell_host(),
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
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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
    #[tokio::test]
    async fn webui_session_stopped_kernel_suppresses_stale_stream_handle() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
            .expect("kernel turn should start");
        state
            .active_stream_tasks
            .insert(session_id, spawn_service_task(async {}));
        stop_webui_session_kernel_generation(&mut state, session_id);

        assert!(state.active_stream_tasks.contains_key(&session_id));
        assert!(!webui_session_has_active_run(&state, session_id));
    }

    /// Verifies legacy stream handles alone cannot become the active-run fact source.
    /// 验证旧流任务句柄本身不能成为活动运行事实源。
    #[tokio::test]
    async fn webui_session_active_run_ignores_stream_handle_without_kernel_turn() {
        let session_id = SessionId::new_v4();
        let mut state = test_service_state_with_parent_session(session_id);

        state
            .active_stream_tasks
            .insert(session_id, spawn_service_task(async {}));

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
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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

    /// Verifies a same-parent inactive subagent panel falls back to the parent surface.
    /// 验证同父级的非活动子代理面板会回退到父会话界面。
    #[test]
    fn inactive_same_parent_subagent_panel_falls_back_to_parent_surface() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let mut guard = ServiceBridgeState {
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_tasks: BTreeMap::from([(
                parent_session_id,
                BTreeMap::from([(
                    task_session_id,
                    SubagentTaskPanelState {
                        task_id: task_session_id,
                        public_task_id: String::from("inactive-task"),
                        parent_session_id,
                        subagent_label: String::from("已结束子代理"),
                        description: None,
                        run_state: SubagentTaskRunState::Completed,
                        result_text: String::new(),
                        conversation_state: None,
                    },
                )]),
            )]),
            session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
            active_subagent_panel: Some(SelectedSubagentTaskPanel {
                parent_session_id,
                task_id: task_session_id,
            }),
            detached_subagent_task_listeners: BTreeSet::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            parent_session_completed_wait_agent_selectors: BTreeMap::new(),
            active_stream_tasks: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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
        assert!(guard.active_subagent_panel.is_none());
        assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
    }

    /// Verifies a stale WebUI subagent panel cannot hijack another selected conversation surface.
    /// 验证 WebUI 残留的子代理面板不能劫持另一个已选会话的显示界面。
    #[test]
    fn webui_active_subagent_panel_cannot_cross_selected_session() {
        let selected_session_id = SessionId::new_v4();
        let previous_session_id = SessionId::new_v4();
        let previous_task_id = SessionId::new_v4();
        let mut guard = ServiceBridgeState {
            shell_host: test_service_shell_host(),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench: fallback_workbench_snapshot(&[]),
            workbench_generation: 0,
            workspace_snapshot: None,
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            token_budget_runtime: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
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
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
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

    /// Verifies WebUI stop projects the same stopped turn records as the WebUI transcript chain.
    /// 验证 WebUI 停止流程投影与 WebUI 转录链条一致的已停止轮次记录。
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

    /// Verifies ordinary tool rows do not trigger persisted task mirror loading.
    /// 验证普通工具行不会触发持久化任务镜像加载。
    #[test]
    fn task_mirror_gate_ignores_non_task_tool_rows() {
        let mut message = SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "调用 shell_command 工具",
            "ok",
        )
        .with_runtime_metadata("tool-shell", "1780000000000");
        message.record_kind = SessionMessageRecordKind::ToolCallOutput;
        message.metadata = Some(json!({
            "tool_name": "shell_command",
            "display_state": "completed"
        }));
        let messages = vec![message];

        assert!(!surface_contains_task_session_tool_card(
            messages.iter(),
            None
        ));
    }

    /// Verifies task-session tool rows pass the persisted mirror repair gate.
    /// 验证 task-session 工具行会通过持久化镜像修复门禁。
    #[test]
    fn task_mirror_gate_accepts_task_session_tool_rows() {
        let messages = vec![test_spawn_agent_tool_message(
            SessionId::new_v4(),
            "running",
            SessionMessageRecordKind::ToolCallOutput,
        )];

        assert!(surface_contains_task_session_tool_card(
            messages.iter(),
            None
        ));
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
