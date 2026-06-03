//! Local service bridge used by the Tauri WebUI.
//! Tauri WebUI 使用的本地服务桥接层。

use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use futures_util::StreamExt;
use serde_json::Value;
use tauri::{Emitter, Manager};
use time::OffsetDateTime;
use vldb_controller_client::ControllerSqliteValue;
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose, ExecutionRunReport,
    ExecutionStreamEvent,
    agent_profile::{AgentProfileCatalog, load_agent_profile_catalog},
    agent_runtime::{
        context_injection_runtime::{
            collect_builtin_manual_injection_ops_and_sanitized_prompt,
            core_provider_context_injection_from_state_for_turn,
        },
        conversation_hooks::{
            ConversationHookRuntime, VmmWritebackHookEffect, vmm_profile_sync_effect_from_report,
        },
        events::{RuntimeEvent, RuntimeEventKind},
        question_runtime::QuestionRuntime,
        task_runtime::TaskRuntime,
        token_budget_runtime::TokenBudgetRuntime,
        vmm_hooks::{
            VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks,
            vmm_profile_placement_phase,
        },
    },
    conversation_protocol::{
        SessionConversationPayload, conversation_state_from_session_messages,
        conversation_state_to_session_messages,
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{McpRuntimeAccess, bootstrap_mcp_runtime_access},
    provider_config::{self, ProviderConfigurationFile, ReasoningEffortLevel},
    session_event_runtime::{
        SessionStreamingRecordIds, append_runtime_event_projection_to_state,
        append_stream_delta_record_to_state, append_user_prompt_record,
        apply_checkpoint_conversation_state, apply_terminal_execution_report_to_state,
        collapse_streaming_thinking_in_state,
    },
    stream_runtime_run,
    tooling::CommandSessionRuntime,
};
use vulcan_service_api::{
    CommandSessionSummary, ConversationMessage, ConversationSummary, FeatureAreaSummary,
    LogEntrySummary, MemorySummary, MessageRole, PluginSummary, ProjectSummary,
    ProviderModelSummary, ProviderSummary, RuntimeModuleSnapshot, ServiceEvent,
    SettingSectionSummary, StartupSnapshot, StartupStepSnapshot, StartupStepStatus,
    ToolCallSummary, WorkbenchSnapshot,
};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary, SessionTitleSource, TaskProjectSummary,
};
use vulcan_workspace_store::{
    data_plane::{APP_SPACE_ID, ControllerRuntimeHandle},
    workspace_store::{SessionHydrationSnapshot, WorkspaceSnapshot},
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
    /// Agent profile catalog reused by runtime preparation.
    /// 运行时准备阶段复用的 Agent 画像目录。
    agent_profile_catalog: AgentProfileCatalog,
    /// LuaSkills runtime handle reused by provider-visible tool execution.
    /// Provider 可见工具执行复用的 LuaSkills 运行时句柄。
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// MCP runtime access reused by provider-visible MCP tools.
    /// Provider 可见 MCP 工具复用的 MCP 运行时访问面。
    mcp_runtime: Option<McpRuntimeAccess>,
    /// Shared task runtime used by nested task tools.
    /// 嵌套任务工具使用的共享任务运行时。
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

/// Number of recent turns loaded for the selected conversation.
/// 为当前选中会话加载的最近轮次数量。
const SELECTED_SESSION_TURN_LOAD_LIMIT: usize = 24;

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
struct ServiceBridgeState {
    /// Latest startup snapshot.
    /// 最近一次启动快照。
    startup: StartupSnapshot,
    /// Latest workbench snapshot.
    /// 最近一次工作台快照。
    workbench: WorkbenchSnapshot,
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
    /// Long-lived token budget runtime handle.
    /// 长生命周期 Token 预算运行时句柄。
    token_budget_runtime: Option<Arc<TauriTokenBudgetRuntime>>,
    /// Long-lived MCP runtime handle.
    /// 长生命周期 MCP 运行时句柄。
    mcp_runtime: Option<TauriMcpRuntimeHandle>,
}

/// Thread-safe service state handle.
/// 线程安全的服务状态句柄。
type SharedServiceBridgeState = Arc<Mutex<ServiceBridgeState>>;

/// Runs the Tauri application and registers local service commands.
/// 运行 Tauri 应用并注册本地服务命令。
pub fn run() {
    let state = Arc::new(Mutex::new(ServiceBridgeState {
        startup: startup_snapshot_with_phase("pending", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workspace_snapshot: None,
        agent_runtime: None,
        service_runtime: None,
        controller_runtime: None,
        luaskills_runtime: None,
        token_budget_runtime: None,
        mcp_runtime: None,
    }));

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            service_health,
            service_startup_snapshot,
            service_workbench_snapshot,
            service_session_messages,
            service_submit_conversation_prompt,
            service_select_workbench_model,
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
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(messages) =
            in_memory_session_messages_for_service(&guard.workspace_snapshot, parsed_session_id)
        {
            return Ok(conversation_messages_from_session(
                &session_id,
                messages.as_slice(),
            ));
        }
        guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?
    };
    let hydration_snapshot =
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime,
            parsed_session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        {
            Ok(snapshot) => snapshot,
            Err(error) if is_missing_session_manifest_error(&error) => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };

    Ok(conversation_messages_from_session(
        &session_id,
        hydration_snapshot.messages.as_slice(),
    ))
}

/// Returns the current in-memory transcript projection for one service session.
/// 返回单个服务会话当前的内存转录投影。
fn in_memory_session_messages_for_service(
    workspace_snapshot: &Option<WorkspaceSnapshot>,
    session_id: SessionId,
) -> Option<Vec<SessionMessageEntry>> {
    let workspace_snapshot = workspace_snapshot.as_ref()?;
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
) -> Result<WorkbenchSnapshot, String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(String::from("请输入任务指令。"));
    }

    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let mut workspace_snapshot =
        load_workspace_for_submission(controller_runtime.clone(), conversation_id.as_deref())
            .await?;
    let (session_id, session_title, workspace_path) = materialize_prompt_session(
        &mut workspace_snapshot,
        project_id.as_deref(),
        conversation_id.as_deref(),
        &prompt,
    )?;
    let request = append_prompt_and_build_request(
        &mut workspace_snapshot,
        &agent_runtime,
        session_id,
        &session_title,
        &workspace_path,
        &prompt,
        execution_mode,
        reasoning_effort,
        |visible_prompt_snapshot| {
            store_and_emit_workbench_snapshot(
                &app,
                &state,
                &controller_runtime,
                &luaskills_catalog,
                &token_budget_runtime,
                &mcp_catalog,
                &agent_runtime.provider_configuration,
                visible_prompt_snapshot.clone(),
                &logs,
            )
            .map(|_| ())
        },
    )
    .await?;

    persist_workspace_session_update(controller_runtime.clone(), &workspace_snapshot, session_id)
        .await?;
    let workbench = store_and_emit_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        workspace_snapshot.clone(),
        &logs,
    )?;
    spawn_agent_stream_task(
        app,
        state.inner().clone(),
        controller_runtime,
        agent_runtime,
        request,
    );
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
    controller_runtime
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            controller_runtime.sqlite_binding_id.as_str(),
            "UPDATE sessions SET title = ?1, title_source = 'manual', updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_name),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(parsed_session_id.to_string()),
            ]],
        )
        .await
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
    let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    else {
        return refresh_workbench_snapshot(state).await;
    };
    if !runtime_state.ui_state.unread {
        return refresh_workbench_snapshot(state).await;
    }
    runtime_state.ui_state.unread = false;
    runtime_state.updated_at = current_unix_millis_text();
    vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
        controller_runtime,
        session_id,
        runtime_state,
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
        mut workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(empty_luaskills_catalog);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(empty_mcp_catalog);
        let logs = guard.startup.recent_logs.clone();
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            provider_configuration,
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
    let selected_messages =
        in_memory_session_messages_for_service(&Some(workspace_snapshot.clone()), session_id)
            .unwrap_or_default();
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &workspace_snapshot,
        selected_messages.as_slice(),
        &logs,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
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
    let luaskills_catalog = guard
        .luaskills_runtime
        .as_ref()
        .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
        .unwrap_or_else(empty_luaskills_catalog);
    let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
        String::from("service is still starting; token budget runtime is not ready")
    })?;
    let mcp_catalog = guard
        .mcp_runtime
        .as_ref()
        .map(TauriMcpRuntimeHandle::catalog)
        .unwrap_or_else(empty_mcp_catalog);
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
    let mut workspace_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    if let Some(session_id) = conversation_id.and_then(parse_session_id) {
        match vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime,
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
            }
            Err(error) if is_missing_session_manifest_error(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(workspace_snapshot)
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

/// Appends the visible user prompt and builds the shared runtime execution request.
/// 追加可见用户提示词，并构建共享运行时执行请求。
async fn append_prompt_and_build_request(
    workspace_snapshot: &mut WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    session_title: &str,
    workspace_path: &str,
    prompt: &str,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    emit_visible_prompt_snapshot: impl FnOnce(&WorkspaceSnapshot) -> Result<(), String>,
) -> Result<ExecutionPreparationRequest, String> {
    let now = current_unix_millis_text();
    let prompt_entry_id = format!("webui-prompt-{session_id}-{now}");
    let (injection_ops, sanitized_prompt) =
        collect_builtin_manual_injection_ops_and_sanitized_prompt(prompt);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.ui_state.unread = false;
    if runtime_state.conversation_state.records.is_empty() {
        if let Some(messages) = workspace_snapshot.session_messages.get(&session_id) {
            runtime_state.conversation_state = conversation_state_from_session_messages(messages);
        }
    }
    append_user_prompt_record(runtime_state, session_id, &now, &sanitized_prompt);
    let system_prompt_hook_phase = vmm_profile_placement_phase(
        runtime_state.conversation_state.messages.len(),
        runtime_state.last_compaction_id.as_deref(),
        runtime_state
            .vmm_profile_sync
            .covered_compaction_id
            .as_deref(),
    );
    let hook_conversation_state = runtime_state.conversation_state.clone();
    workspace_snapshot.session_messages.insert(
        session_id,
        conversation_state_to_session_messages(&runtime_state.conversation_state),
    );
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::MemoryRecall);
    workspace_snapshot.hydrated_session_ids.insert(session_id);
    emit_visible_prompt_snapshot(workspace_snapshot)?;

    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            workspace_snapshot,
        ));
    let owning_project_id = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .and_then(|session| session.project_id.clone())
        .filter(|project_id| !project_id.trim().is_empty());
    let precheck_request = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        hook_runtime.prepare_user_prompt_precheck(
            session_id,
            &prompt_entry_id,
            owning_project_id,
            workspace_path.to_string(),
            PendingSessionPromptSource::UserFollowup,
            system_prompt_hook_phase,
            &sanitized_prompt,
            &hook_conversation_state,
        )
    };
    let precheck_outcome = precheck_request.run().await;
    let mut request_prompt = sanitized_prompt;
    let mut system_prompt_hook_blocks = Vec::new();
    if !injection_ops.is_empty()
        && let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
    {
        runtime_state
            .context_injection_state
            .apply_inserts_for_turn(injection_ops, &now, Some(&precheck_outcome.turn_id));
        runtime_state.updated_at = now.clone();
    }
    match precheck_outcome.report {
        Ok(report) => {
            if let Some(effect) = vmm_profile_sync_effect_from_report(&report)
                && let Some(runtime_state) = workspace_snapshot
                    .session_runtime_state
                    .get_mut(&session_id)
            {
                runtime_state.vmm_profile_sync = effect.state;
                runtime_state.updated_at = current_unix_millis_text();
            }
            if let Some(rewritten_prompt) = report
                .payload
                .value
                .get("user_content")
                .and_then(Value::as_str)
                .filter(|content| *content != request_prompt)
            {
                request_prompt = rewritten_prompt.to_string();
                patch_latest_prechecked_user_prompt_record(
                    workspace_snapshot,
                    session_id,
                    &request_prompt,
                );
            }
            if !report.injections.is_empty()
                && let Some(runtime_state) = workspace_snapshot
                    .session_runtime_state
                    .get_mut(&session_id)
            {
                let now = current_unix_millis_text();
                runtime_state
                    .context_injection_state
                    .apply_inserts_for_turn(
                        report.injections,
                        &now,
                        Some(&precheck_outcome.turn_id),
                    );
                runtime_state.updated_at = now;
            }
            system_prompt_hook_blocks = report.system_prompt_blocks;
        }
        Err(error) => {
            if let Some(runtime_state) = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
            {
                append_runtime_event_projection_to_state(
                    runtime_state,
                    session_id,
                    &current_unix_millis_text(),
                    &RuntimeEvent::new(
                        RuntimeEventKind::Failed,
                        "Precheck hook 执行失败",
                        format!(
                            "会话 `{session_title}` 的 precheck 未能完成，已继续真实请求：{error}"
                        ),
                    ),
                );
            }
        }
    }
    workspace_snapshot.session_messages.insert(
        session_id,
        conversation_state_to_session_messages(
            &workspace_snapshot
                .session_runtime_state
                .get(&session_id)
                .map(|runtime_state| runtime_state.conversation_state.clone())
                .unwrap_or_default(),
        ),
    );
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            workspace_snapshot,
        ));

    let project_configuration = load_project_configuration(workspace_path);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .unwrap_or_default();

    Ok(ExecutionPreparationRequest {
        session_id,
        session_title: session_title.to_string(),
        workspace_path: workspace_path.to_string(),
        prompt: request_prompt,
        conversation_state: runtime_state.conversation_state.clone(),
        execution_mode: execution_mode.unwrap_or_else(|| String::from("build")),
        temperature: runtime_state.active_temperature,
        reasoning_effort: reasoning_effort
            .as_deref()
            .and_then(parse_reasoning_effort)
            .or(runtime_state.active_reasoning_effort)
            .unwrap_or(ReasoningEffortLevel::XHigh),
        ai_tool_configuration: agent_runtime.provider_configuration.ai_tools.clone(),
        prompt_purpose: ExecutionPromptPurpose::UserTurn,
        project_prompt_skills: project_configuration.prompt_skills,
        session_prompt_skill_overrides: runtime_state.prompt_skill_overrides.clone(),
        context_injection: core_provider_context_injection_from_state_for_turn(
            &runtime_state.context_injection_state,
            Some(&precheck_outcome.turn_id),
        ),
        system_prompt_hook_blocks,
        project_vmm: project_configuration.vmm,
        session_vmm: runtime_state.vmm.clone(),
    })
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

/// Rewrites the latest visible user prompt after a precheck transform edits it.
/// 在 precheck transform 改写提示词后回写最近一条可见用户提示词。
fn patch_latest_prechecked_user_prompt_record(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    prompt: &str,
) {
    let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    else {
        return;
    };
    if let Some(message) = runtime_state
        .conversation_state
        .messages
        .iter_mut()
        .rev()
        .find(|message| matches!(message.payload, SessionConversationPayload::UserText { .. }))
        && let SessionConversationPayload::UserText { text, .. } = &mut message.payload
    {
        *text = prompt.to_string();
    }
    if let Some(record) = runtime_state
        .conversation_state
        .records
        .iter_mut()
        .rev()
        .find(|record| {
            record.visible
                && record.kind == SessionMessageKind::User
                && record.record_kind == SessionMessageRecordKind::Transcript
                && record.title == "用户指令"
        })
    {
        record.body = prompt.to_string();
        record.metadata = None;
    }
    runtime_state.updated_at = current_unix_millis_text();
}

/// Builds the shared VMM hook snapshot from the current Tauri workspace state.
/// 根据当前 Tauri 工作区状态构建共享 VMM hook 快照。
fn build_vmm_hook_config_snapshot(
    provider_configuration: &ProviderConfigurationFile,
    workspace_snapshot: &WorkspaceSnapshot,
) -> VmmHookConfigSnapshot {
    let mut projects_by_workspace = std::collections::BTreeMap::new();
    for project in workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
    {
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

    VmmHookConfigSnapshot {
        global: provider_configuration.vmm.clone(),
        projects_by_workspace,
        sessions,
        outboxes,
        profile_sync,
        last_compaction_ids,
    }
}

/// Persists only the changed session after a WebUI runtime mutation.
/// 在 WebUI 运行时变更后仅持久化被修改的单个会话。
async fn persist_workspace_session_update(
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

/// Parses a user-facing optional session identifier and ignores local draft identifiers.
/// 解析用户界面传入的可选会话标识，并忽略本地草稿标识。
fn parse_session_id(value: &str) -> Option<SessionId> {
    value.parse::<SessionId>().ok()
}

/// Converts the composer reasoning string into the shared provider-config enum.
/// 将输入区思考档位字符串转换为共享 provider-config 枚举。
fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffortLevel> {
    match value {
        "off" => Some(ReasoningEffortLevel::Off),
        "low" => Some(ReasoningEffortLevel::Low),
        "medium" => Some(ReasoningEffortLevel::Medium),
        "high" => Some(ReasoningEffortLevel::High),
        "x-high" | "max" => Some(ReasoningEffortLevel::XHigh),
        _ => None,
    }
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
    workspace_snapshot: WorkspaceSnapshot,
    logs: &[String],
) -> Result<WorkbenchSnapshot, String> {
    let selected_messages =
        selected_session_messages_from_workspace(&workspace_snapshot).unwrap_or_default();
    let workbench = workbench_from_runtime(
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        provider_configuration,
        &workspace_snapshot,
        selected_messages.as_slice(),
        logs,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.workbench = workbench.clone();
    }
    let _ = app.emit(
        "vulcan://service-event",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
    Ok(workbench)
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
    tauri::async_runtime::spawn(async move {
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut stream = Box::pin(stream_runtime_run(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            agent_runtime.mcp_runtime.clone(),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Arc::clone(&agent_runtime.token_budget_runtime),
            request,
        ));

        while let Some(event) = stream.next().await {
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
                    if let Err(error) = apply_execution_preparation(&app, &state, &preparation) {
                        eprintln!("[tauri-service] apply preparation failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    if let Err(error) =
                        apply_session_phase(&app, &state, session_id, SessionRunPhase::Requesting)
                    {
                        eprintln!("[tauri-service] apply request-started failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    if let Err(error) =
                        apply_session_phase(&app, &state, session_id, SessionRunPhase::Thinking)
                    {
                        eprintln!("[tauri-service] apply thinking phase failed: {error}");
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &state,
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
                        collapse_streaming_thinking(&app, &state, session_id, &record_ids)
                    {
                        eprintln!("[tauri-service] complete thinking failed: {error}");
                    }
                    if let Err(error) =
                        apply_session_phase(&app, &state, session_id, SessionRunPhase::Requesting)
                    {
                        eprintln!("[tauri-service] restore request phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    if let Err(error) =
                        apply_session_phase(&app, &state, session_id, SessionRunPhase::Requesting)
                    {
                        eprintln!("[tauri-service] apply output phase failed: {error}");
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &state,
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
                    if is_provider_tool_boundary_runtime_event(&event) {
                        record_ids.thinking_record_id = None;
                    }
                    if matches!(event.kind, RuntimeEventKind::ToolStarted) {
                        if let Err(error) = apply_session_phase(
                            &app,
                            &state,
                            session_id,
                            SessionRunPhase::ToolRunning,
                        ) {
                            eprintln!("[tauri-service] apply tool phase failed: {error}");
                        }
                    }
                    if matches!(event.kind, RuntimeEventKind::ToolFinished) {
                        if let Err(error) = apply_session_phase(
                            &app,
                            &state,
                            session_id,
                            SessionRunPhase::Requesting,
                        ) {
                            eprintln!(
                                "[tauri-service] restore request phase after tool failed: {error}"
                            );
                        }
                    }
                    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
                        hook_runtime.record_tool_runtime_event(session_id, &event);
                    }
                    if let Err(error) =
                        append_runtime_event_projection(&app, &state, session_id, &event)
                    {
                        eprintln!("[tauri-service] append runtime event failed: {error}");
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
                        &state,
                        &controller_runtime,
                        session_id,
                        conversation_state,
                        last_successful_provider_usage,
                        provider_request_count,
                    )
                    .await
                    {
                        eprintln!("[tauri-service] apply checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::Finished(report) => {
                    if let Err(error) = apply_execution_report(
                        &app,
                        &state,
                        &controller_runtime,
                        &agent_runtime,
                        report,
                    )
                    .await
                    {
                        eprintln!("[tauri-service] apply final report failed: {error}");
                    }
                    break;
                }
            }
        }
    });
}

/// Applies execution preparation identity fields to the target session runtime state.
/// 将执行准备得到的身份字段应用到目标会话运行时状态。
fn apply_execution_preparation(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    preparation: &ExecutionPreparation,
) -> Result<(), String> {
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
        workspace
            .session_run_phases
            .insert(preparation.session_id, SessionRunPhase::Preparing);
    })
    .map(|_| ())
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
    with_workspace_mutation(app, state, |workspace| {
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
    with_workspace_mutation(app, state, |workspace| {
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
    with_workspace_mutation(app, state, |workspace| {
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

/// Applies one safe canonical conversation checkpoint and persists it through the shared store.
/// 应用一份安全规范会话检查点，并通过共享存储持久化。
async fn apply_conversation_checkpoint(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: vulcan_agent_runtime::conversation_protocol::SessionConversationState,
    last_successful_provider_usage: Option<vulcan_core::types::ProviderUsageSnapshot>,
    provider_request_count: usize,
) -> Result<(), String> {
    let workspace_snapshot = with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
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
    persist_workspace_session_update(controller_runtime.clone(), &workspace_snapshot, session_id)
        .await
        .map(|_| ())
}

/// Applies the terminal execution report and persists the final transcript state.
/// 应用终态执行报告，并持久化最终转录状态。
async fn apply_execution_report(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    report: ExecutionRunReport,
) -> Result<(), String> {
    let session_id = report.session_id;
    let workspace_snapshot = with_workspace_mutation(app, state, |workspace| {
        let now = current_unix_millis_text();
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
        if apply_result.is_success {
            let (hook_turn_id, vmm_writeback_effects) = agent_runtime
                .conversation_hook_runtime
                .lock()
                .map(|mut hook_runtime| {
                    hook_runtime.complete_turn(
                        session_id,
                        project_id.clone(),
                        report.assistant_output.as_deref().unwrap_or(
                            "The model completed the run but did not return visible text output.",
                        ),
                        report.provider_request_count,
                        &conversation_state,
                    )
                })
                .unwrap_or_default();
            runtime_state
                .context_injection_state
                .consume_turn(&hook_turn_id, &now);
            runtime_state.vmm_profile_sync.record_completed_turn();
            let profile_sync = runtime_state.vmm_profile_sync.clone();
            apply_vmm_writeback_effects_to_runtime_state(
                runtime_state,
                vmm_writeback_effects,
                &now,
            );
            agent_runtime
                .vmm_hook_runtime
                .store_profile_sync_state(session_id, profile_sync);
        } else if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
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
        agent_runtime
            .vmm_hook_runtime
            .replace_snapshot(build_vmm_hook_config_snapshot(
                &agent_runtime.provider_configuration,
                workspace,
            ));
    })?;
    persist_workspace_session_update(controller_runtime.clone(), &workspace_snapshot, session_id)
        .await
        .map(|_| ())
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
/// 修改保留的工作区快照并发出重建后的工作台快照。
fn with_workspace_mutation<F>(
    app: &tauri::AppHandle,
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<WorkspaceSnapshot, String>
where
    F: FnOnce(&mut WorkspaceSnapshot),
{
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        workspace_snapshot,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(empty_luaskills_catalog);
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(empty_mcp_catalog);
        let logs = guard.startup.recent_logs.clone();
        let mut workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        mutate(&mut workspace_snapshot);
        let selected_messages =
            selected_session_messages_from_workspace(&workspace_snapshot).unwrap_or_default();
        let workbench = workbench_from_runtime(
            &controller_runtime,
            &luaskills_catalog,
            &token_budget_runtime,
            &mcp_catalog,
            &provider_configuration,
            &workspace_snapshot,
            selected_messages.as_slice(),
            &logs,
        );
        guard.workspace_snapshot = Some(workspace_snapshot.clone());
        guard.workbench = workbench.clone();
        let _ = app.emit(
            "vulcan://service-event",
            ServiceEvent::Workbench(Box::new(workbench)),
        );
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            workspace_snapshot,
        )
    };
    let _ = (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    );
    Ok(workspace_snapshot)
}

/// Returns the selected session messages from runtime state before falling back to persisted rows.
/// 优先从运行时状态返回当前选中会话消息，缺失时回退到持久化行。
fn selected_session_messages_from_workspace(
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<Vec<SessionMessageEntry>> {
    in_memory_session_messages_for_service(
        &Some(workspace_snapshot.clone()),
        workspace_snapshot.selected_session,
    )
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
        previous_snapshot,
        provider_configuration,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let luaskills_catalog = guard
            .luaskills_runtime
            .as_ref()
            .and_then(|handle| crate::luaskills_bridge::load_luaskills_catalog(handle).ok())
            .unwrap_or_else(|| TauriLuaSkillsCatalog {
                runtime_root: PathBuf::new(),
                skills: Vec::new(),
                entry_count: 0,
            });
        let token_budget_runtime = guard.token_budget_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; token budget runtime is not ready")
        })?;
        let mcp_catalog = guard
            .mcp_runtime
            .as_ref()
            .map(TauriMcpRuntimeHandle::catalog)
            .unwrap_or_else(|| TauriMcpRuntimeCatalog {
                configured_server_count: 0,
                enabled_server_count: 0,
                connected_server_count: 0,
                tool_count: 0,
                prompt_count: 0,
                resource_count: 0,
                summary_rows: Vec::new(),
            });
        let logs = guard.startup.recent_logs.clone();
        let previous_snapshot = guard.workspace_snapshot.clone();
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        (
            controller_runtime,
            luaskills_catalog,
            token_budget_runtime,
            mcp_catalog,
            logs,
            previous_snapshot,
            provider_configuration,
        )
    };

    let mut refreshed_snapshot = vulcan_workspace_store::workspace_store::load_workspace_snapshot(
        controller_runtime.clone(),
    )
    .await?;
    preserve_active_session_run_state(previous_snapshot.as_ref(), &mut refreshed_snapshot);
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
                Err(error) if is_missing_session_manifest_error(&error) => None,
                Err(error) => return Err(error),
            };
            hydration_snapshot
                .map(|snapshot| snapshot.messages)
                .unwrap_or_default()
        };
    let workbench = workbench_from_runtime(
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &refreshed_snapshot,
        selected_messages.as_slice(),
        &logs,
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(refreshed_snapshot);
    }
    Ok(workbench)
}

/// Preserves in-memory running session markers across database-backed snapshot refreshes.
/// 在基于数据库重载快照时保留内存中的会话运行中标记。
fn preserve_active_session_run_state(
    previous_snapshot: Option<&WorkspaceSnapshot>,
    refreshed_snapshot: &mut WorkspaceSnapshot,
) {
    let Some(previous_snapshot) = previous_snapshot else {
        return;
    };
    if previous_snapshot.selected_session != SessionId::nil()
        && refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == previous_snapshot.selected_session)
    {
        refreshed_snapshot.selected_session = previous_snapshot.selected_session;
    }
    for previous_session in previous_snapshot
        .sessions
        .iter()
        .filter(|session| session.status == SessionState::Running)
    {
        if let Some(refreshed_session) = refreshed_snapshot
            .sessions
            .iter_mut()
            .find(|session| session.id == previous_session.id)
        {
            refreshed_session.status = SessionState::Running;
            refreshed_session.last_updated_label = previous_session.last_updated_label.clone();
        }
        let previous_phase = previous_snapshot
            .session_run_phases
            .get(&previous_session.id)
            .copied()
            .filter(|phase| is_active_session_run_phase(*phase))
            .unwrap_or(SessionRunPhase::Preparing);
        refreshed_snapshot
            .session_run_phases
            .insert(previous_session.id, previous_phase);
        if let Some(runtime_state) = previous_snapshot
            .session_runtime_state
            .get(&previous_session.id)
            .cloned()
        {
            refreshed_snapshot
                .session_runtime_state
                .insert(previous_session.id, runtime_state);
        }
        if let Some(messages) = previous_snapshot
            .session_messages
            .get(&previous_session.id)
            .cloned()
        {
            refreshed_snapshot
                .session_messages
                .insert(previous_session.id, messages);
        }
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

/// Returns whether a hydration failure is caused by an empty draft row without an event-store manifest.
/// 判断水合失败是否源自空草稿行缺少事件存储 manifest。
fn is_missing_session_manifest_error(error: &str) -> bool {
    error.contains("manifest.json")
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

/// Runs the real startup sequence shared with the legacy GUI.
/// 运行与旧版 GUI 共享的真实启动顺序。
fn run_service_bootstrap(app: &tauri::AppHandle) -> Result<(), String> {
    let mut logs = vec![format!(
        "[tauri-service] workspace root: {}",
        workspace_root().display()
    )];
    let controller_configuration =
        vulcan_provider_config::config::load_controller_configuration_file()?;
    let provider_configuration = vulcan_provider_config::config::load_configuration_file()?;

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
            .map_err(|error| format!("创建 Tauri service tokio runtime 失败：{error}"))?,
    );

    let controller_report = runtime.block_on(
        vulcan_workspace_store::data_plane::bootstrap_application_data_plane(
            controller_configuration.clone(),
        ),
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

    let luaskills_runtime =
        crate::luaskills_bridge::bootstrap_luaskills(controller_configuration.clone())?;
    let luaskills_catalog = crate::luaskills_bridge::load_luaskills_catalog(&luaskills_runtime)?;
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
        .map_err(|error| format!("MCP Runtime 初始化失败：{error}"))?;
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
    let agent_profile_catalog = load_agent_profile_catalog()?;
    let agent_luaskills_runtime = LuaSkillsRuntimeHandle {
        engine: Arc::clone(&luaskills_runtime.engine),
        runtime_root: luaskills_runtime.runtime_root.clone(),
        system_lua_lib_dir: luaskills_runtime.runtime_root.join("system_lua_lib"),
        skill_roots: luaskills_runtime.skill_roots.clone(),
        entry_count: luaskills_catalog.entry_count,
        operation_progress_events: Arc::new(Mutex::new(VecDeque::new())),
    };
    let agent_mcp_runtime = runtime
        .block_on(bootstrap_mcp_runtime_access(provider_configuration.clone()))
        .map_err(|error| format!("Agent MCP Runtime 初始化失败：{error}"))?;
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
    .map_err(|error| format!("VMM hook 注册失败：{error}"))?;
    let agent_runtime = AgentRuntimeBridge {
        provider_configuration: provider_configuration.clone(),
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

    let workspace_snapshot = runtime.block_on(
        vulcan_workspace_store::workspace_store::load_or_seed_workspace(controller_runtime.clone()),
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
    let hydration_snapshot = runtime.block_on(load_selected_session_hydration(
        controller_runtime.clone(),
        selected_session_id,
    ))?;
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
        &workspace_snapshot,
        hydration_snapshot
            .as_ref()
            .map(|snapshot| snapshot.messages.as_slice())
            .unwrap_or(&[]),
        &logs,
    );

    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.agent_runtime = Some(agent_runtime);
        guard.service_runtime = Some(runtime);
        guard.controller_runtime = Some(controller_runtime);
        guard.luaskills_runtime = Some(luaskills_runtime);
        guard.token_budget_runtime = Some(token_budget_runtime);
        guard.mcp_runtime = Some(mcp_runtime);
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

/// Updates the shared service state after a startup failure.
/// 在启动失败后更新共享服务状态。
fn update_service_failure(app: &tauri::AppHandle, error: String) {
    let logs = vec![
        format!("[tauri-service] startup failed: {error}"),
        format!(
            "[tauri-service] workspace root: {}",
            workspace_root().display()
        ),
    ];
    let startup = startup_snapshot_with_phase(
        "failed",
        startup_steps_with_status(
            OffsetDateTime::now_utc(),
            &[
                (
                    "service-process",
                    StartupStepStatus::Ready,
                    "Tauri 本地服务桥接已启动",
                ),
                ("vldb", StartupStepStatus::Error, error.as_str()),
                (
                    "luaskills",
                    StartupStepStatus::Pending,
                    "上游启动失败，未继续",
                ),
                (
                    "token-budget",
                    StartupStepStatus::Pending,
                    "上游启动失败，未继续",
                ),
                ("mcp", StartupStepStatus::Pending, "上游启动失败，未继续"),
                (
                    "workspace",
                    StartupStepStatus::Pending,
                    "上游启动失败，未继续",
                ),
            ],
        ),
        &logs,
    );
    let workbench = fallback_workbench_snapshot(&logs);
    if let Some(state) = app.try_state::<SharedServiceBridgeState>()
        && let Ok(mut guard) = state.lock()
    {
        guard.startup = startup.clone();
        guard.workbench = workbench.clone();
    }
    emit_service_snapshots(app, &startup, &workbench);
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
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    selected_messages: &[SessionMessageEntry],
    logs: &[String],
) -> WorkbenchSnapshot {
    let selected_conversation_id = if workspace.selected_session == SessionId::nil() {
        None
    } else {
        Some(workspace.selected_session.to_string())
    };
    let messages = selected_conversation_id
        .as_deref()
        .map(|conversation_id| {
            conversation_messages_from_session(conversation_id, selected_messages)
        })
        .unwrap_or_default();

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
            .filter(|session| session_event_manifest_exists(session))
            .map(|session| {
                let run_phase = workspace
                    .session_run_phases
                    .get(&session.id)
                    .copied()
                    .unwrap_or(SessionRunPhase::Idle);
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
                    run_phase: run_phase.to_string(),
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
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_luaskills(luaskills_catalog),
        settings: settings(),
        providers: providers(provider_configuration),
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
        })
        .collect()
}

/// Returns whether a persisted session has a readable event-store manifest on disk.
/// 返回持久化会话在磁盘上是否存在可读取的事件存储 manifest。
fn session_event_manifest_exists(session: &vulcan_workbench_model::SessionSummary) -> bool {
    let Some(project_id) = session.project_id.as_deref() else {
        return false;
    };
    user_vulcan_code_sessions_dir()
        .join(sanitize_session_store_component(project_id))
        .join(session.id.to_string())
        .join("manifest.json")
        .is_file()
}

/// Sanitizes one session-store path component with the same rules as the workspace event store.
/// 使用与工作区事件存储一致的规则清洗一个会话存储路径片段。
fn sanitize_session_store_component(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::from("_");
    }
    trimmed
        .chars()
        .map(|character| match character {
            ':' | '\\' | '/' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect()
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
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: Vec::new(),
        settings: settings(),
        providers: providers(&ProviderConfigurationFile::default()),
        memories: Vec::new(),
        command_sessions: Vec::new(),
        logs: logs
            .iter()
            .enumerate()
            .map(|(index, message)| {
                log_entry(&format!("fallback-{index}"), "info", "service", message)
            })
            .collect(),
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

/// Returns the user-level Vulcan Code session event-store directory.
/// 返回用户级 Vulcan Code 会话事件存储目录。
fn user_vulcan_code_sessions_dir() -> PathBuf {
    user_home_dir()
        .join(".vulcan")
        .join("code")
        .join("sessions")
}

/// Returns the current user's home directory using platform environment variables.
/// 使用平台环境变量返回当前用户主目录。
fn user_home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
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
