//! WebUI service bootstrap orchestration and startup snapshot projection.
//! WebUI 服务引导编排与启动快照投影。

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
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
    ServiceEvent, ServiceEventEmitter, StartupRecoveryKind, StartupRecoverySnapshot,
    StartupSnapshot, StartupStepSnapshot, StartupStepStatus, WorkbenchSnapshot,
};
use vulcan_workspace_store::data_plane::ControllerRuntimeHandle;

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, WorkbenchRuntimeProjection,
    active_surface_messages_from_workspace_and_state, build_vmm_hook_config_snapshot,
    current_unix_millis_text, emit_service_event_with_trace, fallback_workbench_snapshot,
    hydrated_conversation_window_summaries, install_webui_question_runtime_event_bridge,
    mark_webui_service_discovery_failed, mark_webui_service_discovery_ready,
    mark_webui_service_discovery_starting, native_tool_summaries_from_agent_runtime,
    normalized_workspace_path_text_from_path, pending_question_snapshots_from_agent_runtime,
    subagent_navigation_snapshot_from_cached_state, token_budget_status, workbench_from_runtime,
    workspace_root,
};

mod error;
mod recovery;
mod startup_snapshot;
mod startup_updates;

pub(super) use error::*;
pub(crate) use recovery::service_rebuild_startup_session_vmm;
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
    let public_ai_configuration = vulcan_provider_config::config::load_public_ai_connections_file()
        .map_err(|error| ServiceBootstrapError::new("service-process", "Core Service", error))?
        .configuration;
    if let Ok(mut guard) = state.lock() {
        guard.startup.ui_locale = provider_configuration.ui_locale.clone();
        guard.workbench.ui_locale = provider_configuration.ui_locale.clone();
    }
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
    let public_ai_runtime = Arc::new(
        vulcan_provider_adapters::PublicAiRuntime::first_batch(Arc::new(
            crate::ai_credential_secret_store::OsAiCredentialSecretStore,
        ))
        .map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("公共 AI 运行时初始化失败：{error}"),
            )
        })?,
    );
    public_ai_runtime
        .replace_configuration(public_ai_configuration.clone())
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    public_ai_runtime
        .configure_provider_task_recovery(
            &vulcan_workspace_store::data_plane::application_data_root()
                .join("provider-task-recovery"),
        )
        .map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("供应商异步任务恢复状态加载失败：{error}"),
            )
        })?;
    refresh_public_ai_catalogs_at_startup(
        public_ai_runtime.as_ref(),
        &public_ai_configuration,
        &mut logs,
    )?;
    // providerTaskRecoveryRuntime resumes only confirmed task identifiers and never replays
    // interrupted dispatch intents.
    // providerTaskRecoveryRuntime 仅恢复已确认任务标识，绝不重放中断的分派意图。
    let provider_task_recovery_runtime = Arc::clone(&public_ai_runtime);
    runtime.spawn(async move {
        loop {
            let configuration = match provider_task_recovery_runtime.configuration_snapshot() {
                Ok(configuration) => configuration,
                Err(error) => {
                    eprintln!(
                        "[webui-service] provider task recovery skipped because configuration is unavailable: {error}"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    continue;
                }
            };
            if let Err(error) = provider_task_recovery_runtime
                .recover_provider_tasks(&configuration)
                .await
            {
                eprintln!("[webui-service] provider task recovery observation failed: {error}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    });
    // usageRefreshRuntime owns the memory-only allowance cache and observes interval changes.
    // usageRefreshRuntime 持有仅内存用量缓存并观测刷新间隔变更。
    let usage_refresh_runtime = Arc::clone(&public_ai_runtime);
    runtime.spawn(async move {
        loop {
            let configuration = match usage_refresh_runtime.configuration_snapshot() {
                Ok(configuration) => configuration,
                Err(error) => {
                    eprintln!("[webui-service] public AI usage refresh skipped: {error}");
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
            };
            if let Err(error) = usage_refresh_runtime
                .refresh_supported_connection_usage(&configuration)
                .await
            {
                eprintln!("[webui-service] public AI usage refresh failed: {error}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(
                configuration
                    .usage_refresh_interval_seconds
                    .clamp(30, 86_400),
            ))
            .await;
        }
    });
    // configuredInferenceService shares the authoritative runtime while owning isolated execution state.
    // configuredInferenceService 共享权威运行时，同时拥有隔离的执行状态。
    let configured_inference_service = Arc::new(
        crate::configured_inference::ConfiguredInferenceService::new(Arc::clone(
            &public_ai_runtime,
        ))
        .with_retry_configuration(&provider_configuration.retry)
        .with_plugin_telemetry(vulcan_plugin_runtime.clone()),
    );
    let agent_runtime = AgentRuntimeBridge {
        provider_configuration: provider_configuration.clone(),
        public_ai_runtime,
        configured_inference_service,
        provider_id_override: None,
        model_id_override: None,
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

    let mut startup = startup_snapshot_with_phase(
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
    startup.ui_locale = provider_configuration.ui_locale.clone();
    let mut emitted_workbench = {
        // guard retains runtime and workbench state without publishing startup readiness before every fallible validation finishes.
        // guard 保留运行时与工作台状态，但不会在全部可失败校验完成前发布启动就绪。
        let mut guard = state.lock().map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("failed to lock service state during workspace bootstrap: {error}"),
            )
        })?;
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
        if selected_session_id != uuid::Uuid::nil() {
            // selected_vmm_controller eagerly validates and opens the selected session before its first UI projection.
            // selected_vmm_controller 在首次 UI 投影前主动校验并打开选中会话。
            let selected_vmm_controller = guard.vmm_controller_registry.get_or_open(
                &controller_runtime.data_root,
                selected_session_id,
                &current_unix_millis_text(),
            );
            if let Err(error) = selected_vmm_controller {
                // bootstrap_error preserves the strict diagnostic and adds recovery only for a storage-classified historical or damaged VMM attachment.
                // bootstrap_error 保留严格诊断，并且仅对存储层已分类的历史或损坏 VMM 附件附加恢复操作。
                let mut bootstrap_error =
                    ServiceBootstrapError::new("workspace", "Workspace", error.to_string());
                if error.requires_rebuild() {
                    // controller_runtime is retained only for the exact recovery action exposed by this failure.
                    // controller_runtime 仅为该失败暴露的精确恢复操作保留。
                    guard.controller_runtime = Some(controller_runtime.clone());
                    bootstrap_error = bootstrap_error.with_recovery(StartupRecoverySnapshot {
                        kind: StartupRecoveryKind::RebuildSessionVmm,
                        session_id: selected_session_id.to_string(),
                    });
                }
                return Err(bootstrap_error);
            }
        }
        // vmm_session_states are immutable projections from the controllers opened during bootstrap.
        // vmm_session_states 是引导期间已打开控制器产生的不可变投影。
        let vmm_session_states = guard
            .vmm_controller_registry
            .snapshots()
            .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
            controller: &controller_runtime,
            command_session_runtime: Some(agent_runtime.command_session_runtime.as_ref()),
            luaskills_catalog: &luaskills_catalog,
            vulcan_plugin_catalog: Some(vulcan_plugin_catalog.as_ref()),
            mcp_server_states: &mcp_server_states,
            token_budget_runtime: &token_budget_runtime,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &provider_configuration,
            public_ai_runtime: Some(agent_runtime.public_ai_runtime.as_ref()),
            workspace: &workspace_snapshot,
            vmm_session_states: &vmm_session_states,
            managed_vmm_runtime: None,
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
        workbench.providers = crate::public_ai_provider_rows(
            &public_ai_configuration,
            agent_runtime.public_ai_runtime.as_ref(),
        )
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        crate::preserve_project_settings_revisions(&mut workbench, &guard.workbench);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workbench = workbench.clone();
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
        workbench
    };
    // Managed VMM starts only after the configured inference service and shared Controller have
    // both been published into service state. Failure degrades memory without blocking the workbench.
    // 托管 VMM 仅在配置化推理服务与共享 Controller 都已发布到服务状态后启动。
    // 启动失败只降级记忆能力，不阻止工作台可用。
    runtime.block_on(reconcile_managed_memory_mesh_runtime(
        state,
        &public_ai_configuration,
        &provider_configuration.vmm,
        &mut logs,
    ));
    // The managed runtime is a separate lifecycle domain; publish its exact fields even when a
    // broader workspace refresh is temporarily unable to reopen one historical VMM session.
    // 托管运行时属于独立生命周期域；即使更广泛的工作区刷新暂时无法重新打开某个历史 VMM 会话，
    // 也要发布其精确字段。
    match crate::managed_memory_mesh_runtime::synchronize_workbench_runtime(state) {
        Ok(workbench) => emitted_workbench = workbench,
        Err(error) => logs.push(format!(
            "[webui-service] managed VMM projection degraded: {error}"
        )),
    }
    // Reproject the canonical workbench after managed VMM reconciliation so the first emitted
    // snapshot cannot retain the pre-launch external-runtime placeholder.
    // 托管 VMM 协调完成后重新投影规范工作台，避免首次发布的快照保留启动前的外部运行时占位状态。
    match runtime.block_on(crate::refresh_workbench_snapshot(state.clone())) {
        Ok(workbench) => emitted_workbench = workbench,
        Err(error) => logs.push(format!(
            "[webui-service] post-managed workspace refresh degraded: {error}"
        )),
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
    {
        // guard publishes ready only after VMM opening, workbench projection, and watcher startup all succeed.
        // guard 仅在 VMM 打开、工作台投影与监听器启动全部成功后发布 ready。
        let mut guard = state.lock().map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("failed to lock service state while publishing startup readiness: {error}"),
            )
        })?;
        guard.startup = startup.clone();
    }
    mark_webui_service_discovery_ready(state);
    emit_service_snapshots(event_emitter, &startup, &emitted_workbench);
    Ok(())
}

/// Starts the configured host-managed VMM child and installs an in-memory transport overlay.
/// 启动已配置的宿主管理 VMM 子进程，并安装内存态传输覆盖。
///
/// # Parameters
/// 参数
/// `state` owns published service dependencies, `runtime` executes asynchronous startup,
/// `ai_configuration` owns the exact VMM embedding route, and `vmm_config` controls local management.
/// `state` 持有已发布服务依赖，`runtime` 执行异步启动，`ai_configuration` 持有精确 VMM
/// 向量路由，`vmm_config` 控制本地托管及运行时行为。
///
/// # Returns
/// 返回值
/// This function records a controlled degraded diagnostic instead of failing global bootstrap.
/// 本函数记录受控降级诊断，而不让全局引导失败。
pub(crate) async fn reconcile_managed_memory_mesh_runtime(
    state: &SharedServiceBridgeState,
    ai_configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    vmm_config: &vulcan_vmm::VmmConfigLayer,
    logs: &mut Vec<String>,
) {
    let Some(install) = vmm_config.install.as_ref() else {
        if let Err(error) = stop_existing_managed_memory_mesh_runtime(state) {
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
        crate::managed_memory_mesh_runtime::clear_snapshot();
        return;
    };
    if !install.managed_install_enabled || install.auto_start_policy.as_deref() == Some("manual") {
        if let Err(error) = stop_existing_managed_memory_mesh_runtime(state) {
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
        crate::managed_memory_mesh_runtime::clear_snapshot();
        return;
    }
    let binary_path = match install
        .managed_binary_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(default_managed_vmm_binary_path)
    {
        Some(path) => path,
        None => {
            let error = String::from("无法解析托管 VMM 可执行文件路径");
            crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
    };
    // Validate the complete packaged runtime before replacing a healthy child.
    // 在替换健康子进程前校验完整的打包运行时。
    let binary_path =
        match crate::managed_memory_mesh_runtime::validate_managed_vmm_package(&binary_path) {
            Ok(path) => path,
            Err(error) => {
                crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
                logs.push(format!("[webui-service] managed VMM degraded: {error}"));
                return;
            }
        };
    let (public_ai_catalogs, optional_inference_routes) = match state.lock() {
        Ok(guard) => match guard.agent_runtime.as_ref() {
            Some(runtime) => {
                // Auto-start must not initialize VMM from stale or incomplete inference routes.
                // 自动启动不得使用陈旧或不完整的推理路由初始化 VMM。
                if let Err(error) =
                    crate::configured_inference::validate_vmm_required_inference_routes(
                        &runtime.public_ai_runtime,
                        ai_configuration,
                    )
                {
                    crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
                    logs.push(format!("[webui-service] managed VMM degraded: {error}"));
                    return;
                }
                let optional_inference_routes =
                    crate::configured_inference::inspect_vmm_optional_inference_routes(
                        &runtime.public_ai_runtime,
                        ai_configuration,
                    );
                match runtime.public_ai_runtime.catalog_snapshot() {
                    Ok(catalogs) => (catalogs, optional_inference_routes),
                    Err(error) => {
                        crate::managed_memory_mesh_runtime::publish_failed_snapshot(
                            "",
                            error.clone(),
                        );
                        logs.push(format!("[webui-service] managed VMM degraded: {error}"));
                        return;
                    }
                }
            }
            None => {
                let error = String::from("公共 AI Runtime 尚未发布");
                crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
                logs.push(format!("[webui-service] managed VMM degraded: {error}"));
                return;
            }
        },
        Err(error) => {
            let error = format!("读取公共 AI Runtime 失败：{error}");
            crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
    };
    let capabilities = match crate::managed_memory_mesh_runtime::configured_vmm_capability_preflight(
        ai_configuration,
        &public_ai_catalogs,
        &optional_inference_routes,
    ) {
        Ok(capabilities) => capabilities,
        Err(error) => {
            crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
    };
    let controller_endpoint = match state.lock() {
        Ok(guard) => match guard.controller_runtime.as_ref() {
            Some(controller) => controller.endpoint.clone(),
            None => {
                let error = String::from("共享 Controller 尚未发布");
                crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
                logs.push(format!("[webui-service] managed VMM degraded: {error}"));
                return;
            }
        },
        Err(error) => {
            let error = format!("读取服务状态失败：{error}");
            crate::managed_memory_mesh_runtime::publish_failed_snapshot("", error.clone());
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
    };
    // Exact prerequisite validation completes before replacing a healthy child, so an invalid
    // edited configuration cannot turn a working memory service into an avoidable outage.
    // 精确前置校验会在替换健康子进程前完成，避免无效编辑配置把可用记忆服务变成可避免的中断。
    if let Err(error) = stop_existing_managed_memory_mesh_runtime(state) {
        crate::managed_memory_mesh_runtime::publish_failed_snapshot(
            controller_endpoint.as_str(),
            error.clone(),
        );
        logs.push(format!("[webui-service] managed VMM degraded: {error}"));
        return;
    }
    let managed = match crate::managed_memory_mesh_runtime::ManagedMemoryMeshRuntime::start(
        &binary_path,
        controller_endpoint.as_str(),
        vmm_config,
        &capabilities,
    )
    .await
    {
        Ok(managed) => managed,
        Err(error) => {
            crate::managed_memory_mesh_runtime::publish_failed_snapshot(
                controller_endpoint.as_str(),
                error.clone(),
            );
            logs.push(format!("[webui-service] managed VMM degraded: {error}"));
            return;
        }
    };
    let snapshot = managed.snapshot();
    let managed = Arc::new(Mutex::new(managed));
    let update_result = publish_managed_memory_mesh_runtime(state, Arc::clone(&managed));
    match update_result {
        Ok(()) => {
            let Some(grpc_target) = snapshot.grpc_target.clone() else {
                logs.push(String::from(
                    "[webui-service] managed VMM degraded: 就绪状态缺少 gRPC 地址",
                ));
                return;
            };
            logs.push(format!(
                "[webui-service] managed VMM ready: target={} controller={} space={}",
                grpc_target, snapshot.controller_endpoint, snapshot.controller_space_id
            ));
            spawn_managed_memory_mesh_supervisor(
                Arc::clone(state),
                managed,
                ManagedMemoryMeshRestartSpec {
                    binary_path,
                    controller_endpoint,
                    vmm_config: vmm_config.clone(),
                    capabilities,
                },
            );
        }
        Err(error) => {
            crate::vmm_hooks::clear_managed_vmm_transport_overlay();
            if let Ok(mut managed) = managed.lock() {
                let _ = managed.stop();
            }
            crate::managed_memory_mesh_runtime::publish_failed_snapshot(
                controller_endpoint.as_str(),
                error.clone(),
            );
            logs.push(format!(
                "[webui-service] managed VMM degraded while publishing runtime: {error}"
            ));
        }
    }
}

/// Stops and detaches the currently owned managed VMM child, if one exists.
/// 停止并解除当前持有的托管 VMM 子进程（如果存在）。
///
/// # Parameters
/// 参数
/// `state` owns the exact managed runtime generation and process-local transport overlay.
/// `state` 持有精确托管运行时代次与进程内传输覆盖。
///
/// # Returns
/// 返回值
/// Success after the child is stopped and the overlay is cleared, or an explicit lock/stop error.
/// 子进程停止且传输覆盖清除后返回成功，否则返回明确的锁定或停止错误。
fn stop_existing_managed_memory_mesh_runtime(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let existing = state
        .lock()
        .map_err(|error| format!("读取既有托管 VMM 运行时失败：{error}"))?
        .managed_memory_mesh_runtime
        .take();
    if let Some(existing) = existing {
        existing
            .lock()
            .map_err(|error| format!("锁定既有托管 VMM 子进程失败：{error}"))?
            .stop()?;
    }
    crate::vmm_hooks::clear_managed_vmm_transport_overlay();
    Ok(())
}

/// Immutable restart inputs that remain valid for one host process generation.
/// 在一个宿主进程代次内保持有效的不可变重启输入。
#[derive(Clone)]
struct ManagedMemoryMeshRestartSpec {
    /// Exact managed sidecar executable.
    /// 精确的托管 Sidecar 可执行文件。
    binary_path: PathBuf,
    /// Shared Controller endpoint owned by Vulcan Code.
    /// 由 Vulcan Code 持有的共享 Controller 端点。
    controller_endpoint: String,
    /// Exact persisted VMM transport and behavior snapshot for crash restarts.
    /// 用于崩溃重启的精确持久化 VMM 传输与行为快照。
    vmm_config: vulcan_vmm::VmmConfigLayer,
    /// Exact configured inference capability snapshot.
    /// 精确配置的推理能力快照。
    capabilities: crate::managed_memory_mesh_runtime::ManagedVmmCapabilityPreflight,
}

/// Publishes one ready runtime, its in-memory credential overlay, and the refreshed hook snapshot atomically.
/// 原子发布一个就绪运行时、其内存凭据覆盖以及刷新后的 Hook 快照。
///
/// # Parameters
/// 参数
/// `state` is the service owner and `managed` is the exact authenticated child runtime.
/// `state` 是服务所有者，`managed` 是精确的已鉴权子运行时。
///
/// # Returns
/// 返回值
/// Success after every process-local consumer sees the same runtime generation.
/// 当所有进程内消费者都看到同一运行时代次后返回成功。
fn publish_managed_memory_mesh_runtime(
    state: &SharedServiceBridgeState,
    managed: Arc<Mutex<crate::managed_memory_mesh_runtime::ManagedMemoryMeshRuntime>>,
) -> Result<(), String> {
    let (snapshot, authorization) = {
        let managed = managed
            .lock()
            .map_err(|error| format!("锁定 VMM 托管运行时失败：{error}"))?;
        (managed.snapshot(), managed.authorization_value())
    };
    let grpc_target = snapshot
        .grpc_target
        .clone()
        .ok_or_else(|| String::from("就绪状态缺少 gRPC 地址"))?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?;
    let workspace = guard
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("工作区快照尚未发布"))?;
    let agent_runtime = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("Agent Runtime 尚未发布"))?;
    crate::vmm_hooks::install_managed_vmm_transport_overlay(
        grpc_target,
        authorization,
        snapshot.capabilities.precheck_ready,
        snapshot.capabilities.postaction_ready,
        snapshot.scope_ready,
    )?;
    let vmm_snapshot =
        build_vmm_hook_config_snapshot(&agent_runtime.provider_configuration, &workspace);
    if let Err(error) = agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(vmm_snapshot)
    {
        crate::vmm_hooks::clear_managed_vmm_transport_overlay();
        return Err(error);
    }
    guard.managed_memory_mesh_runtime = Some(managed);
    crate::managed_memory_mesh_runtime::publish_snapshot(snapshot);
    Ok(())
}

/// Supervises one managed child and restarts unexpected exits with bounded exponential backoff.
/// 监督一个托管子进程，并对意外退出执行有上限的指数退避重启。
///
/// # Parameters
/// 参数
/// `state` owns the current generation, `managed` is the watched child, and `spec` contains trusted restart inputs.
/// `state` 持有当前代次，`managed` 是被监督子进程，`spec` 包含可信重启输入。
fn spawn_managed_memory_mesh_supervisor(
    state: SharedServiceBridgeState,
    managed: Arc<Mutex<crate::managed_memory_mesh_runtime::ManagedMemoryMeshRuntime>>,
    spec: ManagedMemoryMeshRestartSpec,
) {
    tokio::spawn(async move {
        let mut watched = managed;
        let mut retry_delay = Duration::from_secs(5);
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !managed_runtime_is_current(&state, &watched) {
                return;
            }
            let crashed = match watched.lock() {
                Ok(mut runtime) => runtime.refresh_liveness(),
                Err(error) => {
                    crate::managed_memory_mesh_runtime::publish_failed_snapshot(
                        spec.controller_endpoint.as_str(),
                        format!("锁定 VMM 监督状态失败：{error}"),
                    );
                    true
                }
            };
            if !crashed {
                continue;
            }
            crate::vmm_hooks::clear_managed_vmm_transport_overlay();
            let _ = crate::managed_memory_mesh_runtime::synchronize_workbench_runtime(&state);
            tokio::time::sleep(retry_delay).await;
            if !managed_runtime_is_current(&state, &watched) {
                return;
            }
            let replacement =
                match crate::managed_memory_mesh_runtime::ManagedMemoryMeshRuntime::start(
                    &spec.binary_path,
                    spec.controller_endpoint.as_str(),
                    &spec.vmm_config,
                    &spec.capabilities,
                )
                .await
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let error = format!(
                            "VMM 自动重启失败；将在 {} 秒后重试：{error}",
                            retry_delay.as_secs()
                        );
                        if let Ok(mut watched) = watched.lock() {
                            watched.record_restart_failure(error);
                        }
                        let _ = crate::managed_memory_mesh_runtime::synchronize_workbench_runtime(
                            &state,
                        );
                        retry_delay = retry_delay.saturating_mul(2).min(Duration::from_secs(1800));
                        continue;
                    }
                };
            if !managed_runtime_is_current(&state, &watched) {
                let mut replacement = replacement;
                let _ = replacement.stop();
                return;
            }
            let replacement = Arc::new(Mutex::new(replacement));
            match publish_managed_memory_mesh_runtime(&state, Arc::clone(&replacement)) {
                Ok(()) => {
                    watched = replacement;
                    let _ =
                        crate::managed_memory_mesh_runtime::synchronize_workbench_runtime(&state);
                    retry_delay = Duration::from_secs(5);
                }
                Err(error) => {
                    if let Ok(mut replacement) = replacement.lock() {
                        let _ = replacement.stop();
                    }
                    let error = format!(
                        "发布自动重启的 VMM 运行时失败；将在 {} 秒后重试：{error}",
                        retry_delay.as_secs()
                    );
                    if let Ok(mut watched) = watched.lock() {
                        watched.record_restart_failure(error);
                    }
                    let _ =
                        crate::managed_memory_mesh_runtime::synchronize_workbench_runtime(&state);
                    retry_delay = retry_delay.saturating_mul(2).min(Duration::from_secs(1800));
                }
            }
        }
    });
}

/// Tests whether one supervised generation is still the service-owned current runtime.
/// 检查一个受监督代次是否仍是服务持有的当前运行时。
///
/// # Parameters
/// 参数
/// `state` is the service owner and `candidate` is the exact watched runtime allocation.
/// `state` 是服务所有者，`candidate` 是被监督运行时的精确分配。
///
/// # Returns
/// 返回值
/// `true` only when user actions have not replaced or disabled this generation.
/// 仅当用户操作尚未替换或关闭该代次时返回 `true`。
fn managed_runtime_is_current(
    state: &SharedServiceBridgeState,
    candidate: &Arc<Mutex<crate::managed_memory_mesh_runtime::ManagedMemoryMeshRuntime>>,
) -> bool {
    state
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .managed_memory_mesh_runtime
                .as_ref()
                .map(|current| Arc::ptr_eq(current, candidate))
        })
        .unwrap_or(false)
}

/// Resolves the packaged VMM sidecar beside the current Vulcan Code executable.
/// 解析当前 Vulcan Code 可执行文件旁的打包 VMM Sidecar。
///
/// # Returns
/// 返回值
/// A platform-specific executable candidate, or `None` when the host path is unavailable.
/// 返回平台专用可执行文件候选；宿主路径不可用时返回 `None`。
pub(crate) fn default_managed_vmm_binary_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    #[cfg(windows)]
    let file_name = "vmm-local.exe";
    #[cfg(not(windows))]
    let file_name = "vmm-local";
    Some(directory.join(file_name))
}

/// Refreshes every configured public AI catalog into process memory during startup.
/// 在启动期间把每个已配置公共 AI 目录刷新到进程内存。
///
/// # Parameters
/// 参数
/// `runtime` owns process-local catalogs, `configuration` owns connection metadata, and `logs`
/// receives controlled per-connection outcomes.
/// `runtime` 持有进程内目录，`configuration` 持有连接元数据，`logs` 接收受控的逐连接结果。
///
/// # Returns
/// 返回值
/// Success after all connections were attempted, or a startup-runtime construction error.
/// 尝试全部连接后返回成功，或返回启动运行时构建错误。
fn refresh_public_ai_catalogs_at_startup(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    logs: &mut Vec<String>,
) -> Result<(), ServiceBootstrapError> {
    if configuration.connection_graph.connections.is_empty() {
        return Ok(());
    }
    let async_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("公共 AI 启动同步运行时创建失败：{error}"),
            )
        })?;
    // Startup synchronization runs independent credential connections concurrently so one
    // unavailable provider consumes at most one metadata timeout window.
    // 启动同步并发处理相互独立的凭据连接，使单个不可用供应商最多占用一个元数据超时窗口。
    let connection_ids = configuration
        .connection_graph
        .connections
        .values()
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    let results = async_runtime.block_on(async {
        futures_util::future::join_all(connection_ids.into_iter().map(|connection_id| async move {
            let result = runtime
                .refresh_connection_catalog(configuration, connection_id.as_str())
                .await;
            (connection_id, result)
        }))
        .await
    });
    for (connection_id, result) in results {
        match result {
            Ok(catalog) => logs.push(format!(
                "[webui-service] public AI catalog synchronized: connection={} models={}",
                connection_id,
                catalog.models.len()
            )),
            Err(error) => logs.push(format!(
                "[webui-service] public AI catalog synchronization failed: connection={} error={}",
                connection_id, error
            )),
        }
    }
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
