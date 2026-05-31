//! Local service bridge used by the Tauri WebUI.
//! Tauri WebUI 使用的本地服务桥接层。

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tauri::{Emitter, Manager};
use time::OffsetDateTime;
use vulcan_service_api::{
    CommandSessionSummary, ConversationMessage, ConversationSummary, FeatureAreaSummary,
    LogEntrySummary, MemorySummary, MessageRole, PluginSummary, ProjectSummary, ProviderSummary,
    RuntimeModuleSnapshot, ServiceEvent, SettingSectionSummary, StartupSnapshot,
    StartupStepSnapshot, StartupStepStatus, ToolCallSummary, WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionId, SessionMessageEntry, SessionMessageKind};
use vulcan_workspace_store::data_plane::ControllerRuntimeHandle;

use crate::luaskills_bridge::{TauriLuaSkillsCatalog, TauriLuaSkillsHandle};
use crate::mcp_bridge::{TauriMcpRuntimeCatalog, TauriMcpRuntimeHandle};
use crate::token_budget_bridge::TauriTokenBudgetRuntime;

/// Number of recent turns loaded for the selected conversation.
/// 为当前选中会话加载的最近轮次数量。
const SELECTED_SESSION_TURN_LOAD_LIMIT: usize = 24;

/// Shared service state exposed to Tauri commands.
/// 暴露给 Tauri 命令的共享服务状态。
#[derive(Debug)]
struct ServiceBridgeState {
    /// Latest startup snapshot.
    /// 最近一次启动快照。
    startup: StartupSnapshot,
    /// Latest workbench snapshot.
    /// 最近一次工作台快照。
    workbench: WorkbenchSnapshot,
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
            service_session_messages
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
    let controller_runtime = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
    let hydration_snapshot =
        vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
            controller_runtime,
            parsed_session_id,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await?;

    Ok(conversation_messages_from_session(
        &session_id,
        hydration_snapshot.messages.as_slice(),
    ))
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

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("创建 Tauri service tokio runtime 失败：{error}"))?;

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
    let selected_session_id = workspace_snapshot.selected_session;
    let hydration_snapshot = if selected_session_id == SessionId::nil() {
        None
    } else {
        Some(runtime.block_on(
            vulcan_workspace_store::workspace_store::load_session_hydration_snapshot(
                controller_runtime.clone(),
                selected_session_id,
                SELECTED_SESSION_TURN_LOAD_LIMIT,
            ),
        )?)
    };
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
            })
            .collect(),
        conversations: workspace
            .sessions
            .iter()
            .map(|session| ConversationSummary {
                id: session.id.to_string(),
                title: session.title.clone(),
                status: session.status.label().to_string(),
            })
            .collect(),
        selected_conversation_id,
        messages,
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: plugins_from_luaskills(luaskills_catalog),
        settings: settings(),
        providers: providers(),
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
        }],
        conversations: Vec::new(),
        selected_conversation_id: None,
        messages: Vec::new(),
        tool_calls: tool_calls(),
        feature_areas: feature_areas(),
        plugins: Vec::new(),
        settings: settings(),
        providers: providers(),
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
        feature_area(
            "chat",
            "控制台",
            "控制台、消息流与模型输入区",
            "真实快照",
        ),
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
fn providers() -> Vec<ProviderSummary> {
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

/// Returns the first existing path from a workspace-relative candidate list.
/// 从工作区相对候选列表中返回第一个存在的路径。
#[allow(dead_code)]
fn first_existing_path(workspace_root: &Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|candidate| workspace_root.join(candidate))
        .find(|path| path.exists())
}
