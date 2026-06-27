//! WebUI service bootstrap orchestration and startup snapshot projection.
//! WebUI 服务引导编排与启动快照投影。

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

use time::OffsetDateTime;
use vulcan_agent_runtime::{
    agent_profile::load_agent_profile_catalog,
    agent_runtime::{
        conversation_hooks::ConversationHookRuntime,
        question_runtime::QuestionRuntime,
        task_runtime::TaskRuntime,
        token_budget_runtime::TokenBudgetRuntime,
        vmm_hooks::{VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks},
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::bootstrap_mcp_runtime_access,
    tooling::CommandSessionRuntime,
};
use vulcan_provider_config::config::ProviderConfigurationFile;
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, StartupSnapshot, StartupStepSnapshot, StartupStepStatus,
    WorkbenchSnapshot,
};
use vulcan_workbench_model::SessionId;

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, build_vmm_hook_config_snapshot,
    emit_service_event_with_trace, fallback_workbench_snapshot,
    hydrate_selected_session_runtime_if_needed, install_webui_question_runtime_event_bridge,
    mark_webui_service_discovery_failed, mark_webui_service_discovery_ready,
    normalized_workspace_path_text_from_path, pending_question_snapshot_from_agent_runtime,
    token_budget_status, workbench_from_runtime, workspace_root,
};

/// Emits initial service events after the WebView is created.
/// 在 WebView 创建后发出初始服务事件。
pub fn emit_initial_service_events(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
) {
    if let Ok(guard) = state.lock() {
        emit_service_snapshots(event_emitter, &guard.startup, &guard.workbench);
    }
}

/// Starts the real service bootstrap on a background thread.
/// 在后台线程启动真实服务引导流程。
pub fn spawn_service_bootstrap(
    state: SharedServiceBridgeState,
    event_emitter: ServiceEventEmitter,
) {
    std::thread::Builder::new()
        .name(String::from("vulcan-webui-service-bootstrap"))
        .spawn(move || {
            if let Err(error) = run_service_bootstrap(&state, &event_emitter) {
                update_service_failure(&state, &event_emitter, error);
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

/// Loads the same standalone AI capability configuration used by the WebUI workbench.
/// 加载与 WebUI 工作台一致的独立 AI 能力配置。
fn load_service_ai_tool_configuration(
    provider_configuration: &ProviderConfigurationFile,
    logs: &mut Vec<String>,
) -> vulcan_agent_runtime::provider_config::PersistedAiToolConfiguration {
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
                        "[webui-service] ai capability configuration normalization save failed: {error}"
                    ));
                }
            }
            match loaded.source {
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::DedicatedFile => logs.push(String::from(
                    "[webui-service] ai capability configuration loaded from ~/.vulcan/vulcan-ai-capabilities.json",
                )),
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::LegacyProviderFile => {
                    match vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    ) {
                        Ok(()) => logs.push(String::from(
                            "[webui-service] legacy embedded ai capability configuration migrated to ~/.vulcan/vulcan-ai-capabilities.json",
                        )),
                        Err(error) => logs.push(format!(
                            "[webui-service] legacy ai capability configuration migration failed: {error}"
                        )),
                    }
                }
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::Default => logs.push(String::from(
                    "[webui-service] ai capability configuration uses built-in defaults",
                )),
            }
            loaded.configuration
        }
        Err(error) => {
            logs.push(format!(
                "[webui-service] ai capability configuration load failed, falling back: {error}"
            ));
            if provider_configuration.ai_tools.is_default() {
                vulcan_agent_runtime::provider_config::PersistedAiToolConfiguration::default()
            } else {
                provider_configuration.ai_tools.clone()
            }
        }
    }
}

/// Runs the real startup sequence owned by the WebUI service.
/// 运行由 WebUI 服务持有的真实启动顺序。
fn run_service_bootstrap(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
) -> Result<(), ServiceBootstrapError> {
    let mut logs = vec![format!(
        "[webui-service] workspace root: {}",
        normalized_workspace_path_text_from_path(&workspace_root())
    )];
    let controller_configuration =
        vulcan_provider_config::config::load_controller_configuration_file().map_err(|error| {
            ServiceBootstrapError::new("service-process", "Core Service", error)
        })?;
    let provider_configuration = vulcan_provider_config::config::load_configuration_file()
        .map_err(|error| ServiceBootstrapError::new("service-process", "Core Service", error))?;
    let ai_tool_configuration =
        load_service_ai_tool_configuration(&provider_configuration, &mut logs);

    update_startup(state, event_emitter, "bootstrapping-vldb", &logs, |now| {
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
        "[webui-service] vldb ready: {}",
        controller_report.handle.sqlite_path.display()
    ));
    let controller_runtime = controller_report.handle.clone();

    update_startup(
        state,
        event_emitter,
        "bootstrapping-luaskills",
        &logs,
        |now| {
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
        },
    );

    let luaskills_runtime = bootstrap_step(
        vulcan_service_runtime::luaskills_bridge::bootstrap_luaskills(
            controller_configuration.clone(),
            |message| update_luaskills_bootstrap_progress(state, event_emitter, &logs, message),
        ),
        "luaskills",
        "LuaSkills",
    )?;
    let luaskills_catalog = bootstrap_step(
        vulcan_service_runtime::luaskills_bridge::load_luaskills_catalog(&luaskills_runtime),
        "luaskills",
        "LuaSkills",
    )?;
    logs.push(format!(
        "[webui-service] luaskills ready: root={} skills={} entries={}",
        luaskills_catalog.runtime_root.display(),
        luaskills_catalog.skills.len(),
        luaskills_catalog.entry_count
    ));

    update_startup(
        state,
        event_emitter,
        "bootstrapping-token-budget",
        &logs,
        |now| {
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
        },
    );

    let token_budget_runtime =
        vulcan_service_runtime::token_budget_bridge::TauriTokenBudgetRuntime::initialize();
    logs.push(format!(
        "[webui-service] token budget: {}",
        token_budget_runtime.status_row()
    ));

    update_startup(state, event_emitter, "bootstrapping-mcp", &logs, |now| {
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
        .block_on(vulcan_service_runtime::mcp_bridge::bootstrap_mcp_runtime(
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
        "[webui-service] mcp ready: servers={}/{} connected={} tools={} prompts={} resources={}",
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
        engine.list_entries_for_authority(
            vulcan_service_runtime::luaskills::SkillManagementAuthority::System,
        )
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
    vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::set_runtime_event_projection_barriers_enabled(true);
    install_webui_question_runtime_event_bridge(
        event_emitter,
        state,
        &agent_runtime.question_runtime,
    )
    .map_err(|error| {
        ServiceBootstrapError::new(
            "service-process",
            "Core Service",
            format!("安装 ask 弹窗事件桥失败：{error}"),
        )
    })?;
    logs.push(String::from(
        "[webui-service] agent runtime bridge ready: provider/LuaSkills/MCP/token-budget handles are mounted",
    ));

    update_startup(state, event_emitter, "loading-workspace", &logs, |now| {
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
                    "正在载入项目与会话索引",
                ),
            ],
        )
    });

    let mut workspace_snapshot = bootstrap_step(
        runtime.block_on(
            vulcan_workspace_store::workspace_store::load_or_seed_workspace_index(
                controller_runtime.clone(),
            ),
        ),
        "workspace",
        "Workspace",
    )?;
    let selected_session_id = workspace_snapshot.selected_session;
    if selected_session_id != SessionId::nil() {
        bootstrap_step(
            runtime.block_on(hydrate_selected_session_runtime_if_needed(
                controller_runtime.clone(),
                &mut workspace_snapshot,
                selected_session_id,
            )),
            "workspace",
            "Workspace",
        )?;
    }
    let vmm_hook_snapshot =
        build_vmm_hook_config_snapshot(&provider_configuration, &workspace_snapshot);
    agent_runtime
        .vmm_hook_runtime
        .seed_profile_cache(vmm_hook_snapshot.profile_sync.clone());
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(vmm_hook_snapshot);
    logs.push(format!(
        "[webui-service] workspace loaded: projects={} sessions={} selected={}",
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
                    "项目与会话索引已载入",
                ),
            ],
        ),
        &logs,
    );
    let workbench = workbench_from_runtime(
        &controller_runtime,
        Some(agent_runtime.command_session_runtime.as_ref()),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &workspace_snapshot,
        &[],
        None,
        pending_question_snapshot_from_agent_runtime(Some(&agent_runtime)),
        &logs,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &[],
    );

    if let Ok(mut guard) = state.lock() {
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
    mark_webui_service_discovery_ready(state);
    emit_service_snapshots(event_emitter, &startup, &workbench);
    Ok(())
}

/// Updates startup state while a bootstrap step is running.
/// 在引导步骤运行中更新启动状态。
fn update_startup<F>(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    phase: &str,
    logs: &[String],
    build_steps: F,
) where
    F: FnOnce(OffsetDateTime) -> Vec<StartupStepSnapshot>,
{
    let startup = startup_snapshot_with_phase(phase, build_steps(OffsetDateTime::now_utc()), logs);
    if let Ok(mut guard) = state.lock() {
        guard.startup = startup.clone();
    }
    emit_service_event_with_trace(
        event_emitter,
        "startup_snapshot_emit",
        ServiceEvent::Startup(Box::new(startup)),
    );
}

/// Emits a LuaSkills-specific startup progress snapshot.
/// 发出 LuaSkills 专用启动进度快照。
fn update_luaskills_bootstrap_progress(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    logs: &[String],
    message: &str,
) {
    update_startup(
        state,
        event_emitter,
        "bootstrapping-luaskills",
        logs,
        |now| {
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
        },
    );
}

/// Updates the shared service state after a startup failure.
/// 在启动失败后更新共享服务状态。
fn update_service_failure(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    error: ServiceBootstrapError,
) {
    let logs = vec![
        format!(
            "[webui-service] startup failed at {}: {}",
            error.step_label, error.message
        ),
        format!(
            "[webui-service] workspace root: {}",
            normalized_workspace_path_text_from_path(&workspace_root())
        ),
    ];
    let startup = startup_snapshot_with_phase(
        "failed",
        startup_failure_steps(OffsetDateTime::now_utc(), &error),
        &logs,
    );
    let workbench = fallback_workbench_snapshot(&logs);
    if let Ok(mut guard) = state.lock() {
        guard.startup = startup.clone();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench.clone();
    }
    mark_webui_service_discovery_failed(state);
    emit_service_snapshots(event_emitter, &startup, &workbench);
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
        "workspace" => "项目与会话索引已载入",
        _ => "已就绪",
    }
    .to_string()
}

/// Emits startup and workbench snapshots to subscribed WebUI clients.
/// 向已订阅的 WebUI 客户端发送启动与工作台快照。
fn emit_service_snapshots(
    event_emitter: &ServiceEventEmitter,
    startup: &StartupSnapshot,
    workbench: &WorkbenchSnapshot,
) {
    emit_service_event_with_trace(
        event_emitter,
        "startup_snapshot_emit",
        ServiceEvent::Startup(Box::new(startup.clone())),
    );
    emit_service_event_with_trace(
        event_emitter,
        "service_workbench_snapshot_emit",
        ServiceEvent::Workbench(Box::new(workbench.clone())),
    );
}

/// Builds a startup snapshot for one phase.
/// 为单个阶段构建启动快照。
pub(super) fn startup_snapshot_with_phase(
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
