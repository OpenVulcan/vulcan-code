//! Core service kernel used by Vulcan Code WebUI hosts.
//! Vulcan Code WebUI 宿主使用的核心服务内核。

#![expect(
    clippy::too_many_arguments,
    reason = "service-kernel WebUI boundary functions still pass explicit runtime handles while the shared service context object is being designed as a separate architectural refactor"
)]
#![expect(
    clippy::type_complexity,
    reason = "service-kernel runtime handle bundles intentionally remain explicit until the shared service context object lands"
)]

// Re-export the extracted protocol core under the historical source name during namespace migration.
// 在命名空间迁移期间以历史源码名称引用已提取的协议核心。
extern crate vulcan_llm_core as vulcan_ai_protocols;

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
    ExecutionRunReport, ExecutionStreamEvent, KnowledgeSearchHandler,
    TransientReasoningStatus as RuntimeTransientReasoningStatus,
    TransientReasoningStatusKind as RuntimeTransientReasoningStatusKind,
    agent_profile::AgentProfileCatalog,
    agent_runtime::{
        context_injection_runtime::context_injection_panel_cards_for_kind,
        conversation_hooks::{ConversationHookRuntime, ConversationTurnCompletionStatus},
        events::{ProviderRequestDurabilityCompletion, RuntimeEvent, RuntimeEventKind},
        extension_generation::ExtensionGeneration,
        prompt_composer_runtime::invalidate_runtime_prompt_luaskill_catalog_cache,
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
        validate_plugin_mcp_runtime_partition,
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
    ConversationsArchivedDelta, FeatureAreaSummary, KeyValueSummary, LogEntrySummary,
    LuaSkillConfigDeleteRequest, LuaSkillConfigDeleteResult, LuaSkillConfigDescriptor,
    LuaSkillConfigEnumOption, LuaSkillConfigEvent, LuaSkillConfigEventBatch, LuaSkillConfigIssue,
    LuaSkillConfigItem, LuaSkillConfigRefreshRequest, LuaSkillConfigSetRequest,
    LuaSkillConfigSetResult, LuaSkillConfigStoreRefresh, LuaSkillConfigSummary,
    LuaSkillConfigValidation, LuaSkillSummary, LuaSkillToolSummary, LuaSkillsCatalogSummary,
    LuaSkillsCatalogUpdate, LuaSkillsManagedRuntimeBootstrapSummary,
    LuaSkillsManagedRuntimeComponentSummary, McpServerSummary, McpToolSummary, MemorySummary,
    OptionalTextPatch, PendingQuestionChangedDelta, PendingQuestionOptionSummary,
    PendingQuestionPromptSummary, PendingQuestionSnapshot, PluginSummary, ProjectRemovedDelta,
    ProjectSummary, ProjectTerminalCatalog, ProjectTerminalDisplayAttachment,
    ProjectTerminalDisplayChangedEvent, ProjectTerminalRunningCountChangedEvent,
    ProjectTerminalShellOption, ProjectTerminalSummary, ProviderModelSummary, ProviderSummary,
    ReasoningSelectionOptionSummary, ReasoningSelectionSummary, RuntimeModuleSnapshot,
    ServiceEvent, ServiceEventEmitter, SessionAddonChangedEvent, SessionAddonsChangedDelta,
    SessionAddonsChangedResponse, SessionContextCompactionWindowUpdateRequest, SessionGoalSummary,
    SessionOutputStyleSummary, SessionOutputStyleUpdateRequest, SessionPlanArtifactSummary,
    SessionPlanModeSummary, SessionTodolistItemSummary, SessionTodolistSummary,
    SessionVmmPolicyByConversation, SessionVmmPolicySummary, SettingSectionSummary,
    StartupSnapshot, StartupStepStatus, SubagentCatalogIssueSummary, SubagentCatalogSummary,
    SubagentCatalogUpdate, SubagentCategorySummary, SubagentDefinitionLocaleSummary,
    SubagentDefinitionSaveRequest, SubagentDefinitionSummary, SubagentNavigationChangedDelta,
    SubagentNavigationSnapshot, SubagentPanelRouteResponse, TerminalDisplayChunk, ToolCallSummary,
    TransientReasoningStatusChangedDelta, TransientReasoningStatusKind,
    TransientReasoningStatusSnapshot, TransientReasoningSummaryItemSnapshot,
    UpdateConversationRunPhaseDelta, VmmMemorySnapshot, WEBUI_SERVICE_DISCOVERY_STATUS_FAILED,
    WEBUI_SERVICE_DISCOVERY_STATUS_READY, WEBUI_SERVICE_PORT_ENV, WEBUI_STATIC_DIR_ENV,
    WorkbenchDelta, WorkbenchDeltaEnvelope, WorkbenchSnapshot, WorkspaceGitChangedEvent,
    webui_service_discovery_snapshot_with_status, webui_service_starting_discovery_snapshot,
    webui_static_root_from_candidates,
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
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

mod agent_skills_watcher;
mod agent_stream;
mod ai_credential_secret_store;
mod attachments;
mod codebase_memory_controller;
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
mod execution_report_memory;
mod fallback_workbench_projection;
mod git;
mod git_diff_services;
mod git_watcher;
mod host_tools_settings;
pub mod inference_access;
mod inference_service;
mod knowledge_base_services;
mod managed_memory_mesh_maintenance;
mod managed_memory_mesh_runtime;
mod mcp_settings;
mod media_upload;
mod output_style;
mod pending_prompt_services;
mod pending_prompts;
mod plugin_mcp_secret_store;
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
mod session_goal_lifecycle;
mod session_goals;
mod session_hydration;
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
use agent_stream::*;
use attachments::{
    composer_attachment_capabilities, composer_attachment_draft_scope,
    composer_attachment_summary_from_core, composer_attachment_summary_to_core,
    composer_attachment_summary_to_provider_core, composer_attachment_workspace_path,
    composer_attachment_workspace_path_from_snapshot,
    resolve_composer_attachment_summary_for_workspace, service_execute_project_attachment_cleanup,
    service_pick_composer_attachments, service_preview_project_attachment_cleanup,
    service_reconcile_composer_attachment_draft, service_stage_composer_attachment_bytes,
    service_stage_composer_attachment_paths, service_stage_composer_attachment_preview_bytes,
};
use codebase_memory_controller::*;
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
use host_tools_settings::{host_tools_settings, host_tools_settings_with_native_tools};
use knowledge_base_services::*;
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
use plugin_services::{
    service_invoke_plugin_action, service_load_plugin_ui_module, service_read_plugin_text_asset,
};
pub use project_conversation_services::service_add_project_initial_directory;
use project_conversation_services::*;
pub use project_directories::service_add_project_from_path;
use project_directories::service_browse_project_directories;
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
pub use service_bridge_io::spawn_webui_service_http_bridge;
use service_bridge_io::*;
use service_diagnostics_paths::*;
use service_state_access::*;
use session_goal_lifecycle::{
    account_session_goal_from_report, account_session_goal_provider_usage,
    append_goal_turn_usage_context, attach_goal_context_to_provider_visible_agent_messages,
    cache_session_goal_summary, ensure_regular_session_exists, normalize_session_goal_objective,
    prepend_goal_context_to_current_prompt, replace_canonical_current_user_prompt,
    session_goal_id_for_provider_request, session_goal_prompt_parts_from_state,
    session_goal_request_session_id, sync_selected_session_v1_projection_from_store,
    validate_session_goal_budget,
};
#[cfg(test)]
use session_goal_lifecycle::{
    render_goal_budget_limited_context, render_goal_continuation_context,
    render_goal_objective_updated_context, render_goal_turn_usage_context,
    render_session_goal_context_for_prompt,
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
use state::*;
use subagent_event_ingest::*;
use subagent_panels::*;
use subagent_workspace_mutation::*;
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
use vulcan_service_runtime::mcp_bridge::{TauriMcpRuntimeCatalog, TauriMcpRuntimeHandle};

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
        codebase_memory_controller: Default::default(),
        managed_memory_mesh_runtime: None,
        vmm_controller_registry: Default::default(),
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        knowledge_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
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
        session_terminal_persistence_owners: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_runtime_registry: Arc::new(Default::default()),
        project_resource_catalog_registry: Arc::new(Default::default()),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
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
    let (
        task_runtime,
        mcp_runtime,
        managed_memory_mesh_runtime,
        provider_manifest_sync,
        session_v1_handles,
    ) = {
        // guard provides the single authoritative shutdown snapshot of runtime handles.
        // guard 提供运行时句柄的单次权威关闭快照。
        let guard = state
            .lock()
            .map_err(|_| String::from("The service bridge state is locked during shutdown."))?;
        match guard.agent_runtime.as_ref() {
            Some(agent_runtime) => (
                Some(Arc::clone(&agent_runtime.task_runtime)),
                Some(agent_runtime.extension_generation.mcp_runtime()),
                guard.managed_memory_mesh_runtime.as_ref().map(Arc::clone),
                guard.provider_manifest_sync.clone(),
                Arc::clone(&guard.session_runtime_registry),
            ),
            None => (
                None,
                None,
                guard.managed_memory_mesh_runtime.as_ref().map(Arc::clone),
                guard.provider_manifest_sync.clone(),
                Arc::clone(&guard.session_runtime_registry),
            ),
        }
    };
    let session_v1_handles = session_v1_handles
        .begin_shutdown()
        .map_err(|error| error.to_string())?;
    if let Some(provider_manifest_sync) = provider_manifest_sync {
        provider_manifest_sync.stop_periodic_task()?;
    }
    let project_terminal_shutdown = cancel_all_project_terminal_displays()
        .and_then(|_| project_terminal_runtime().request_all_terminal_stops_for_shutdown());
    let runtime_shutdown = ordered_runtime_shutdown(
        || match task_runtime.as_ref() {
            Some(task_runtime) => {
                // stopped_task_ids are captured before canonical writer drain prevents new runtime events.
                // stopped_task_ids 在规范写入器排空阻止新运行事件之前捕获。
                let stopped_task_ids = task_runtime.stop_active_tasks_for_app_shutdown()?;
                for handle in &session_v1_handles {
                    handle.flush_all().map_err(|error| error.to_string())?;
                }
                Ok(stopped_task_ids)
            }
            None => {
                for handle in &session_v1_handles {
                    handle.flush_all().map_err(|error| error.to_string())?;
                }
                Ok(Vec::new())
            }
        },
        || {
            if let Some(managed_memory_mesh_runtime) = managed_memory_mesh_runtime {
                managed_memory_mesh_runtime
                    .lock()
                    .map_err(|_| {
                        String::from("The managed VMM runtime is locked during shutdown.")
                    })?
                    .stop()?;
                crate::vmm_hooks::clear_managed_vmm_transport_overlay();
            }
            Ok(())
        },
        || {
            if let Some(mcp_runtime) = mcp_runtime {
                mcp_runtime.shutdown();
            }
            Ok(())
        },
    );
    match (runtime_shutdown, project_terminal_shutdown) {
        (Ok(stopped_task_ids), Ok(_)) => Ok(stopped_task_ids),
        (Err(runtime_error), Ok(_)) => Err(runtime_error),
        (Ok(_), Err(terminal_error)) => Err(terminal_error),
        (Err(runtime_error), Err(terminal_error)) => Err(format!(
            "{runtime_error} Project terminal shutdown also failed: {terminal_error}"
        )),
    }
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
    let guard = state.lock().map_err(|_| {
        String::from("The service bridge state is locked during shutdown recovery.")
    })?;
    let session_runtime_registry = Arc::clone(&guard.session_runtime_registry);
    drop(guard);
    session_runtime_registry
        .resume_after_failed_shutdown()
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Runs the strict task, managed-memory, and MCP shutdown sequence.
/// 按严格顺序执行任务、托管记忆与 MCP 关闭。
///
/// # Parameters
/// 参数
/// `stop_tasks` cancels active agent work and therefore drains in-flight VMM hooks;
/// `stop_managed_memory` closes the owned VMM child; `stop_mcp` releases MCP last.
/// `stop_tasks` 取消活动智能体工作并因此排空运行中的 VMM Hook；
/// `stop_managed_memory` 关闭持有的 VMM 子进程；`stop_mcp` 最后释放 MCP。
///
/// # Returns
/// 返回值
/// The exact task identifiers stopped by the first phase, or the first shutdown failure.
/// 返回第一阶段停止的精确任务标识，或首个关闭失败。
fn ordered_runtime_shutdown<StopTasks, StopManagedMemory, StopMcp>(
    stop_tasks: StopTasks,
    stop_managed_memory: StopManagedMemory,
    stop_mcp: StopMcp,
) -> Result<Vec<Uuid>, String>
where
    StopTasks: FnOnce() -> Result<Vec<Uuid>, String>,
    StopManagedMemory: FnOnce() -> Result<(), String>,
    StopMcp: FnOnce() -> Result<(), String>,
{
    // Active work must stop first so no request can enter VMM while its child is closing.
    // 活动工作必须先停止，避免 VMM 子进程关闭时仍有请求进入。
    let stopped_task_ids = stop_tasks()?;
    stop_managed_memory()?;
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
