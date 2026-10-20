//! Core service kernel used by Vulcan Code WebUI hosts.
//! Vulcan Code WebUI 宿主使用的核心服务内核。

// The real media integration fixtures exceed rustc's default type-query depth in test builds.
// 真实媒体集成夹具在测试构建中超过 rustc 默认类型查询深度。
// Keep this compiler-only bound test-scoped; production recursion and lint policy stay unchanged.
// 将此编译器边界限定于测试；生产递归与 lint 策略保持不变。
#![cfg_attr(test, recursion_limit = "256")]
// Operational diagnostics must use the host subscriber; tests may still print fixture results.
// 运行诊断必须使用宿主接收器，测试仍可输出夹具结果。
#![cfg_attr(not(test), deny(clippy::print_stderr, clippy::print_stdout))]

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use uuid::Uuid;
#[cfg(test)]
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
#[cfg(test)]
use vulcan_agent_runtime::conversation_protocol::conversation_state_from_session_messages;
use vulcan_agent_runtime::{
    ExecutionKnowledgeAccessContext, ExecutionPreparation, ExecutionPreparationRequest,
    ExecutionRunReport, ExecutionStreamEvent,
    TransientReasoningStatus as RuntimeTransientReasoningStatus,
    TransientReasoningStatusKind as RuntimeTransientReasoningStatusKind,
    agent_profile::AgentProfileCatalog,
    agent_runtime::{
        context_injection_runtime::context_injection_panel_cards_for_kind,
        conversation_hooks::{ConversationHookRuntime, ConversationTurnCompletionStatus},
        events::{ProviderRequestDurabilityCompletion, RuntimeEvent, RuntimeEventKind},
        extension_generation::ExtensionGeneration,
        provider_request_telemetry::{
            ProviderRequestCompletedUsage, ProviderRequestTelemetryEvent,
            ProviderRequestTelemetryEventKind,
        },
        question_runtime::{
            PendingQuestionChangeCallback, PendingQuestionDurabilityAction,
            PendingQuestionDurabilityCallback, QuestionRuntime,
        },
        task_runtime::{DetachedTaskRuntimeEvent, TaskRuntime},
        tool_runtime_bridge::persistent_tool_output_from_result,
        vmm_hooks::VmmHookRuntimeHandle,
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{
        McpRuntimeAccess, plugin_mcp_server_configs_for_descriptors,
        remove_plugin_mcp_runtime_partition, replace_plugin_mcp_runtime_partition,
        replace_user_mcp_runtime_partition, runtime_mcp_server_configs,
    },
    provider_config::{
        self, ProviderConfigurationFile, ReasoningEffortLevel, WorkbenchThinkingDisplayMode,
    },
    session_event_runtime::build_terminal_execution_report_state,
    stream_runtime_run_with_intervention_plugins_and_public_ai,
    tooling::{CommandSessionRuntime, command_session_runtime::CommandSessionInputEvent},
    turn_stats::build_session_turn_stats_from_state_with_segments,
};
use vulcan_context::{
    ContextBudgetSeverity, ContextBudgetSnapshot, SessionConversationRecord, append_record,
};
use vulcan_context_injection::{InjectionKind, InjectionPanelCard};
use vulcan_core::types::ProviderUsageSnapshot;
pub(crate) use vulcan_provider_config::config::{
    PersistedLuaSkillToolRegistrationState, PersistedMcpOAuthConfig, PersistedMcpServerEntry,
    PersistedMcpTransportKind, PersistedPromptSkillState,
};
#[cfg(test)]
use vulcan_service_api::SessionGoalStatus;
use vulcan_service_api::{
    AgentSkillCategorySaveRequest, AgentSkillLoadIssueSummary, AgentSkillSaveRequest,
    AgentSkillsCatalogChangedDelta, AgentSkillsCatalogSummary, AgentSkillsCatalogUpdate,
    CommandSessionDisplayAttachment, CommandSessionDisplayChangedEvent, CommandSessionSummary,
    CommandSessionsChangedDelta, ContextCompactionWindowNodeSummary, ContextInjectionCardSummary,
    ContextInjectionChangedDelta, ContextInjectionItemSummary, ContextInjectionSnapshot,
    ContextUsageBreakdownItem, ContextUsageChangedDelta, ContextUsageSnapshot,
    ConversationArchivedDelta, ConversationRunSettledEvent, ConversationSummary,
    ConversationsArchivedDelta, KeyValueSummary, LogEntrySummary, LuaSkillConfigDeleteRequest,
    LuaSkillConfigDeleteResult, LuaSkillConfigDescriptor, LuaSkillConfigEnumOption,
    LuaSkillConfigEvent, LuaSkillConfigEventBatch, LuaSkillConfigIssue, LuaSkillConfigItem,
    LuaSkillConfigRefreshRequest, LuaSkillConfigSetRequest, LuaSkillConfigSetResult,
    LuaSkillConfigStoreRefresh, LuaSkillConfigSummary, LuaSkillConfigValidation, LuaSkillSummary,
    LuaSkillToolSummary, LuaSkillsCatalogSummary, LuaSkillsCatalogUpdate,
    LuaSkillsManagedRuntimeBootstrapSummary, LuaSkillsManagedRuntimeComponentSummary,
    McpServerSummary, McpToolSummary, MemorySummary, OptionalTextPatch,
    PendingQuestionChangedDelta, PendingQuestionOptionSummary, PendingQuestionPromptSummary,
    PendingQuestionSnapshot, PluginSummary, ProjectRemovedDelta, ProjectSummary,
    ProjectTerminalCatalog, ProjectTerminalDisplayAttachment, ProjectTerminalDisplayChangedEvent,
    ProjectTerminalRunningCountChangedEvent, ProjectTerminalShellOption, ProjectTerminalSummary,
    ProviderModelSummary, ProviderSummary, ReasoningSelectionOptionSummary,
    ReasoningSelectionSummary, RuntimeModuleSnapshot, ServiceActiveWorkSummary, ServiceEvent,
    ServiceEventEmitter, ServiceLifecycle, ServiceStatusFailure, ServiceStatusSnapshot,
    SessionAddonChangedEvent, SessionAddonsChangedDelta, SessionAddonsChangedResponse,
    SessionContextCompactionWindowUpdateRequest, SessionGoalSummary, SessionOutputStyleSummary,
    SessionOutputStyleUpdateRequest, SessionPlanArtifactSummary, SessionPlanModeSummary,
    SessionTodolistItemSummary, SessionTodolistSummary, SessionVmmPolicyByConversation,
    SessionVmmPolicySummary, SettingSectionSummary, StartupSnapshot, StartupStepStatus,
    SubagentCatalogIssueSummary, SubagentCatalogSummary, SubagentCatalogUpdate,
    SubagentCategorySummary, SubagentDefinitionSaveRequest, SubagentDefinitionSummary,
    SubagentNavigationChangedDelta, SubagentNavigationSnapshot, SubagentPanelRouteResponse,
    TerminalDisplayChunk, ToolCallSummary, TransientReasoningStatusChangedDelta,
    TransientReasoningStatusKind, TransientReasoningStatusSnapshot,
    TransientReasoningSummaryItemSnapshot, UpdateConversationRunPhaseDelta, VmmMemorySnapshot,
    WEBUI_SERVICE_DISCOVERY_STATUS_FAILED, WEBUI_SERVICE_DISCOVERY_STATUS_READY,
    WEBUI_SERVICE_PORT_ENV, WEBUI_STATIC_DIR_ENV, WorkbenchDelta, WorkbenchDeltaEnvelope,
    WorkbenchSnapshot, WorkspaceGitChangedEvent, webui_service_discovery_snapshot_with_status,
    webui_service_starting_discovery_snapshot, webui_static_root_from_candidates,
};
use vulcan_service_host::{
    SharedWebuiServiceDiscoverySnapshot, WebuiServiceHttpConfig, WebuiServiceRpcDispatcher,
    bind_webui_service_http_listener, serve_webui_service_http_listener,
    write_webui_service_discovery_snapshot,
};
use vulcan_session_core::{
    ActiveTurnKind, SessionNodeId, SessionRuntimeKernel, TurnTerminalStatus,
};
#[cfg(test)]
use vulcan_subagent_core::{SUBAGENT_TASK_ID_KEY, SUBAGENT_TASK_KIND};
#[cfg(test)]
use vulcan_workbench_model::PendingSessionPromptSource;
#[cfg(test)]
use vulcan_workbench_model::SessionTurnStats;
use vulcan_workbench_model::{
    SUBAGENT_TASK_STOPPED_SUMMARY_TEXT, SelectedSubagentTaskPanel, SessionAddonNamespace,
    SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
    SessionPlanArtifactState, SessionPlanModeState, SessionRunPhase, SessionRuntimeStateEntry,
    SessionState, SessionSummary, SessionTitleSource, SessionTodolistItemStatus,
    SessionTurnStatsKind, SessionTurnStatsStatus, SessionTurnUsageSegment, SessionWorkMode,
    SubagentTaskPanelState, SubagentTaskRunState, TaskProjectSummary,
    build_session_record_message_id, parse_subagent_task_panel_metadata,
    subagent_close_agent_projection_from_host_result,
    subagent_close_agent_projection_from_tool_metadata,
    subagent_feedback_execution_id_from_tool_metadata, subagent_navigation_task_label,
};
use vulcan_workspace_store::{data_plane::HostRuntimeHandle, workspace_store::WorkspaceSnapshot};

mod agent_collaboration;
mod agent_collaboration_execution;
mod agent_skills_watcher;
mod agent_stream;
mod ai_credential_secret_store;
mod answer_mode;
mod attachments;
pub mod browser;
mod command_sessions;
pub mod configured_inference;
mod context_compaction;
mod conversation_actions;
mod conversation_drafts_v1;
mod conversation_events_v1;
mod conversation_generations_v1;
mod conversation_resources_v1;
mod conversation_run_services;
mod conversation_submission;
mod dedicated_session;
mod execution_report_memory;
mod fallback_workbench_projection;
#[cfg(test)]
mod generated_ts_constants;
mod git;
mod git_diff_services;
mod git_watcher;
mod host_tools_settings;
mod internal_ai;
mod knowledge_base_services;
mod licenses;
pub mod luaskills_diagnostics;
mod luaskills_install_progress;
mod luaskills_model;
mod markdown_memory_services;
mod mcp_oauth;
mod mcp_settings;
mod media_upload;
mod native_plugin_shutdown;
mod native_vmm;
mod output_style;
mod pending_prompt_services;
mod pending_prompts;
mod permission_judge;
mod permission_mode;
mod plugin_environment_install_tasks;
mod plugin_mcp_secret_store;
mod plugin_runtime_services;
mod plugin_services;
mod plugin_storage;
mod plugin_ui_events;
mod plugin_vldb;
mod project_conversation_services;
mod project_directories;
mod project_terminals;
mod provider_manifest_sync;
mod provider_settings;
mod provider_usage_runtime;
mod public_ai_settings;
mod rpc_dispatch;
mod runtime_summary_projection;
mod runtime_workspace_helpers;
mod service_bootstrap;
mod service_bridge_io;
mod service_diagnostics_paths;
mod service_state_access;
mod session_business_hook;
mod session_goal_lifecycle;
mod session_goals;
mod session_hydration;
mod session_media_resources_v1;
mod session_model_projection_v1;
mod session_plan_runtime;
mod session_request_config_v1;
mod session_runtime_kernel;
mod session_runtime_registry_v1;
mod session_v1_lifecycle;
mod settings_provider_projection;
mod settings_runtime_projection;
mod skill_catalog_projection;
mod skills_services;
mod state;
mod subagent_event_ingest;
mod subagent_panels;
mod subagent_workspace_mutation;
mod system_prompt_settings;
mod terminal_settlement_service;
mod tool_permission_host;
mod vmm;
mod vmm_hooks;
mod vmm_session_controller;
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
mod workspace_session_queries;
mod workspace_watcher;
use agent_collaboration::service_cancel_collaboration_question;
use agent_stream::*;
use attachments::{
    composer_attachment_capabilities, composer_attachment_draft_scope,
    composer_attachment_summary_from_core, composer_attachment_summary_to_core,
    composer_attachment_summary_to_provider_core, composer_attachment_workspace_path,
    composer_attachment_workspace_path_from_snapshot,
    resolve_composer_attachment_summary_for_workspace, service_execute_project_attachment_cleanup,
    service_preview_project_attachment_cleanup, service_reconcile_composer_attachment_draft,
    service_stage_composer_attachment_bytes, service_stage_composer_attachment_paths,
    service_stage_composer_attachment_preview_bytes,
};
use command_sessions::*;
use context_compaction::{
    MANUAL_CONTEXT_COMPACTION_TRIGGER, append_webui_context_compaction_status_message_to_workspace,
    context_compaction_budget_note_from_state,
    force_auto_compact_webui_session_after_provider_boundary,
    maybe_auto_compact_webui_session_before_provider_request, spawn_webui_context_compaction_task,
    webui_context_compaction_plan_from_v1,
};
use conversation_actions::*;
pub use conversation_events_v1::service_install_conversation_event_v1_emitter;
use conversation_events_v1::*;
use conversation_generations_v1::*;
use conversation_run_services::*;
use conversation_submission::{
    service_continue_conversation, service_submit_conversation_prompt_rpc,
};
#[cfg(test)]
use execution_report_memory::apply_task_session_execution_report_to_memory;
use execution_report_memory::{
    apply_execution_report_to_memory, apply_task_session_execution_report_to_memory_in_place,
    spawn_conversation_completion_hook_task,
};
use fallback_workbench_projection::*;
use git::{
    WebuiGitRepositoryCatalog, WebuiGitRepositoryMonitoringRequest, WebuiWorkspaceDiffFileDetail,
    WebuiWorkspaceDiffFileDetailRequest, WebuiWorkspaceDiffRequest,
    WebuiWorkspaceDiffSearchRequest, WebuiWorkspaceDiffSearchResponse, WebuiWorkspaceDiffSnapshot,
    WebuiWorkspaceGitFileActionRequest, WebuiWorkspaceGitFileActionResult, git_commit_settings,
    load_webui_workspace_diff_file_detail, load_webui_workspace_diff_search,
    load_webui_workspace_diff_snapshot, resolve_webui_repository_root_from_workspace_text_with_key,
    run_webui_workspace_git_file_action, service_generate_git_commit_message,
    service_run_git_commit_action, service_save_git_commit_settings,
};
use git_diff_services::*;
use host_tools_settings::{
    host_tools_settings, host_tools_settings_with_native_tools, service_save_default_command_shell,
};
use knowledge_base_services::*;
use markdown_memory_services::*;
use mcp_settings::*;
use media_upload::*;
pub(crate) use output_style::{
    applied_output_style_snapshot, apply_output_style_snapshot, default_output_style_id,
    output_style_catalog_summary, resolve_output_style_snapshot, service_output_style_catalog,
    service_save_prompt_output_style,
};
use pending_prompt_services::{
    service_edit_pending_prompt, service_remove_pending_prompt, service_reorder_pending_prompts,
};
use pending_prompts::{
    emit_pending_prompts_changed_delta, enqueue_webui_pending_prompt_from_submission,
    session_has_blocking_system_prompt_work,
};
use plugin_environment_install_tasks::*;
use plugin_services::{
    service_invoke_plugin_action, service_load_plugin_ui_module, service_read_plugin_text_asset,
};
pub use project_conversation_services::service_add_project_initial_directory;
use project_conversation_services::*;
pub use project_directories::service_add_project_from_path;
use project_directories::service_browse_project_directories;
use project_directories::visible_project_id_for_path;
use project_terminals::*;
use provider_settings::{
    provider_retry_settings, service_clear_selected_provider_api_trace_directory,
    service_open_selected_provider_api_trace_directory,
    service_save_provider_request_trace_settings, service_save_provider_retry_settings,
};
use provider_usage_runtime::*;
use public_ai_settings::*;
use rpc_dispatch::dispatch_service_rpc;
use runtime_summary_projection::*;
use runtime_workspace_helpers::*;
use service_bootstrap::startup_snapshot_with_phase;
pub use service_bootstrap::{emit_initial_service_events, spawn_service_bootstrap};
use service_bridge_io::*;
pub use service_bridge_io::{
    WebuiServiceHttpBridgeHandle, remove_owned_webui_service_discovery, service_startup_snapshot,
    service_status_snapshot_from_state, service_ui_locale_from_state, service_workbench_snapshot,
    spawn_webui_service_http_bridge,
};
use service_diagnostics_paths::*;
use service_state_access::*;
use session_goal_lifecycle::{
    account_session_goal_from_report, account_session_goal_provider_usage,
    cache_session_goal_summary, ensure_regular_session_exists, normalize_session_goal_objective,
    persist_goal_turn_usage_context, session_goal_id_for_provider_request,
    session_goal_prompt_parts_from_state, session_goal_request_session_id,
    sync_selected_session_v1_projection_from_store, validate_session_goal_budget,
};
#[cfg(test)]
use session_goal_lifecycle::{
    render_goal_budget_limited_context, render_goal_continuation_context,
    render_goal_objective_updated_context, render_session_goal_context_for_prompt,
};
use session_goals::*;
#[cfg(test)]
use session_hydration::runtime_state_has_concrete_conversation_context;
use session_hydration::{
    capture_preserved_workspace_run_state, hydrate_retained_submission_session_if_needed,
    hydrate_selected_session_runtime_if_needed, load_workspace_for_submission,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, sync_hydrated_session_dialogue_ready_marker,
};
use session_plan_runtime::*;
use session_runtime_kernel::*;
use settings_provider_projection::*;
use settings_runtime_projection::*;
use skill_catalog_projection::*;
use skills_services::*;
pub use state::ServiceShellHost;
pub use state::SharedServiceBridgeState;
use state::*;
use subagent_event_ingest::*;
use subagent_panels::*;
use subagent_workspace_mutation::*;
use terminal_settlement_service::*;
use vmm::{
    service_load_project_vmm_config, service_run_vmm_memory_action, service_save_vmm_memory_config,
    static_vmm_profile_snapshot, vmm_memory_snapshot,
};
use vmm_hooks::{
    apply_vmm_writeback_effects_to_runtime_state, build_vmm_hook_config_snapshot,
    cached_project_vmm_layer, refresh_exact_vmm_hook_session_projection,
};
use webui_prompt_context::*;
use webui_prompt_lifecycle::{
    mark_webui_prompt_preparation_failed, queued_dispatch_authority_for_client_action,
    service_insert_pending_prompt, start_webui_precheck_or_continue,
    webui_shared_prompt_precheck_plan, webui_vmm_runtime_readiness,
};
use webui_turn_chaining::{
    maybe_start_session_goal_continuation_if_idle, maybe_start_webui_turn_chain_after_finished,
    start_exact_v1_pending_in_turn, start_exact_v1_queued_turn,
    start_exact_v1_subagent_return_turn,
};
use workbench_context_usage::*;
use workbench_model_routes::{
    provider_secondary_route_summary_from_public_selection,
    service_list_workbench_model_secondary_routes, service_select_webui_locale,
    service_select_workbench_model_secondary_route, service_select_workbench_reasoning_effort,
    service_select_workbench_thinking_display_mode,
};
use workbench_refresh::*;
use workbench_session_panels::*;
use workbench_surface_projection::{
    open_subagent_registry_entry_for_task,
    subagent_navigation_snapshot_for_parent_from_cached_state,
    subagent_navigation_snapshot_from_cached_state, subagent_navigation_snapshot_from_state,
};
use workspace_files::{service_workspace_files_list, service_workspace_files_read};
use workspace_persistence_versions::*;
use workspace_session_queries::*;

use vulcan_service_runtime::luaskills_bridge::{TauriLuaSkillsCatalog, TauriLuaSkillsHandle};
use vulcan_service_runtime::mcp_bridge::TauriMcpRuntimeCatalog;

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
        pending_prompt_process_mirrors: BTreeMap::new(),
        agent_runtime: None,
        native_vmm_maintenance: None,
        workspace_watcher: None,
        vmm_controller_registry: Default::default(),
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        knowledge_runtime: None,
        luaskills_runtime: None,
        host_database_providers: None,
        cached_luaskills_catalog: None,
        provider_manifest_sync: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeMap::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        active_stream_tasks: BTreeMap::new(),
        question_preparation_tasks: BTreeMap::new(),
        collaboration_dispatch_owners: BTreeMap::new(),
        session_terminal_persistence_owners: BTreeMap::new(),
        child_question_execution_owners: BTreeMap::new(),
        child_maintenance_owners: BTreeMap::new(),
        child_maintenance_tasks: BTreeMap::new(),
        child_maintenance_stops: BTreeMap::new(),
        terminal_pre_intent_repairs: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        automatic_plugin_prewarm: BTreeMap::new(),
        session_runtime_registry: Arc::new(Default::default()),
        service_lifecycle_state: crate::state::ServiceLifecycleState::default(),
        pending_local_plugin_updates: Arc::new(Default::default()),
        // Actual original removal resources survive executor failures.
        // 实际原移除资源跨执行器失败存续。
        pending_local_plugin_uninstalls: Arc::new(Default::default()),
        permission_judge: Arc::new(Default::default()),
        project_resource_catalog_registry: Arc::new(Default::default()),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
        browser_manager: crate::browser::BrowserManager::new(),
    }))
}

/// Stops project terminals, live MCP runtime, and active child-agent work before the host exits.
/// 在宿主应用退出前停止项目终端、实时 MCP 运行时与活动子代理工作。
///
/// # Parameters
/// 参数
/// `state` is the shared service bridge state created during desktop startup.
/// `state` 是桌面启动期间创建的共享服务桥状态。
///
/// # Returns
/// 返回值
/// Task session ids stopped by the shutdown coordinator.
/// 关闭协调器停止的任务会话标识。
pub fn service_prepare_for_app_shutdown(
    state: &SharedServiceBridgeState,
) -> Result<Vec<Uuid>, String> {
    // Runtime handles and the provider sync service are cloned under one state observation,
    // then stopped outside the service lock.
    // 运行时句柄与供应商同步服务在同一次状态观察中完成克隆，随后在服务锁外停止。
    let (task_runtime, mcp_runtime, provider_manifest_sync, session_v1_handles, service_runtime) = {
        // guard provides the single authoritative shutdown snapshot of runtime handles.
        // guard 提供运行时句柄的单次权威关闭快照。
        let mut guard = state
            .lock()
            .map_err(|_| String::from("The service bridge state is locked during shutdown."))?;
        // Close bootstrap admission in the same observation that freezes runtime handles.
        // 在冻结运行时句柄的同一次观测中关闭引导准入。
        guard.service_lifecycle_state.begin_shutdown_drain();
        match guard.agent_runtime.as_ref() {
            Some(agent_runtime) => (
                Some(Arc::clone(&agent_runtime.task_runtime)),
                Some(agent_runtime.extension_generation.mcp_runtime()),
                guard.provider_manifest_sync.clone(),
                Arc::clone(&guard.session_runtime_registry),
                guard.service_runtime.clone(),
            ),
            None => (
                None,
                None,
                guard.provider_manifest_sync.clone(),
                Arc::clone(&guard.session_runtime_registry),
                guard.service_runtime.clone(),
            ),
        }
    };
    let session_v1_handles = session_v1_handles
        .begin_shutdown()
        .map_err(|error| error.to_string())?;
    // Closed admission prevents late publication while actual original update executors and resources drain.
    // 关闭准入防止晚发布，同时排空实际原更新执行器及资源。
    let pending_local_plugin_updates = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_updates,
    );
    if let Some(service_runtime) = service_runtime.as_ref() {
        service_runtime.block_on(pending_local_plugin_updates.drain_for_shutdown())?;
    } else {
        pending_local_plugin_updates.require_empty_without_runtime()?;
    }
    // Original removal ownership drains without publishing or acknowledging failed external effects during shutdown.
    // 关闭期间排空原移除所有权，不发布或确认失败外部副作用。
    let pending_local_plugin_uninstalls = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_uninstalls,
    );
    if let Some(service_runtime) = service_runtime.as_ref() {
        service_runtime.block_on(pending_local_plugin_uninstalls.drain_for_shutdown())?;
    } else {
        pending_local_plugin_uninstalls.require_empty_without_runtime()?;
    }
    // A terminal drain failure is still recoverable here; no process-owned provider/MCP runtime
    // has been stopped. Do not continue into irreversible teardown after that failure.
    // 此时终端排空失败仍可恢复；供应商与 MCP 进程运行时尚未停止。失败后不得继续不可逆拆卸。
    cancel_all_project_terminal_displays()
        .and_then(|_| project_terminal_runtime().request_all_terminal_stops_for_shutdown())
        .map_err(|error| format!("Project terminal shutdown failed: {error}"))?;
    if let Some(provider_manifest_sync) = provider_manifest_sync {
        provider_manifest_sync.stop_periodic_task()?;
    }
    // The periodic synchronizer and subsequent task/MCP teardown have no complete in-process
    // rollback. A later persistence error must leave admission closed and report failure.
    // 周期同步器及后续任务/MCP 拆卸没有完整的进程内回滚；之后持久化失败必须保持准入关闭并报告故障。
    state
        .lock()
        .map_err(|error| error.to_string())?
        .service_lifecycle_state
        .block_shutdown_recovery();
    let runtime_shutdown = ordered_runtime_shutdown(
        || {
            // Maintenance owns independent work even after business tasks finish; drain it before flushing writers.
            // 业务任务结束后维护仍持有独立工作；刷写写入器之前先排空维护。
            if let Some(service_runtime) = service_runtime.as_ref() {
                service_runtime
                    .block_on(agent_collaboration::child_maintenance::stop_all(state))?;
            } else {
                let service = state.lock().map_err(|error| error.to_string())?;
                if !service.child_maintenance_tasks.is_empty()
                    || !service.child_maintenance_owners.is_empty()
                {
                    return Err(String::from(
                        "Child maintenance shutdown requires the retained service runtime.",
                    ));
                }
            }
            match task_runtime.as_ref() {
                Some(task_runtime) => {
                    // stopped_task_ids are captured before canonical writer drain prevents new runtime events.
                    // stopped_task_ids 在规范写入器排空阻止新运行事件之前捕获。
                    let stopped_task_ids = task_runtime.stop_active_tasks_for_app_shutdown()?;
                    // Native finalizers must finish before writers and their MCP dependencies are released.
                    // 原生关闭器必须在写入器及其 MCP 依赖释放之前完成。
                    native_plugin_shutdown::close_original_plugin_runtime(state)?;
                    for handle in &session_v1_handles {
                        handle.flush_all().map_err(|error| error.to_string())?;
                    }
                    Ok(stopped_task_ids)
                }
                None => {
                    // Bootstrap owns the same native parent even before agent runtime publication.
                    // 即使智能体运行时尚未发布，引导也持有同一原生父级。
                    native_plugin_shutdown::close_original_plugin_runtime(state)?;
                    for handle in &session_v1_handles {
                        handle.flush_all().map_err(|error| error.to_string())?;
                    }
                    Ok(Vec::new())
                }
            }
        },
        || {
            if let Some(mcp_runtime) = mcp_runtime {
                mcp_runtime.shutdown();
            }
            Ok(())
        },
    );
    let (stopped_task_ids, runtime_error) = match runtime_shutdown {
        Ok(stopped_task_ids) => (stopped_task_ids, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    if runtime_error.is_none() {
        // LuaSkills model and progress callbacks are process-wide and may retain service-owned
        // runtime handles, so the successful shutdown boundary removes them explicitly.
        // LuaSkills 模型与进度回调是进程级的，并可能保留服务持有的运行时句柄，因此正常关闭
        // 边界必须显式移除它们。
        crate::luaskills_model::shutdown_global();
        crate::luaskills_install_progress::shutdown_global();
        return Ok(stopped_task_ids);
    }
    Err(runtime_error.expect("runtime shutdown failed before successful return"))
}

/// Reopens V1 session mutation admission after the host keeps the window open following a failed shutdown.
/// 在关闭失败后宿主保持窗口打开时重新开放 V1 会话修改准入。
///
/// # Returns
/// 返回值
/// Reopens the V1 session owners that were closed by the failed shutdown attempt.
/// 重新开放此前因关闭尝试而停止准入的 V1 会话所有者。
pub fn service_resume_after_failed_app_shutdown(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let (session_runtime_registry, recovery_blocked) = {
        let guard = state.lock().map_err(|_| {
            String::from("The service bridge state is locked during shutdown recovery.")
        })?;
        (
            Arc::clone(&guard.session_runtime_registry),
            guard.service_lifecycle_state.shutdown_recovery_blocked,
        )
    };
    if recovery_blocked {
        // Reopening a registry after MCP or provider teardown would advertise a ready service
        // whose next AI call cannot use the runtime it was promised.
        // MCP 或供应商运行时拆卸后重新开放注册表，会虚假宣称服务就绪，而下一次 AI 调用已无法使用原有运行时。
        let error = String::from(
            "Shutdown stopped process-owned runtimes; admission cannot safely recover in this process. Restart the service.",
        );
        if let Ok(mut guard) = state.lock() {
            guard
                .service_lifecycle_state
                .retain_recovery_failure(ServiceStatusFailure {
                    stage: String::from("shutdown-recovery"),
                    code: String::from("shutdown_recovery_failed"),
                    message: error.clone(),
                });
        }
        return Err(error);
    }
    let recovery_result = (|| -> Result<(), String> {
        session_runtime_registry
            .resume_after_failed_shutdown()
            .map_err(|error| error.to_string())?;
        state
            .lock()
            .map_err(|error| error.to_string())?
            .service_lifecycle_state
            .end_shutdown_drain();
        Ok(())
    })();
    if let Err(error) = recovery_result {
        let failure = ServiceStatusFailure {
            stage: String::from("shutdown-recovery"),
            code: String::from("shutdown_recovery_failed"),
            message: error.clone(),
        };
        if let Ok(mut guard) = state.lock() {
            guard
                .service_lifecycle_state
                .retain_recovery_failure(failure);
        }
        return Err(error);
    }
    Ok(())
}

/// Runs the strict task and MCP shutdown sequence.
/// 按严格顺序执行任务与 MCP 关闭。
///
/// # Parameters
/// 参数
/// `stop_tasks` cancels active agent work and therefore drains in-flight VMM hooks;
/// `stop_mcp` releases MCP after task shutdown completes.
/// `stop_tasks` 取消活动智能体工作并因此排空运行中的 VMM Hook；
/// `stop_mcp` 在任务关闭完成后释放 MCP。
///
/// # Returns
/// 返回值
/// The exact task identifiers stopped by the first phase, or the first shutdown failure.
/// 返回第一阶段停止的精确任务标识，或首个关闭失败。
fn ordered_runtime_shutdown<StopTasks, StopMcp>(
    stop_tasks: StopTasks,
    stop_mcp: StopMcp,
) -> Result<Vec<Uuid>, String>
where
    StopTasks: FnOnce() -> Result<Vec<Uuid>, String>,
    StopMcp: FnOnce() -> Result<(), String>,
{
    // Active work must stop first so no request can enter VMM while MCP teardown begins.
    // 活动工作必须先停止，避免 MCP 开始释放时仍有请求进入 VMM。
    let stopped_task_ids = stop_tasks()?;
    stop_mcp()?;
    Ok(stopped_task_ids)
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
#[cfg(test)]
mod tests;
