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
        WorkbenchThinkingDisplayMode,
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
    AppendMessageBodyDelta, CommandSessionSummary, CommandSessionsChangedDelta,
    ComposerAttachmentSummary, ContextInjectionCardSummary, ContextInjectionChangedDelta,
    ContextInjectionItemSummary, ContextInjectionSnapshot, ContextUsageChangedDelta,
    ContextUsageSnapshot,
    ConversationForkSourceSummary, ConversationMessage, ConversationSummary, FeatureAreaSummary,
    HydratedConversationWindowSummary, KeyValueSummary, LogEntrySummary, LuaSkillSummary,
    LuaSkillToolSummary, LuaSkillsCatalogSummary, McpServerSummary, MemorySummary, MessageRole,
    PendingQuestionChangedDelta, PendingQuestionOptionSummary, PendingQuestionPromptSummary,
    PendingQuestionSnapshot, PluginSummary, ProjectSummary, ProviderFieldSummary,
    ProviderModelSummary, ProviderSummary, ReasoningSelectionOptionSummary,
    ReasoningSelectionSummary, RuntimeModuleSnapshot, ServiceEvent, ServiceEventEmitter,
    SessionAddonChangedEvent, SessionAddonsChangedDelta, SessionAddonsChangedResponse,
    SessionGoalSummary, SessionPlanArtifactSummary, SessionPlanModeSummary,
    SessionTodolistItemSummary, SessionTodolistSummary, SessionVmmPolicyByConversation,
    SessionVmmPolicySummary, SettingSectionSummary, StartupSnapshot, StartupStepStatus,
    SubagentNavigationChangedDelta, SubagentNavigationSnapshot, SubagentPanelRouteResponse,
    ToolCallSummary, UpdateConversationRunPhaseDelta, UserPromptEditDraftSummary,
    VmmMemorySnapshot, WEBUI_SERVICE_DISCOVERY_STATUS_FAILED, WEBUI_SERVICE_DISCOVERY_STATUS_READY,
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
use vulcan_subagent_core::{SUBAGENT_TASK_ID_KEY, SUBAGENT_TASK_KIND};
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

mod agent_stream;
mod ai_capabilities;
mod attachments;
mod command_sessions;
mod context_compaction;
mod conversation_actions;
mod conversation_display;
mod conversation_history;
mod conversation_message_projection;
mod conversation_run_services;
mod conversation_submission;
mod execution_report_memory;
mod fallback_workbench_projection;
mod git;
mod git_diff_services;
mod host_tools_settings;
mod mcp_settings;
mod pending_prompt_services;
mod pending_prompts;
mod project_conversation_services;
mod project_directories;
mod provider_settings;
mod rpc_dispatch;
mod runtime_summary_projection;
mod runtime_workspace_helpers;
mod service_bootstrap;
mod service_bridge_io;
mod service_diagnostics_paths;
mod service_state_access;
mod session_ai_capabilities;
mod session_goal_lifecycle;
mod session_goals;
mod session_hydration;
mod session_plan_runtime;
mod session_runtime_kernel;
mod settings_provider_projection;
mod settings_runtime_projection;
mod skill_catalog_projection;
mod skills_services;
mod state;
mod subagent_event_ingest;
mod subagent_panels;
mod subagent_workspace_mutation;
mod task_mirror_status_projection;
mod vmm;
mod vmm_hooks;
mod webui_prompt_context;
mod webui_prompt_lifecycle;
mod webui_turn_chaining;
mod workbench_context_usage;
mod workbench_model_routes;
mod workbench_refresh;
mod workbench_session_panels;
mod workbench_surface_projection;
mod workspace_files;
mod workspace_persistence_versions;
mod workspace_session_persistence;
use agent_stream::*;
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
use command_sessions::*;
use context_compaction::{
    MANUAL_CONTEXT_COMPACTION_TRIGGER, append_webui_context_compaction_status_message_to_workspace,
    context_compaction_budget_note_from_state,
    maybe_auto_compact_webui_session_before_provider_request, spawn_webui_context_compaction_task,
    webui_context_compaction_plan_from_conversation_state,
};
use conversation_actions::*;
use conversation_display::*;
use conversation_history::{
    HydratedConversationWindowState, hydrated_conversation_window_summaries,
    load_hydrated_session_workbench, retain_hydrated_conversation_windows_for_workspace,
    service_session_history_page, service_session_messages, service_session_tool_detail,
};
use conversation_message_projection::*;
use conversation_run_services::*;
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
use fallback_workbench_projection::*;
use git::{
    WebuiWorkspaceDiffFileDetail, WebuiWorkspaceDiffFileDetailRequest, WebuiWorkspaceDiffRequest,
    WebuiWorkspaceDiffSearchRequest, WebuiWorkspaceDiffSearchResponse, WebuiWorkspaceDiffSnapshot,
    WebuiWorkspaceGitFileActionRequest, WebuiWorkspaceGitFileActionResult, git_commit_settings,
    load_webui_workspace_diff_file_detail, load_webui_workspace_diff_search,
    load_webui_workspace_diff_snapshot, run_webui_workspace_git_file_action,
    service_generate_git_commit_message, service_run_git_commit_action,
    service_save_git_commit_settings,
};
use git_diff_services::*;
use host_tools_settings::{
    host_tools_settings, service_get_rtk_gain_stats, service_save_rtk_routing_settings,
};
use mcp_settings::*;
use pending_prompt_services::{
    service_edit_pending_prompt, service_remove_pending_prompt, service_reorder_pending_prompts,
};
use pending_prompts::{
    clear_internal_pending_prompt_queue_for_stop, emit_pending_prompts_changed_delta,
    enqueue_webui_pending_prompt_from_submission, pending_prompt_summaries_from_runtime_state,
    provider_configuration_for_model_reference, runtime_state_has_queued_pending_prompt_work,
    session_has_blocking_system_prompt_work, take_webui_internal_guidance_prompts,
};
pub use project_conversation_services::service_add_project_initial_directory;
use project_conversation_services::*;
pub use project_directories::service_add_project_from_path;
use project_directories::service_browse_project_directories;
use provider_settings::{
    service_remove_provider_configuration, service_save_provider_configuration,
    service_save_provider_enabled_models, service_save_provider_model_override,
};
use rpc_dispatch::dispatch_service_rpc;
use runtime_summary_projection::*;
use runtime_workspace_helpers::*;
use service_bootstrap::startup_snapshot_with_phase;
pub use service_bootstrap::{emit_initial_service_events, spawn_service_bootstrap};
pub use service_bridge_io::spawn_webui_service_http_bridge;
use service_bridge_io::*;
use service_diagnostics_paths::*;
use service_state_access::*;
use session_ai_capabilities::*;
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
use session_goals::*;
use session_hydration::{
    apply_hydration_to_selected_workspace_snapshot, capture_preserved_workspace_run_state,
    hydrate_retained_submission_session_if_needed, hydrate_selected_session_runtime_if_needed,
    load_selected_session_display_hydration, load_workspace_for_submission,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, runtime_state_has_concrete_conversation_context,
    sync_hydrated_session_dialogue_ready_marker,
};
use session_plan_runtime::*;
use session_runtime_kernel::*;
use settings_provider_projection::*;
use settings_runtime_projection::*;
use skill_catalog_projection::*;
use skills_services::*;
pub use state::ServiceShellHost;
use state::*;
use subagent_event_ingest::*;
use subagent_panels::*;
use subagent_workspace_mutation::*;
use task_mirror_status_projection::*;
use vmm::{
    service_load_project_vmm_config, service_run_vmm_memory_action, service_save_vmm_memory_config,
    static_vmm_profile_snapshot, vmm_memory_snapshot,
};
use vmm_hooks::{
    apply_vmm_writeback_effects_to_runtime_state, build_vmm_hook_config_snapshot,
    cached_project_vmm_layer,
};
use webui_prompt_context::*;
use webui_prompt_lifecycle::{
    maybe_prepare_next_webui_pending_prompt, maybe_start_webui_mailbox_turn_if_ready,
    service_insert_pending_prompt, start_webui_precheck_or_continue,
    webui_shared_prompt_precheck_plan,
};
use webui_turn_chaining::{
    maybe_start_session_goal_continuation_if_idle, maybe_start_webui_turn_chain_after_finished,
};
use workbench_context_usage::*;
use workbench_model_routes::{
    provider_secondary_route_summary_from_openrouter_selection,
    service_list_workbench_model_secondary_routes, service_select_webui_locale,
    service_select_workbench_model, service_select_workbench_model_secondary_route,
    service_select_workbench_reasoning_effort, service_select_workbench_thinking_display_mode,
};
use workbench_refresh::*;
use workbench_session_panels::*;
#[cfg(test)]
use workbench_surface_projection::active_surface_session_id_from_state;
use workbench_surface_projection::{
    active_surface_messages_from_workspace_and_state, preserved_active_surface_session_id,
    subagent_navigation_snapshot_from_cached_state, subagent_navigation_snapshot_from_state,
    subagent_run_state_key, surface_messages_for_session_without_subagent_navigation_refresh,
    webui_subagent_panel_has_recoverable_detail,
};
use workspace_files::{service_workspace_files_list, service_workspace_files_read};
use workspace_persistence_versions::*;
use workspace_session_persistence::*;

/// Context-injection tag used for the latest session AI capability runtime update.
/// 最新会话 AI 能力运行时变更注入使用的上下文注入标签。
const SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG: &str = "session_ai_capability_update";
/// Stable dedupe key that replaces stale AI capability update notices in the current context window.
/// 用于在当前上下文窗口内替换旧 AI 能力变更通知的稳定去重键。
const SESSION_AI_CAPABILITY_UPDATE_DEDUPE_KEY: &str = "session_ai_capability_update:latest";
use vulcan_service_runtime::luaskills_bridge::{TauriLuaSkillsCatalog, TauriLuaSkillsHandle};
use vulcan_service_runtime::mcp_bridge::{TauriMcpRuntimeCatalog, TauriMcpRuntimeHandle};
use vulcan_service_runtime::token_budget_bridge::TauriTokenBudgetRuntime;

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
    // initial_ui_locale mirrors the persisted WebUI locale before bootstrap finishes.
    // initial_ui_locale 在 bootstrap 完成前镜像已持久化的 WebUI 语言。
    let initial_ui_locale = provider_config::load_configuration_file()
        .ok()
        .and_then(|configuration| configuration.ui_locale);
    let mut startup = startup_snapshot_with_phase("pending", Vec::new(), &[]);
    startup.ui_locale = initial_ui_locale.clone();
    let mut workbench = fallback_workbench_snapshot(&[]);
    workbench.ui_locale = initial_ui_locale;
    Arc::new(Mutex::new(ServiceBridgeState {
        shell_host,
        startup,
        workbench,
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
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    }))
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

#[cfg(test)]
mod tests;
