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
        system_plugin_hooks::synchronize_system_plugin_hooks,
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

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, WorkbenchRuntimeProjection,
    active_surface_messages_from_workspace_and_state, build_vmm_hook_config_snapshot,
    emit_service_event_with_trace, fallback_workbench_snapshot,
    hydrated_conversation_window_summaries, install_webui_question_runtime_event_bridge,
    mark_webui_service_discovery_failed, mark_webui_service_discovery_ready,
    native_tool_summaries_from_agent_runtime, normalized_workspace_path_text_from_path,
    pending_question_snapshots_from_agent_runtime, subagent_navigation_snapshot_from_cached_state,
    token_budget_status, workbench_from_runtime, workspace_root,
};

mod ai_tool_config;
mod error;
mod startup_snapshot;
mod startup_updates;

pub(super) use ai_tool_config::*;
pub(super) use error::*;
pub(super) use startup_snapshot::*;
pub(super) use startup_updates::*;

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
    if let Ok(mut guard) = state.lock() {
        guard.startup.ui_locale = provider_configuration.ui_locale.clone();
        guard.workbench.ui_locale = provider_configuration.ui_locale.clone();
    }
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
        engine
            .list_entries_for_authority(
                vulcan_service_runtime::luaskills::SkillManagementAuthority::System,
            )
            .map_err(|error| {
                ServiceBootstrapError::new(
                    "luaskills",
                    "LuaSkills",
                    format!("读取 Agent Runtime 工具入口失败：{error}"),
                )
            })?
    };
    let system_plugin_runtime = vulcan_plugin_runtime::SystemPluginRuntimeManager::new(
        Arc::clone(&luaskills_runtime.engine),
        luaskills_runtime.runtime_root.join("system_lua_lib"),
    );
    let system_plugin_catalog = system_plugin_runtime
        .refresh_catalog(
            &provider_configuration.luaskills.disabled_system_plugins,
            &provider_configuration.luaskills.enabled_system_plugins,
        )
        .map_err(|error| {
            ServiceBootstrapError::new(
                "luaskills",
                "System Plugins",
                format!("加载系统插件目录失败：{error}"),
            )
        })?;
    logs.push(format!(
        "[webui-service] system plugins ready: plugins={} tools={} revision={}",
        system_plugin_catalog.plugins.len(),
        system_plugin_catalog.enabled_tools.len(),
        system_plugin_catalog.revision
    ));
    let agent_luaskills_runtime = LuaSkillsRuntimeHandle {
        engine: Arc::clone(&luaskills_runtime.engine),
        runtime_root: luaskills_runtime.runtime_root.clone(),
        system_lua_lib_dir: luaskills_runtime.runtime_root.join("system_lua_lib"),
        skill_roots: luaskills_runtime.skill_roots.clone(),
        entry_count: luaskills_catalog.entry_count,
        tool_entries: Arc::new(Mutex::new(agent_luaskills_tool_entries)),
        operation_progress_events: Arc::new(Mutex::new(VecDeque::new())),
        system_plugin_runtime: system_plugin_runtime.clone(),
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
    synchronize_system_plugin_hooks(
        conversation_hook_runtime.dispatcher_mut(),
        system_plugin_runtime,
        &system_plugin_catalog,
    )
    .map_err(|error| {
        ServiceBootstrapError::new(
            "workspace",
            "Workspace",
            format!("System Plugin hook 注册失败：{error}"),
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

    let workspace_snapshot = bootstrap_step(
        runtime.block_on(
            vulcan_workspace_store::workspace_store::load_or_seed_workspace_index(
                controller_runtime.clone(),
            ),
        ),
        "workspace",
        "Workspace",
    )?;
    let selected_session_id = workspace_snapshot.selected_session;
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
    let mut emitted_workbench = fallback_workbench_snapshot(&logs);
    if let Ok(mut guard) = state.lock() {
        guard.startup = startup.clone();
        guard.agent_runtime = Some(agent_runtime.clone());
        let surface_messages =
            active_surface_messages_from_workspace_and_state(&mut guard, &workspace_snapshot)
                .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        let subagent_navigation =
            subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
                .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        let hydrated_conversation_windows = hydrated_conversation_window_summaries(
            &guard.hydrated_conversation_windows,
            &guard.webui_preloaded_unclosed_subagent_roots,
        );
        let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
            controller: &controller_runtime,
            command_session_runtime: Some(agent_runtime.command_session_runtime.as_ref()),
            luaskills_catalog: &luaskills_catalog,
            system_plugin_catalog: Some(system_plugin_catalog.as_ref()),
            token_budget_runtime: &token_budget_runtime,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &provider_configuration,
            ai_tool_configuration: &ai_tool_configuration,
            workspace: &workspace_snapshot,
            surface_messages: surface_messages.as_slice(),
            subagent_navigation,
            pending_questions: pending_question_snapshots_from_agent_runtime(Some(&agent_runtime)),
            logs: &logs,
            live_context_budgets: &BTreeMap::new(),
            session_goal_summaries: &BTreeMap::new(),
            hydrated_conversation_windows: &hydrated_conversation_windows,
            native_tools: native_tool_summaries_from_agent_runtime(Some(&agent_runtime)),
        });
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workbench = workbench.clone();
        emitted_workbench = workbench;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.agent_runtime = Some(agent_runtime);
        guard.service_runtime = Some(runtime.clone());
        guard.controller_runtime = Some(controller_runtime);
        guard.luaskills_runtime = Some(luaskills_runtime);
        guard.cached_luaskills_catalog = Some(luaskills_catalog);
        guard.token_budget_runtime = Some(token_budget_runtime);
        guard.mcp_runtime = Some(mcp_runtime);
        guard.cached_mcp_catalog = Some(mcp_catalog);
    }
    // prompt_skill_config resolves the exact roots watched by the independent Agent Skills domain.
    // prompt_skill_config 解析独立 Agent Skills 领域监听的精确根目录。
    let prompt_skill_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &provider_configuration.prompt_skills,
    );
    // runtime_guard makes the retained service runtime current while the notify loop is spawned.
    // runtime_guard 在生成 notify 循环期间使保留的服务运行时成为当前运行时。
    let _runtime_guard = runtime.enter();
    crate::agent_skills_watcher::start_agent_skills_catalog_watcher(
        state.clone(),
        event_emitter.clone(),
        vulcan_prompt_skills::configured_system_skill_root(&prompt_skill_config),
        vulcan_prompt_skills::configured_user_skill_root(&prompt_skill_config),
    )
    .map_err(|error| ServiceBootstrapError::new("workspace", "Agent Skills", error))?;
    mark_webui_service_discovery_ready(state);
    emit_service_snapshots(event_emitter, &startup, &emitted_workbench);
    Ok(())
}
