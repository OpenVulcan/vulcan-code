//! WebUI service bootstrap orchestration and startup snapshot projection.
//! WebUI 服务引导编排与启动快照投影。

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use time::OffsetDateTime;
use vulcan_agent_runtime::{
    agent_profile::load_agent_profile_catalog,
    agent_runtime::{
        conversation_hooks::ConversationHookRuntime,
        extension_generation::ExtensionGeneration,
        question_runtime::QuestionRuntime,
        task_runtime::TaskRuntime,
        token_budget_runtime::TokenBudgetRuntime,
        vmm_hooks::{VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks},
        vulcan_plugin_hooks::synchronize_vulcan_plugin_hooks,
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{bootstrap_mcp_runtime_access, plugin_mcp_server_configs, runtime_mcp_server_configs},
    project_temp_maintenance, runtime_assets,
    tooling::CommandSessionRuntime,
};
use vulcan_provider_config::config::ProviderConfigurationFile;
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, StartupSnapshot, StartupStepSnapshot, StartupStepStatus,
    WorkbenchSnapshot,
};
use vulcan_workspace_store::data_plane::ControllerRuntimeHandle;

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
                ("plugins", StartupStepStatus::Pending, "等待插件目录初始化"),
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
    let knowledge_runtime = vulcan_workspace_store::knowledge_base::KnowledgeBaseRuntime::new(
        controller_runtime.clone(),
    );
    bootstrap_step(
        runtime.block_on(knowledge_runtime.recover_interrupted_jobs()),
        "vldb",
        "VLDB",
    )?;

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
                    ("plugins", StartupStepStatus::Pending, "等待插件目录初始化"),
                    (
                        "workspace",
                        StartupStepStatus::Pending,
                        "等待工作区数据库载入",
                    ),
                ],
            )
        },
    );

    // LuaSkills bootstrap is an optional execution adapter; catalog-only plugins must survive its failure.
    // LuaSkills 引导属于可选执行适配器；仅含内容的插件必须能在其失败时继续工作。
    let (luaskills_runtime, luaskills_catalog, luaskills_step_status, luaskills_step_message) =
        match vulcan_service_runtime::luaskills_bridge::bootstrap_luaskills(
            controller_configuration.clone(),
            |message| update_luaskills_bootstrap_progress(state, event_emitter, &logs, message),
            |progress| {
                update_managed_runtime_bootstrap_progress(state, event_emitter, &logs, progress)
            },
        ) {
            Ok(runtime_handle) => {
                // managed_runtime_report is diagnostic-only and cannot invalidate an otherwise usable engine.
                // managed_runtime_report 仅用于诊断，不能使原本可用的引擎失效。
                match runtime_handle.managed_runtime_report.lock() {
                    Ok(report) => logs.push(format!(
                        "[webui-service] managed runtimes: status={} platform={} install_attempted={} diagnostic={}",
                        report.status.as_key(),
                        report.platform.as_deref().unwrap_or("unknown"),
                        report.install_attempted,
                        report.diagnostic,
                    )),
                    Err(_) => logs.push(String::from(
                        "[webui-service] LuaSkills managed-runtime diagnostic lock is poisoned",
                    )),
                }
                match vulcan_service_runtime::luaskills_bridge::load_luaskills_catalog(
                    &runtime_handle,
                ) {
                    Ok(catalog) => {
                        logs.push(format!(
                            "[webui-service] luaskills ready: root={} skills={} entries={}",
                            catalog.runtime_root.display(),
                            catalog.skills.len(),
                            catalog.entry_count
                        ));
                        (
                            Some(runtime_handle),
                            catalog,
                            StartupStepStatus::Ready,
                            String::from("LuaSkills 引擎与技能目录已就绪"),
                        )
                    }
                    Err(error) => {
                        logs.push(format!(
                            "[webui-service] LuaSkills catalog unavailable; continuing without Lua tools: {error}"
                        ));
                        (
                            Some(runtime_handle),
                            crate::workspace_persistence_versions::empty_luaskills_catalog(),
                            StartupStepStatus::Warning,
                            String::from("LuaSkills 目录不可用，插件内容组件继续运行"),
                        )
                    }
                }
            }
            Err(error) => {
                logs.push(format!(
                    "[webui-service] LuaSkills unavailable; continuing with content plugins: {error}"
                ));
                (
                    None,
                    crate::workspace_persistence_versions::empty_luaskills_catalog(),
                    StartupStepStatus::Warning,
                    String::from("LuaSkills 不可用，插件内容组件继续运行"),
                )
            }
        };
    let (vulcan_plugin_runtime, vulcan_plugin_catalog) = prepare_vulcan_plugin_runtime(
        luaskills_runtime.as_ref(),
        &provider_configuration,
        &controller_runtime,
        runtime.handle().clone(),
        event_emitter.clone(),
    )?;
    // pluginPrewarmTask overlaps the cold System VM creation cost with independent token-budget and MCP initialization.
    // pluginPrewarmTask 将冷启动 System VM 的创建成本与独立的 Token 预算及 MCP 初始化并行重叠。
    let plugin_prewarm_runtime = vulcan_plugin_runtime.clone();
    let plugin_prewarm_task = runtime.spawn(async move {
        // prewarmStarted measures only plugin work and excludes later MCP join delay.
        // prewarmStarted 仅测量插件工作，不包含后续等待 MCP 完成的时间。
        let prewarm_started = Instant::now();
        let result = plugin_prewarm_runtime
            .prewarm_enabled_plugin_runtimes()
            .await;
        (result, prewarm_started.elapsed())
    });

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
                        luaskills_step_status.clone(),
                        luaskills_step_message.as_str(),
                    ),
                    (
                        "token-budget",
                        StartupStepStatus::Running,
                        "正在初始化 Token 预算运行时",
                    ),
                    ("mcp", StartupStepStatus::Pending, "等待 Token 预算运行时"),
                    (
                        "plugins",
                        StartupStepStatus::Running,
                        "正在后台预热已启用插件 Hook 运行时",
                    ),
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
                    luaskills_step_status.clone(),
                    luaskills_step_message.as_str(),
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                (
                    "mcp",
                    StartupStepStatus::Running,
                    "正在并行初始化服务 MCP Runtime 与插件 Hook 运行时",
                ),
                (
                    "plugins",
                    StartupStepStatus::Running,
                    "正在后台预热已启用插件 Hook 运行时",
                ),
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
    update_startup(
        state,
        event_emitter,
        "bootstrapping-plugins",
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
                        luaskills_step_status.clone(),
                        luaskills_step_message.as_str(),
                    ),
                    (
                        "token-budget",
                        token_budget_status(&token_budget_runtime),
                        token_budget_runtime.status_row().as_str(),
                    ),
                    ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                    (
                        "plugins",
                        StartupStepStatus::Running,
                        "正在等待后台插件 Hook 运行时预热完成",
                    ),
                    (
                        "workspace",
                        StartupStepStatus::Pending,
                        "等待插件运行时初始化",
                    ),
                ],
            )
        },
    );
    let (prewarmed_plugin_ids, plugin_prewarm_elapsed) =
        runtime.block_on(plugin_prewarm_task).map_err(|error| {
            ServiceBootstrapError::new(
                "plugins",
                "Plugin Hook Runtime",
                format!("插件 Hook 运行时预热线程失败：{error}"),
            )
        })?;
    let prewarmed_plugin_ids = prewarmed_plugin_ids.map_err(|error| {
        ServiceBootstrapError::new(
            "plugins",
            "Plugin Hook Runtime",
            format!("预热已启用插件 Hook 运行时失败：{error}"),
        )
    })?;
    logs.push(format!(
        "[webui-service] Vulcan plugins ready: plugins={} tools={} prewarmed_runtimes={} revision={} elapsed_ms={}",
        vulcan_plugin_catalog.plugins.len(),
        vulcan_plugin_catalog.enabled_tools.len(),
        prewarmed_plugin_ids.len(),
        vulcan_plugin_catalog.revision,
        plugin_prewarm_elapsed.as_millis()
    ));
    let agent_profile_catalog = load_agent_profile_catalog()
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    // agent_luaskills_runtime is absent in degraded mode while the independent plugin runtime remains mounted.
    // agent_luaskills_runtime 在降级模式下为空，而独立插件运行时仍保持挂载。
    let agent_luaskills_runtime = luaskills_runtime.as_ref().and_then(|runtime_handle| {
        let engine = match runtime_handle.engine.lock() {
            Ok(engine) => engine,
            Err(_) => {
                logs.push(String::from(
                    "[webui-service] LuaSkills engine lock is poisoned; provider Lua tools are disabled",
                ));
                return None;
            }
        };
        let tool_entries = match engine.list_entries_for_authority(
            vulcan_service_runtime::luaskills::SkillManagementAuthority::System,
        ) {
            Ok(tool_entries) => tool_entries,
            Err(error) => {
                logs.push(format!(
                    "[webui-service] LuaSkills provider tool entries are unavailable: {error}"
                ));
                return None;
            }
        };
        Some(LuaSkillsRuntimeHandle::from_shared_engine(
            Arc::clone(&runtime_handle.engine),
            runtime_handle.runtime_root.clone(),
            runtime_handle.skill_roots.clone(),
            tool_entries.len(),
            tool_entries,
            runtime_handle
                .effective_runtime_settings
                .ai_tools
                .enable_run_lua,
            runtime_handle
                .effective_runtime_settings
                .ai_tools
                .enable_run_lua_session,
        ))
    });
    update_startup(
        state,
        event_emitter,
        "bootstrapping-agent-mcp",
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
                        luaskills_step_status.clone(),
                        luaskills_step_message.as_str(),
                    ),
                    (
                        "token-budget",
                        token_budget_status(&token_budget_runtime),
                        token_budget_runtime.status_row().as_str(),
                    ),
                    (
                        "mcp",
                        StartupStepStatus::Running,
                        "正在初始化 Agent MCP Runtime",
                    ),
                    (
                        "plugins",
                        StartupStepStatus::Ready,
                        "插件目录与已启用 Hook 运行时已就绪",
                    ),
                    (
                        "workspace",
                        StartupStepStatus::Pending,
                        "等待 Agent MCP Runtime 就绪",
                    ),
                ],
            )
        },
    );
    let mut mcp_server_configs =
        runtime_mcp_server_configs(&provider_configuration).map_err(|error| {
            ServiceBootstrapError::new(
                "mcp",
                "MCP Runtime",
                format!("Agent MCP Runtime 配置无效：{error}"),
            )
        })?;
    let plugin_secret_store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
    mcp_server_configs.extend(
        plugin_mcp_server_configs(&vulcan_plugin_runtime, &plugin_secret_store).map_err(
            |error| {
                ServiceBootstrapError::new(
                    "mcp",
                    "Plugin MCP Runtime",
                    format!("插件 MCP Runtime 配置无效：{error}"),
                )
            },
        )?,
    );
    let agent_mcp_runtime = runtime
        .block_on(bootstrap_mcp_runtime_access(mcp_server_configs))
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
    synchronize_vulcan_plugin_hooks(
        conversation_hook_runtime.dispatcher_mut(),
        vulcan_plugin_runtime.clone(),
        &vulcan_plugin_catalog,
    )
    .map_err(|error| {
        ServiceBootstrapError::new(
            "workspace",
            "Workspace",
            format!("Vulcan Plugin hook 注册失败：{error}"),
        )
    })?;
    let agent_runtime = AgentRuntimeBridge {
        provider_configuration: provider_configuration.clone(),
        ai_tool_configuration: ai_tool_configuration.clone(),
        agent_profile_catalog,
        luaskills_runtime: agent_luaskills_runtime,
        vulcan_plugin_runtime: vulcan_plugin_runtime.clone(),
        extension_generation: Arc::new(ExtensionGeneration::new(
            vulcan_plugin_catalog.revision,
            Some(Arc::clone(&vulcan_plugin_catalog)),
            agent_mcp_runtime,
        )),
        task_runtime: Arc::new(TaskRuntime::new()),
        question_runtime: Arc::new(QuestionRuntime::new()),
        command_session_runtime: Arc::new(CommandSessionRuntime::new()),
        token_budget_runtime: agent_token_budget_runtime,
        conversation_hook_runtime: Arc::new(Mutex::new(conversation_hook_runtime)),
        vmm_hook_runtime,
    };
    vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::set_runtime_event_projection_barriers_enabled(true)
        .map_err(|error| {
            ServiceBootstrapError::new(
                "service-process",
                "Core Service",
                format!("启用运行时事件投影屏障失败：{error}"),
            )
        })?;
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
                    luaskills_step_status.clone(),
                    luaskills_step_message.as_str(),
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                (
                    "plugins",
                    StartupStepStatus::Ready,
                    "插件目录与已启用运行时已就绪",
                ),
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
    // project_paths contains the exact persisted roots for both visible and archived projects.
    // project_paths 包含可见项目与归档项目各自持久化的精确根目录。
    let project_paths = workspace_snapshot
        .task_projects
        .iter()
        .chain(workspace_snapshot.archived_task_projects.iter())
        .map(|project| PathBuf::from(&project.path))
        .collect::<Vec<_>>();
    match project_temp_maintenance::maintain_project_temp_dirs(&project_paths) {
        Ok(()) => logs.push(String::from(
            "[webui-service] project temp maintenance completed",
        )),
        Err(error) => logs.push(format!(
            "[webui-service] project temp maintenance warning: {error}"
        )),
    }
    let selected_session_id = workspace_snapshot.selected_session;
    let vmm_hook_snapshot =
        build_vmm_hook_config_snapshot(&provider_configuration, &workspace_snapshot);
    bootstrap_step(
        agent_runtime
            .vmm_hook_runtime
            .seed_profile_sync(vmm_hook_snapshot.profile_sync.clone()),
        "workspace",
        "Workspace",
    )?;
    bootstrap_step(
        agent_runtime
            .vmm_hook_runtime
            .replace_snapshot(vmm_hook_snapshot),
        "workspace",
        "Workspace",
    )?;
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
                    luaskills_step_status.clone(),
                    luaskills_step_message.as_str(),
                ),
                (
                    "token-budget",
                    token_budget_status(&token_budget_runtime),
                    token_budget_runtime.status_row().as_str(),
                ),
                ("mcp", StartupStepStatus::Ready, "MCP Runtime 已初始化"),
                (
                    "plugins",
                    StartupStepStatus::Ready,
                    "插件目录与已启用运行时已就绪",
                ),
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
        let mcp_server_states = agent_runtime
            .extension_generation
            .mcp_runtime()
            .server_states();
        let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
            controller: &controller_runtime,
            command_session_runtime: Some(agent_runtime.command_session_runtime.as_ref()),
            luaskills_catalog: &luaskills_catalog,
            vulcan_plugin_catalog: Some(vulcan_plugin_catalog.as_ref()),
            mcp_server_states: &mcp_server_states,
            token_budget_runtime: &token_budget_runtime,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &provider_configuration,
            ai_tool_configuration: &ai_tool_configuration,
            workspace: &workspace_snapshot,
            surface_messages: surface_messages.as_slice(),
            subagent_navigation,
            pending_questions: pending_question_snapshots_from_agent_runtime(Some(&agent_runtime))
                .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?,
            logs: &logs,
            live_context_budgets: &BTreeMap::new(),
            session_goal_summaries: &BTreeMap::new(),
            hydrated_conversation_windows: &hydrated_conversation_windows,
            native_tools: native_tool_summaries_from_agent_runtime(Some(&agent_runtime)),
        });
        crate::preserve_project_settings_revisions(&mut workbench, &guard.workbench);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workbench = workbench.clone();
        emitted_workbench = workbench;
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.agent_runtime = Some(agent_runtime);
        guard.service_runtime = Some(runtime.clone());
        guard.controller_runtime = Some(controller_runtime);
        guard.knowledge_runtime = Some(knowledge_runtime);
        guard.luaskills_runtime = luaskills_runtime;
        guard.cached_luaskills_catalog = Some(luaskills_catalog);
        guard.token_budget_runtime = Some(token_budget_runtime);
        guard.mcp_runtime = Some(mcp_runtime);
        guard.cached_mcp_catalog = Some(mcp_catalog);
    }
    // Managed Codebase Memory enablement survives restart; unavailable packaged assets degrade only that integration.
    // 托管 Codebase Memory 启用意图会跨重启保留；打包资产不可用时仅降级该集成。
    if let Err(error) = runtime.block_on(crate::reconcile_codebase_memory_runtime(state)) {
        if let Ok(guard) = state.lock() {
            guard
                .codebase_memory_controller
                .mark_runtime_unsynchronized(&error);
        }
        eprintln!("[webui-service] managed Codebase Memory bootstrap degraded: {error}");
    }
    // prompt_skill_config resolves the exact roots watched by the independent Agent Skills domain.
    // prompt_skill_config 解析独立 Agent Skills 领域监听的精确根目录。
    let prompt_skill_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &provider_configuration.prompt_skills,
    );
    // runtime_guard makes the retained service runtime current while the notify loop is spawned.
    // runtime_guard 在生成 notify 循环期间使保留的服务运行时成为当前运行时。
    let _runtime_guard = runtime.enter();
    crate::start_codebase_memory_recovery_monitor(state.clone(), event_emitter.clone());
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

/// Builds the host-owned plugin manager and validated catalog independently from LuaSkills availability.
/// 独立于 LuaSkills 可用状态构建宿主持有的插件管理器与已校验目录。
///
/// # Parameters
/// 参数
/// `luaskills_runtime` optionally supplies the execution engine, while the remaining arguments own catalog policy, storage, async bridging, and events.
/// `luaskills_runtime` 可选提供执行引擎，其余参数分别持有目录策略、存储、异步桥接与事件。
///
/// # Returns
/// 返回值
/// Returns the long-lived manager and immutable catalog used by background Hook prewarming and later runtime projection.
/// 返回供后台 Hook 预热及后续运行时投影使用的长生命周期管理器与不可变目录。
fn prepare_vulcan_plugin_runtime(
    luaskills_runtime: Option<&vulcan_service_runtime::luaskills_bridge::TauriLuaSkillsHandle>,
    provider_configuration: &ProviderConfigurationFile,
    controller_runtime: &ControllerRuntimeHandle,
    service_runtime: tokio::runtime::Handle,
    event_emitter: ServiceEventEmitter,
) -> Result<
    (
        vulcan_plugin_runtime::VulcanPluginRuntimeManager,
        Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    ),
    ServiceBootstrapError,
> {
    // bundledPluginRoot and userPluginRoot are managed by Vulcan instead of LuaSkills.
    // bundledPluginRoot 与 userPluginRoot 由 Vulcan 管理，而不是由 LuaSkills 管理。
    let bundled_plugin_root = runtime_assets::resolve_bundled_plugin_root().ok_or_else(|| {
        ServiceBootstrapError::new("plugins", "Plugins", "未找到 Vulcan 内建插件根目录。")
    })?;
    let user_plugin_root = runtime_assets::resolve_user_plugin_root().ok_or_else(|| {
        ServiceBootstrapError::new("plugins", "Plugins", "未找到 Vulcan 用户插件根目录。")
    })?;
    std::fs::create_dir_all(&bundled_plugin_root).map_err(|error| {
        ServiceBootstrapError::new(
            "plugins",
            "Plugins",
            format!(
                "创建内建插件根 `{}` 失败：{error}",
                bundled_plugin_root.display()
            ),
        )
    })?;
    std::fs::create_dir_all(&user_plugin_root).map_err(|error| {
        ServiceBootstrapError::new(
            "plugins",
            "Plugins",
            format!(
                "创建用户插件根 `{}` 失败：{error}",
                user_plugin_root.display()
            ),
        )
    })?;
    // pluginStateRoot is shared by the manager metadata and Controller-backed data isolation.
    // pluginStateRoot 由管理器元数据与 Controller 后端数据隔离共同使用。
    let plugin_state_root = runtime_assets::resolve_user_plugin_state_root().ok_or_else(|| {
        ServiceBootstrapError::new("plugins", "Plugins", "未找到 Vulcan 用户插件状态根目录。")
    })?;
    // bridge_package_root is only consumed by execution plugins; content plugins remain independent from this optional adapter path.
    // bridge_package_root 仅供执行型插件使用；内容型插件不依赖此可选适配器路径。
    let bridge_package_root = luaskills_runtime
        .map(|runtime| {
            runtime
                .runtime_root
                .join("system_lua_lib")
                .join("vulcan-plugin-bridge")
        })
        .or_else(|| {
            runtime_assets::resolve_luaskills_runtime_root()
                .map(|root| root.join("system_lua_lib").join("vulcan-plugin-bridge"))
        })
        .unwrap_or_else(|| plugin_state_root.join("unavailable-plugin-bridge"));
    let vulcan_plugin_runtime = vulcan_plugin_runtime::VulcanPluginRuntimeManager::without_runtime(
        bundled_plugin_root,
        user_plugin_root,
        bridge_package_root,
        runtime_assets::resolve_managed_runtime_distribution_root().ok_or_else(|| {
            ServiceBootstrapError::new(
                "plugins",
                "Plugins",
                "未找到 Vulcan 共享 Python/Node 发行根目录。",
            )
        })?,
        plugin_state_root.clone(),
        runtime_assets::resolve_user_plugin_runtime_root().ok_or_else(|| {
            ServiceBootstrapError::new(
                "plugins",
                "Plugins",
                "未找到 Vulcan 用户插件运行环境根目录。",
            )
        })?,
    );
    if let Some(luaskills_runtime) = luaskills_runtime {
        vulcan_plugin_runtime
            .attach_runtime_engine(Arc::clone(&luaskills_runtime.engine))
            .map_err(|error| {
                ServiceBootstrapError::new(
                    "plugins",
                    "Vulcan Plugins",
                    format!("附加 Vulcan 插件执行运行时失败：{error}"),
                )
            })?;
    }
    // storageService routes plugin database calls through the already-connected shared Controller client.
    // storageService 通过已连接的共享 Controller 客户端路由插件数据库调用。
    let storage_service = Arc::new(crate::plugin_storage::ControllerPluginStorageService::new(
        controller_runtime.client.clone(),
        service_runtime,
        plugin_state_root,
    ));
    // uiEventPublisher forwards revision-only invalidations through existing service event sinks.
    // uiEventPublisher 通过现有服务事件接收器转发仅含修订号的失效通知。
    let ui_event_publisher = Arc::new(crate::plugin_ui_events::ServicePluginUiEventPublisher::new(
        event_emitter,
    ));
    vulcan_plugin_runtime
        .install_host_services(vulcan_plugin_runtime::VulcanPluginHostServices {
            storage: Some(storage_service),
            ui_events: Some(ui_event_publisher),
        })
        .map_err(|error| {
            ServiceBootstrapError::new(
                "plugins",
                "Vulcan Plugins",
                format!("安装 Vulcan 插件宿主服务失败：{error}"),
            )
        })?;
    let vulcan_plugin_catalog = vulcan_plugin_runtime
        .refresh_catalog(
            &provider_configuration.luaskills.disabled_vulcan_plugins,
            &provider_configuration.luaskills.enabled_vulcan_plugins,
        )
        .map_err(|error| {
            ServiceBootstrapError::new(
                "plugins",
                "Vulcan Plugins",
                format!("加载 Vulcan 插件目录失败：{error}"),
            )
        })?;
    Ok((vulcan_plugin_runtime, vulcan_plugin_catalog))
}
