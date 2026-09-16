//! WebUI service bootstrap orchestration and startup snapshot projection.
//! WebUI 服务引导编排与启动快照投影。

use std::{
    collections::BTreeMap,
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
        vmm_hooks::{VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks},
        vulcan_plugin_hooks::synchronize_vulcan_plugin_hooks,
    },
    luaskills_runtime::LuaSkillsRuntimeHandle,
    mcp::{bootstrap_mcp_runtime_access, plugin_mcp_server_configs, runtime_mcp_server_configs},
    runtime_assets,
    tooling::CommandSessionRuntime,
};
use vulcan_ai_contract::HostAiPort;
use vulcan_provider_config::config::ProviderConfigurationFile;
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, StartupSnapshot, StartupStepSnapshot, StartupStepStatus,
    WorkbenchSnapshot,
};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, WorkbenchRuntimeProjection,
    build_vmm_hook_config_snapshot, emit_service_event_with_trace, fallback_workbench_snapshot,
    install_webui_question_runtime_event_bridge, mark_webui_service_discovery_failed,
    mark_webui_service_discovery_ready, native_tool_summaries_from_agent_runtime,
    normalized_workspace_path_text_from_path, pending_question_snapshots_from_agent_runtime,
    subagent_navigation_snapshot_from_cached_state, workbench_from_runtime, workspace_root,
};

mod error;
mod startup_snapshot;
mod startup_updates;

#[cfg(test)]
#[path = "service_bootstrap/vce_startup_tests.rs"]
mod vce_startup_tests;

#[cfg(test)]
mod host_database_provider_lifecycle_tests {
    use super::*;
    use std::collections::HashMap;
    use vulcan_agent_runtime::luaskills_runtime::host_database_providers;
    use vulcan_workspace_store::local_database_runtime::LocalDatabaseRuntime;

    /// Verifies failed bootstrap rolls back providers and committed service state releases them on drop.
    /// 验证引导失败会回滚 provider，且已提交的服务状态会在销毁时释放 provider。
    /// No inputs or return value; all resources are private to this isolated test process.
    /// 无输入或返回值；所有资源均属于该隔离测试进程。
    #[test]
    #[ignore = "mutates the process-wide LuaSkills provider callback slots"]
    fn pending_and_committed_provider_installations_have_exact_owners() {
        host_database_providers::shutdown();
        // tokio_runtime owns both short-lived provider execution channels in this test.
        // tokio_runtime 持有本测试中两个短生命周期 provider 执行通道。
        let tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build isolated provider lifecycle runtime");

        // pending_runtime proves rollback does not require a published service state.
        // pending_runtime 证明回滚不依赖已发布的服务状态。
        let pending_runtime = Arc::new(
            LocalDatabaseRuntime::open(HashMap::new())
                .expect("open pending provider database runtime"),
        );
        // pending remains armed and therefore must remove the process-wide registration on drop.
        // pending 保持启用，因此销毁时必须移除进程级注册。
        let pending = PendingHostDatabaseProviderInstallation::new(
            host_database_providers::install(pending_runtime, tokio_runtime.handle().clone())
                .expect("install pending host database providers"),
        );
        assert!(host_database_providers::is_installed());
        drop(pending);
        assert!(!host_database_providers::is_installed());

        // committed_runtime proves ownership transfers to the long-lived service state.
        // committed_runtime 证明所有权会转移给长生命周期服务状态。
        let committed_runtime = Arc::new(
            LocalDatabaseRuntime::open(HashMap::new())
                .expect("open committed provider database runtime"),
        );
        // committed is disarmed only when its exact installation is stored in service state.
        // committed 仅在其精确安装写入服务状态时解除回滚。
        let committed = PendingHostDatabaseProviderInstallation::new(
            host_database_providers::install(committed_runtime, tokio_runtime.handle().clone())
                .expect("install committed host database providers"),
        );
        // state is the sole long-lived owner outside the process-wide installation slot.
        // state 是进程级安装槽位之外唯一的长生命周期所有者。
        let state = crate::create_service_bridge_state(
            crate::tests::test_support::test_service_shell_host(),
        );
        state
            .lock()
            .expect("lock isolated service state")
            .host_database_providers = Some(committed.commit());
        assert!(host_database_providers::is_installed());
        drop(state);
        assert!(!host_database_providers::is_installed());
    }
}

pub(super) use error::*;
pub(super) use startup_snapshot::*;
pub(super) use startup_updates::*;

/// Rollback guard for a host database provider installation that bootstrap has not published yet.
/// 尚未由引导流程正式发布的宿主数据库 provider 安装回滚守卫。
struct PendingHostDatabaseProviderInstallation {
    /// Installation retained until startup either commits it to service state or fails.
    /// 在启动流程提交到服务状态或失败之前保留的安装。
    installation: Option<
        Arc<
            vulcan_agent_runtime::luaskills_runtime::host_database_providers::InstalledHostDatabaseProviders,
        >,
    >,
}

impl PendingHostDatabaseProviderInstallation {
    /// Creates a rollback guard for one newly installed provider pair.
    /// 为一对新安装的 provider 创建回滚守卫。
    ///
    /// # Parameters / 参数
    /// `installation` is the exact process-wide provider installation created during bootstrap.
    /// `installation` 是引导期间创建的精确进程级 provider 安装。
    ///
    /// # Returns / 返回值
    /// Returns an armed guard that removes the process-wide registration on drop.
    /// 返回一个已启用的守卫，其销毁时会移除进程级注册。
    fn new(
        installation: Arc<
            vulcan_agent_runtime::luaskills_runtime::host_database_providers::InstalledHostDatabaseProviders,
        >,
    ) -> Self {
        Self {
            installation: Some(installation),
        }
    }

    /// Transfers the installation into the long-lived service state.
    /// 将安装转移给长生命周期服务状态。
    ///
    /// # Parameters / 参数
    /// This method consumes the pending guard.
    /// 本方法会消费待提交守卫。
    ///
    /// # Returns / 返回值
    /// Returns the exact installation that the guard owned.
    /// 返回守卫持有的精确安装。
    fn commit(mut self) -> Arc<
        vulcan_agent_runtime::luaskills_runtime::host_database_providers::InstalledHostDatabaseProviders,
    >{
        self.installation
            .take()
            .expect("pending host database provider installation must exist before commit")
    }
}

impl Drop for PendingHostDatabaseProviderInstallation {
    /// Rolls back the process-wide registration when bootstrap exits before publication.
    /// 当引导流程在发布前退出时回滚进程级注册。
    fn drop(&mut self) {
        if self.installation.is_some() {
            vulcan_agent_runtime::luaskills_runtime::host_database_providers::shutdown();
        }
    }
}

#[cfg(test)]
mod luaskills_callback_lifecycle_tests {
    use super::*;
    use luaskills::SkillLifecycleAction;
    use vulcan_ai_contract::test_support::MockHostAiPort;

    /// Verifies a failed startup removes both process-wide LuaSkills callback domains.
    /// 验证失败启动会移除两个进程级 LuaSkills 回调域。
    ///
    /// No inputs or return value; all process-wide slots are isolated by serial execution.
    /// 不含输入或返回值；全部进程级槽通过串行执行隔离。
    #[test]
    #[ignore = "mutates process-wide LuaSkills model and progress callback slots"]
    fn pending_callbacks_roll_back_before_service_publication() {
        crate::luaskills_model::shutdown_global();
        crate::luaskills_install_progress::shutdown_global();
        // runtime remains alive until callback rollback finishes.
        // runtime 保持存活直到回调回滚结束。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build callback lifecycle runtime");
        let port: Arc<dyn HostAiPort> = Arc::new(MockHostAiPort::new());
        let _owner = crate::luaskills_model::install_global(port, runtime.handle().clone());
        crate::luaskills_install_progress::install_global();
        let pending = PendingLuaSkillsCallbackInstallation::new();

        assert!(crate::luaskills_model::sync_global_availability().installed);
        assert!(
            crate::luaskills_install_progress::begin_global_operation(
                SkillLifecycleAction::Install
            )
            .is_some()
        );

        drop(pending);
        assert!(!crate::luaskills_model::sync_global_availability().installed);
        assert!(
            crate::luaskills_install_progress::begin_global_operation(
                SkillLifecycleAction::Install
            )
            .is_none()
        );
    }
}

/// Rolls back process-wide LuaSkills callbacks until service publication succeeds.
/// 在服务发布成功前负责回滚进程级 LuaSkills 回调。
struct PendingLuaSkillsCallbackInstallation {
    /// Whether bootstrap still owns rollback responsibility.
    /// 引导流程是否仍持有回滚责任。
    armed: bool,
}

impl PendingLuaSkillsCallbackInstallation {
    /// Creates an armed rollback guard after both callback owners are installed.
    /// 在两个回调所有者都安装后创建已启用的回滚守卫。
    ///
    /// # Returns / 返回值
    /// Returns a guard that clears model and progress callbacks unless committed.
    /// 返回一个守卫；未提交时会清除模型与进度回调。
    fn new() -> Self {
        Self { armed: true }
    }

    /// Transfers callback lifetime to the published service process.
    /// 将回调生命周期转移给已发布的服务进程。
    ///
    /// # Parameters / 参数
    /// This method consumes the exact pending guard and has no other inputs.
    /// 本方法消费精确的待提交守卫，不含其他输入。
    ///
    /// # Returns / 返回值
    /// No value is returned after rollback responsibility is disabled.
    /// 解除回滚责任后不返回值。
    fn commit(mut self) {
        self.armed = false;
    }
}

impl Drop for PendingLuaSkillsCallbackInstallation {
    /// Clears both callback domains when startup exits before final publication.
    /// 当启动在最终发布前退出时清理两个回调域。
    fn drop(&mut self) {
        if self.armed {
            crate::luaskills_model::shutdown_global();
            crate::luaskills_install_progress::shutdown_global();
        }
    }
}

/// Starts one bounded startup-stage timer and emits its explicit entry boundary.
/// 启动一个有界的启动阶段计时器，并输出其显式进入边界。
///
/// # Parameters
/// 参数
/// `stage` is the stable startup stage identifier.
/// `stage` 是稳定的启动阶段标识。
///
/// # Returns
/// 返回值
/// The monotonic origin used by the matching completion record.
/// 返回供对应完成记录使用的单调计时起点。
fn start_bootstrap_stage(stage: &str) -> Instant {
    eprintln!("[webui-service][startup-timing] stage={stage} status=started");
    Instant::now()
}

/// Emits one bounded startup-stage timing record without performing any additional I/O.
/// 在不执行任何额外 I/O 的前提下输出一条有界启动阶段耗时记录。
///
/// # Parameters
/// 参数
/// `stage` is the stable startup stage identifier.
/// `stage` 是稳定的启动阶段标识。
/// `started_at` is the monotonic origin captured around existing work.
/// `started_at` 是包围既有工作的单调计时起点。
/// `detail` contains already-available counts or status labels only.
/// `detail` 仅包含已经存在的数量或状态标签。
fn trace_bootstrap_stage(stage: &str, started_at: Instant, detail: impl AsRef<str>) {
    let elapsed_ms = started_at.elapsed().as_millis();
    eprintln!(
        "[webui-service][startup-timing] stage={stage} status=completed elapsed_ms={elapsed_ms} {}",
        detail.as_ref()
    );
}

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

/// Build a user-facing LuaSkills startup warning from one exact bootstrap error.
/// 根据一次确定的 LuaSkills 引导错误生成用户可见的启动警告。
///
/// `error` is the bootstrap failure; the result includes manual recovery only for settings errors.
/// `error` 是引导失败原因；返回值仅在设置错误时包含手动恢复指引。
fn luaskills_startup_warning_message(error: &str) -> String {
    if [
        "failed to read LuaSkills runtime settings `",
        "failed to parse LuaSkills runtime settings `",
        "invalid LuaSkills runtime settings `",
    ]
    .iter()
    .any(|prefix| error.starts_with(prefix))
    {
        return format!(
            "LuaSkills 配置无法加载：{error}。请备份并手动修复该配置文件，然后重启应用；原文件不会被自动覆盖。"
        );
    }
    String::from("LuaSkills 不可用，插件内容组件继续运行；请查看启动日志了解原因。")
}

/// Runs the real startup sequence owned by the WebUI service.
/// 运行由 WebUI 服务持有的真实启动顺序。
fn run_service_bootstrap(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
) -> Result<(), ServiceBootstrapError> {
    // bootstrap_started_at measures the complete synchronous service bootstrap lifecycle.
    // bootstrap_started_at 测量完整的同步服务引导生命周期。
    let bootstrap_started_at = start_bootstrap_stage("total");
    // configuration_started_at measures only the three exact startup configuration files.
    // configuration_started_at 仅测量三个精确启动配置文件。
    let configuration_started_at = start_bootstrap_stage("configuration");
    let mut logs = vec![format!(
        "[webui-service] workspace root: {}",
        normalized_workspace_path_text_from_path(&workspace_root())
    )];
    let provider_configuration = vulcan_provider_config::config::load_configuration_file()
        .map_err(|error| ServiceBootstrapError::new("service-process", "Core Service", error))?;
    let mut public_ai_configuration =
        vulcan_provider_config::config::load_public_ai_connections_file()
            .map_err(|error| ServiceBootstrapError::new("service-process", "Core Service", error))?
            .configuration;
    trace_bootstrap_stage("configuration", configuration_started_at, "files=3");
    // configuration_reconciliation_started_at covers migration, startup projection, and Tokio runtime creation.
    // configuration_reconciliation_started_at 覆盖配置迁移、启动投影与 Tokio Runtime 创建。
    let configuration_reconciliation_started_at =
        start_bootstrap_stage("configuration_reconciliation");
    // legacy_custom_provider_ids are embedded providers not yet represented by standalone
    // references. They migrate through the same recoverable journal as ordinary saves, so a
    // crash cannot split provider bytes from the central reference document.
    // legacy_custom_provider_ids 是尚未由独立引用表示的内嵌供应商。它们通过与普通保存
    // 相同的可恢复日志迁移，进程崩溃不会拆分供应商字节与中央引用文档。
    let legacy_custom_provider_ids = public_ai_configuration
        .custom_providers
        .keys()
        .filter(|provider_id| {
            !public_ai_configuration
                .custom_provider_file_refs
                .contains(*provider_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !legacy_custom_provider_ids.is_empty() {
        let app_data_root = vulcan_workspace_store::data_plane::application_data_root();
        vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
            &public_ai_configuration,
            &app_data_root,
        )
        .map_err(|error| {
            ServiceBootstrapError::new(
                "service-process",
                "Core Service",
                format!("自定义供应商独立清单事务迁移失败：{error}"),
            )
        })?;
        public_ai_configuration.custom_provider_file_refs = public_ai_configuration
            .custom_providers
            .keys()
            .cloned()
            .collect();
        logs.push(format!(
            "已事务迁移 {} 个自定义供应商到独立清单：{}",
            legacy_custom_provider_ids.len(),
            legacy_custom_provider_ids.join(", ")
        ));
    }
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
                ("mcp", StartupStepStatus::Pending, "等待 LuaSkills 就绪"),
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
            // A stable runtime-specific name makes fatal worker diagnostics attributable.
            // 稳定的运行时专属名称使致命工作线程诊断可以精确归因。
            .thread_name("vulcan-service-runtime")
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
    trace_bootstrap_stage(
        "configuration_reconciliation",
        configuration_reconciliation_started_at,
        format!("migrated_providers={}", legacy_custom_provider_ids.len()),
    );

    // data_plane_started_at measures controller startup without any workspace body hydration.
    // data_plane_started_at 测量不包含工作区正文水合的 Controller 启动耗时。
    let data_plane_started_at = start_bootstrap_stage("controller_data_plane");
    let controller_report = bootstrap_step(
        runtime.block_on(vulcan_workspace_store::data_plane::bootstrap_application_data_plane()),
        "vldb",
        "VLDB",
    )?;
    trace_bootstrap_stage(
        "controller_data_plane",
        data_plane_started_at,
        "result=ready",
    );
    logs.push(format!(
        "[webui-service] vldb ready: {}",
        controller_report.handle.sqlite_path.display()
    ));
    let controller_runtime = controller_report.handle.clone();
    let knowledge_runtime = vulcan_workspace_store::knowledge_base::KnowledgeBaseRuntime::new(
        controller_runtime.clone(),
    );
    // knowledge_recovery_started_at measures only exact persisted interrupted-job recovery.
    // knowledge_recovery_started_at 仅测量精确的持久化中断任务恢复。
    let knowledge_recovery_started_at = start_bootstrap_stage("knowledge_job_recovery");
    bootstrap_step(
        runtime.block_on(knowledge_runtime.recover_interrupted_jobs()),
        "vldb",
        "VLDB",
    )?;
    trace_bootstrap_stage(
        "knowledge_job_recovery",
        knowledge_recovery_started_at,
        "result=ready",
    );

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
                    ("mcp", StartupStepStatus::Pending, "等待 MCP Runtime"),
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
    // luaskills_started_at covers engine bootstrap plus the first exact catalog load.
    // luaskills_started_at 覆盖引擎引导与首次精确目录加载。
    // Publish the host database providers before any LuaSkills engine exists. The ordering matters because the
    // engine refuses to start in `HostCallback` mode with no callback registered. The pending owner rolls the
    // process-wide registration back if any later bootstrap stage fails.
    // 在任何 LuaSkills 引擎存在之前发布宿主数据库 provider。该顺序有两个原因：引擎在缺少回调时拒绝以
    // `HostCallback` 模式启动。待提交所有者会在任一后续引导阶段失败时回滚进程级注册。
    let pending_host_database_providers = PendingHostDatabaseProviderInstallation::new(
        vulcan_agent_runtime::luaskills_runtime::host_database_providers::install(
            std::sync::Arc::clone(&controller_runtime.local),
            runtime.handle().clone(),
        )
        .map_err(|error| {
            ServiceBootstrapError::new(
                "luaskills-database",
                "LuaSkills Host Database",
                format!("Failed to install the LuaSkills host database providers: {error}"),
            )
        })?,
    );

    let luaskills_started_at = start_bootstrap_stage("luaskills");
    let (luaskills_runtime, luaskills_catalog, luaskills_step_status, luaskills_step_message) =
        match vulcan_service_runtime::luaskills_bridge::bootstrap_luaskills(
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
                    luaskills_startup_warning_message(&error),
                )
            }
        };
    trace_bootstrap_stage(
        "luaskills",
        luaskills_started_at,
        format!(
            "result={} skills={} entries={}",
            if luaskills_runtime.is_some() {
                "available"
            } else {
                "degraded"
            },
            luaskills_catalog.skills.len(),
            luaskills_catalog.entry_count
        ),
    );
    // plugin_runtime_started_at measures catalog preparation without the asynchronous prewarm join.
    // plugin_runtime_started_at 测量不包含异步预热等待的插件目录准备。
    let plugin_runtime_started_at = start_bootstrap_stage("plugin_catalog_prepare");
    let (vulcan_plugin_runtime, vulcan_plugin_catalog) = prepare_vulcan_plugin_runtime(
        luaskills_runtime.as_ref(),
        &provider_configuration,
        &controller_runtime,
        runtime.handle().clone(),
        event_emitter.clone(),
    )?;
    trace_bootstrap_stage(
        "plugin_catalog_prepare",
        plugin_runtime_started_at,
        format!("plugins={}", vulcan_plugin_catalog.plugins.len()),
    );
    // pluginPrewarmTask overlaps the cold System VM creation cost with independent MCP initialization.
    // pluginPrewarmTask 将冷启动 System VM 的创建成本与独立的 MCP 初始化并行重叠。
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

    // mcp_started_at measures the complete MCP runtime bootstrap call.
    // mcp_started_at 测量完整的 MCP Runtime 引导调用。
    let mcp_started_at = start_bootstrap_stage("mcp");
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
    trace_bootstrap_stage(
        "mcp",
        mcp_started_at,
        format!(
            "configured={} connected={} tools={}",
            mcp_catalog.configured_server_count,
            mcp_catalog.connected_server_count,
            mcp_catalog.tool_count
        ),
    );
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
    // plugin_prewarm_join_started_at measures only the remaining wait after MCP overlap completes.
    // plugin_prewarm_join_started_at 仅测量 MCP 重叠阶段结束后的剩余等待。
    let plugin_prewarm_join_started_at = start_bootstrap_stage("plugin_prewarm_join");
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
    trace_bootstrap_stage(
        "plugin_prewarm_join",
        plugin_prewarm_join_started_at,
        format!("prewarmed={}", prewarmed_plugin_ids.len()),
    );
    logs.push(format!(
        "[webui-service] Vulcan plugins ready: plugins={} tools={} prewarmed_runtimes={} revision={} elapsed_ms={}",
        vulcan_plugin_catalog.plugins.len(),
        vulcan_plugin_catalog.enabled_tools.len(),
        prewarmed_plugin_ids.len(),
        vulcan_plugin_catalog.revision,
        plugin_prewarm_elapsed.as_millis()
    ));
    // static_catalogs_started_at measures already-configured profile, prompt-skill, and subagent roots.
    // static_catalogs_started_at 测量已配置的画像、提示技能与智能体根目录。
    let static_catalogs_started_at = start_bootstrap_stage("static_catalogs");
    let agent_profile_catalog = load_agent_profile_catalog()
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    // promptSkillConfig and promptSkillCatalog are captured once before the service accepts
    // submissions; the watcher publishes later generations only after an explicit filesystem change.
    // promptSkillConfig 与 promptSkillCatalog 在服务接受提交前仅捕获一次；监视器只在明确文件变化后发布后续代次。
    let prompt_skill_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &provider_configuration.prompt_skills,
    );
    let prompt_skill_catalog = Arc::new(vulcan_prompt_skills::load_prompt_skills(
        &prompt_skill_config,
    ));
    // subagentCatalog is loaded exactly once during startup and then retained in memory for every request and workbench projection.
    // subagentCatalog 在启动期间精确加载一次，随后由内存保留并供所有请求与工作台投影复用。
    let mut subagent_catalog = vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        vulcan_workspace_store::data_plane::application_data_root().join("agents"),
    );
    if let Err(error) = subagent_catalog
        .merge_plugin_definitions(vulcan_plugin_catalog.enabled_agents.values().cloned())
    {
        subagent_catalog
            .errors
            .push(vulcan_task_session::SubagentCatalogLoadError {
                path: vulcan_plugin_catalog.root.clone(),
                message: format!("合并插件智能体目录失败：{error}"),
            });
    }
    for error in &subagent_catalog.errors {
        logs.push(format!(
            "[webui-service] subagent catalog warning: path={} error={}",
            error.path.display(),
            error.message
        ));
    }
    trace_bootstrap_stage(
        "static_catalogs",
        static_catalogs_started_at,
        format!(
            "agent_profiles={} prompt_skills={} subagents={} errors={}",
            agent_profile_catalog.agents.len(),
            prompt_skill_catalog.skills.len(),
            subagent_catalog.definitions.len(),
            subagent_catalog.errors.len()
        ),
    );
    // agent_runtime_assembly_started_at covers provider catalogs, Agent MCP, hooks, and runtime bridge construction.
    // agent_runtime_assembly_started_at 覆盖 Provider 目录、Agent MCP、Hook 与运行时桥构造。
    let agent_runtime_assembly_started_at = start_bootstrap_stage("agent_runtime_assembly");
    // agent_luaskills_runtime is absent in degraded mode while the independent plugin runtime remains mounted.
    // agent_luaskills_runtime 在降级模式下为空，而独立插件运行时仍保持挂载。
    let agent_luaskills_runtime = luaskills_runtime.as_ref().and_then(|runtime_handle| {
        let engine = match runtime_handle.engine.read() {
            Ok(engine) => engine,
            Err(_) => {
                logs.push(String::from(
                    "[webui-service] LuaSkills engine read lock is poisoned; provider Lua tools are disabled",
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
            runtime_handle
                .effective_runtime_settings
                .vm_pool
                .max_size,
            runtime_handle
                .effective_runtime_settings
                .run_lua_pool
                .max_size,
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
    // credentialStore binds every new credential mutation to the Controller-owned application SQLite database.
    // credentialStore 将全部新凭据变更绑定到 Controller 持有的应用 SQLite 数据库。
    let credential_store = Arc::new(
        crate::ai_credential_secret_store::SqliteAiCredentialSecretStore::new(
            controller_runtime.clone(),
        ),
    );
    let public_ai_runtime = Arc::new(
        vulcan_provider_adapters::PublicAiRuntime::first_batch(credential_store).map_err(
            |error| {
                ServiceBootstrapError::new(
                    "workspace",
                    "Workspace",
                    format!("公共 AI 运行时初始化失败：{error}"),
                )
            },
        )?,
    );
    public_ai_runtime
        .replace_configuration(public_ai_configuration.clone())
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    // Post-dispatch media tools reuse the existing ordered WebUI upload progress surface.
    // 分派后媒体工具复用现有有序 WebUI 上传进度界面。
    crate::media_upload::install_runtime_media_upload_progress_bridge(
        public_ai_runtime.as_ref(),
        event_emitter.clone(),
        state.clone(),
    )
    .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    // The startup recovery chain restores any persisted system manifest layers (active,
    // quarantined-invalid, then previous) into the single authoritative catalog before the
    // first catalog refresh so operators never observe the bundled baseline over a
    // successfully activated layer (plan §17.5).
    // 启动恢复链在首次目录刷新前，把任何持久化系统清单层（active、隔离无效后 previous）
    // 恢复到唯一权威目录，使操作者绝不在已成功激活层之上观察到内置基线（方案 §17.5）。
    let manifest_restores = crate::provider_manifest_sync::restore_system_manifests_from_store(
        public_ai_runtime.as_ref(),
        &vulcan_workspace_store::data_plane::application_data_root(),
    )
    .map_err(|error| {
        ServiceBootstrapError::new(
            "workspace",
            "Workspace",
            format!("系统清单恢复链执行失败：{error}"),
        )
    })?;
    if !manifest_restores.is_empty() {
        logs.push(format!(
            "已从持久化存储恢复 {} 个系统供应商清单：{}",
            manifest_restores.len(),
            manifest_restores
                .iter()
                .map(|report| format!("{}({})", report.provider_id, report.layer))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    // The provider-manifest sync service is injected once into the shared state so RPCs and
    // periodic triggers reuse the same instance instead of constructing empty-key/None-URL
    // services per call (plan §R7.13).
    // 供应商清单同步服务在共享状态中注入一次，使 RPC 与周期触发复用同一实例，而不是
    // 每次调用构造空密钥/None URL 的服务（方案 §R7.13）。
    {
        let sync_http =
            crate::provider_manifest_sync::build_sync_http_client().map_err(|error| {
                ServiceBootstrapError::new(
                    "workspace",
                    "Workspace",
                    format!("供应商清单安全同步客户端初始化失败：{error}"),
                )
            })?;
        let sync_service = crate::provider_manifest_sync::ProviderManifestSyncService::new(
            &vulcan_workspace_store::data_plane::application_data_root(),
            sync_http,
            Vec::new(),
            None,
        );
        if let Ok(mut guard) = state.lock() {
            guard.provider_manifest_sync = Some(sync_service);
        }
    }
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
    crate::provider_manifest_sync::emit_published_startup_manifest_restore_events(
        public_ai_runtime.as_ref(),
        &manifest_restores,
    )
    .map_err(|error| {
        ServiceBootstrapError::new(
            "workspace",
            "Workspace",
            format!("系统清单恢复事件发布失败：{error}"),
        )
    })?;
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
    // usageRefreshController persists the same observations through the existing application binding.
    // usageRefreshController 通过现有应用绑定持久化同一批观测。
    let usage_refresh_controller = controller_runtime.clone();
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
            match usage_refresh_runtime.usage_cache_snapshot() {
                Ok(snapshots) => {
                    if let Err(error) =
                        crate::provider_usage_runtime::persist_native_account_usage_snapshots(
                            &usage_refresh_controller,
                            usage_refresh_runtime.as_ref(),
                            &configuration,
                            &snapshots,
                        )
                        .await
                    {
                        eprintln!("[webui-service] public AI usage persistence failed: {error}");
                    }
                }
                Err(error) => {
                    eprintln!("[webui-service] public AI usage cache snapshot failed: {error}");
                }
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
        .with_provider_request_trace_enabled(provider_configuration.provider_request_trace_enabled)
        .with_native_usage_storage(controller_runtime.clone())
        .with_plugin_telemetry(vulcan_plugin_runtime.clone()),
    );
    let internal_ai_port: Arc<dyn HostAiPort> =
        Arc::new(crate::internal_ai::ConfiguredInferenceHostAiPort::new(
            Arc::clone(&configured_inference_service),
        ));
    // LuaSkillsModelCallbacks are process-wide upstream slots; the service owns
    // them here, after the shared Host AI port exists and before the service is
    // published to sessions. An unconfigured `luaskills` profile only makes the
    // capabilities unavailable; it never fails startup.
    // LuaSkillsModelCallbacks 是上游进程级槽；服务在此处持有它们——共享 Host AI
    // Port 已存在、且服务尚未发布给会话。未配置 `luaskills` Profile 只会让能力不可用，
    // 绝不导致启动失败。
    let _luaskills_model_owner = crate::luaskills_model::install_global(
        Arc::clone(&internal_ai_port),
        runtime.handle().clone(),
    );
    // The upstream operation-progress callback is process-wide as well, so the
    // production service owns it here and a non-production bootstrap cannot
    // silently replace it with an unowned one.
    // 上游操作进度回调同样是进程级的，因此由生产服务在此持有，非生产引导无法用无主回调
    // 静默替换它。
    crate::luaskills_install_progress::install_global_with_projection(
        event_emitter.clone(),
        Arc::clone(state),
        runtime.handle().clone(),
    );
    // pending_luaskills_callbacks rolls both process-wide callback domains back if any
    // later startup stage fails before ready publication.
    // pending_luaskills_callbacks 在就绪发布前任一后续启动阶段失败时回滚两个进程级回调域。
    let pending_luaskills_callbacks = PendingLuaSkillsCallbackInstallation::new();
    // LuaSkills model availability must be visible at startup without blocking
    // it: an unconfigured or expired route only degrades the two Lua model
    // capabilities, and the settings page owns the repair entry.
    // LuaSkills 模型可用性必须在启动时可见但不阻断启动：未配置或失效的路由只会降级
    // 两个 Lua 模型能力，修复入口属于设置页。
    let luaskills_model_availability = crate::luaskills_model::luaskills_model_availability();
    if !luaskills_model_availability.llm || !luaskills_model_availability.embedding {
        logs.push(format!(
            "[webui-service] LuaSkills model capability unavailable (llm={}, embedding={}); \
             configure the `luaskills` inference routes to enable it",
            luaskills_model_availability.llm, luaskills_model_availability.embedding
        ));
    }
    // workspaceWatcher owns the single physical notify backend shared by Git and VCE subscribers.
    // workspaceWatcher 持有供 Git 与 VCE 订阅者共享的唯一物理 notify 后端。
    let workspace_watcher = match crate::workspace_watcher::WorkspaceWatcherCenter::new() {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            eprintln!("[webui-service] shared workspace watcher unavailable: {error}");
            None
        }
    };
    // vceRuntime reuses the shared HostAI and controller services while isolating code-semantic data.
    // vceRuntime 复用共享 HostAI 与 Controller 服务，同时隔离代码语义数据。
    let vce_runtime = match runtime.block_on(async {
        vulcan_workspace_store::vce_data_plane::bootstrap_vce_data_plane(&controller_runtime).await
    }) {
        Ok(data_plane) => Some(crate::vce::VceRuntimeManager::new(
            Arc::clone(&internal_ai_port),
            data_plane,
            event_emitter.clone(),
            crate::vce::vce_publication_authority(state),
        )),
        Err(error) => {
            eprintln!("[webui-service] optional native VCE bootstrap unavailable: {error}");
            None
        }
    };
    // vceWatcher receives an independent subscriber while reusing the shared physical backend.
    // vceWatcher 接收独立订阅者，同时复用共享物理后端。
    let vce_watcher = match (workspace_watcher.as_ref(), vce_runtime.as_ref()) {
        (Some(watcher), Some(manager)) => {
            let _runtime_guard = runtime.enter();
            Some(crate::vce::VceWatcherService::new(
                watcher,
                manager.clone(),
                event_emitter.clone(),
                Arc::clone(&runtime),
            ))
        }
        _ => None,
    };
    let global_vmm_effective =
        vulcan_vmm::VmmEffectiveConfig::resolve(&provider_configuration.vmm, None, None);
    // nativeVmmBootstrap initializes one independent local backend without making ExternalGrpc
    // depend on local SQLite, LanceDB, Core recovery, or worker availability.
    // nativeVmmBootstrap 初始化独立本地后端，不让 ExternalGrpc 依赖本地 SQLite、LanceDB、Core 恢复或工作器可用性。
    let native_vmm_bootstrap = runtime.block_on(async {
        // nativeVmmDataPlane creates the host-owned SQLite/LanceDB split and protected fallback project.
        // nativeVmmDataPlane 创建宿主持有的 SQLite/LanceDB 分离存储及受保护兜底项目。
        let native_vmm_data_plane =
            vulcan_workspace_store::vmm_data_plane::bootstrap_vmm_data_plane(&controller_runtime)
                .await
                .map_err(|error| format!("原生 VMM 数据平面初始化失败：{error}"))?;
        let native_vmm_runtime = Arc::new(
            crate::native_vmm::NativeVmmCoreRuntime::bootstrap(
                Arc::clone(&internal_ai_port),
                native_vmm_data_plane.clone(),
            )
            .await
            .map_err(|error| format!("原生 VMM Core 初始化失败：{error}"))?,
        );
        // Install committed-content publication before any client or background worker can mutate Core.
        // 在任何客户端或后台工作器可以变更 Core 之前安装已提交内容发布。
        native_vmm_runtime.install_content_projection(state, event_emitter)?;
        // nativeVmmOperationGate separates concurrent functional reads/writes from exclusive project maintenance.
        // nativeVmmOperationGate 将并发功能读写与独占项目维护隔离。
        let native_vmm_operation_gate = Arc::new(tokio::sync::RwLock::new(()));
        let native_vmm_maintenance = crate::native_vmm::NativeVmmMaintenance::new(
            Arc::clone(&native_vmm_runtime),
            native_vmm_data_plane.clone(),
            Arc::clone(&native_vmm_operation_gate),
        )
        .with_writeback_runtime(vmm_hook_runtime.clone());
        if native_vmm_bootstrap_is_required(&global_vmm_effective)
            && let Err(error) = native_vmm_maintenance.validate_host_ai().await
        {
            eprintln!("[webui-service] native VMM Host AI validation failed: {error}");
        }
        let (native_backend_enabled, pending_analysis_notify) =
            native_vmm_maintenance.pending_analysis_activation();
        let native_vmm_client = Arc::new(crate::native_vmm::NativeVmmClient::new(
            Arc::clone(&native_vmm_runtime),
            native_vmm_data_plane,
            native_vmm_operation_gate,
            native_backend_enabled,
            pending_analysis_notify,
        ));
        native_vmm_client
            .prepare_pending_analysis_worker()
            .await
            .map_err(|error| format!("原生 VMM 待分析队列恢复失败：{error}"))?;
        Ok::<_, String>((native_vmm_maintenance, native_vmm_client))
    });
    let native_vmm_maintenance = match native_vmm_bootstrap.and_then(
        |(native_vmm_maintenance, native_vmm_client)| {
            let process_native_vmm_client: Arc<dyn vulcan_vmm::VmmClient> =
                native_vmm_client.clone();
            vmm_hook_runtime
                .install_native_client(Arc::clone(&process_native_vmm_client))
                .map_err(|error| format!("原生 VMM Hook 客户端安装失败：{error}"))?;
            vulcan_vmm::install_process_native_vmm_client(process_native_vmm_client)
                .map_err(|error| format!("原生 VMM 进程客户端安装失败：{error}"))?;
            // The durable worker starts only after every public native client registry accepts it.
            // 仅在全部公开原生客户端注册表接纳后启动耐久工作器。
            native_vmm_client.start_pending_analysis_worker(runtime.handle());
            native_vmm_maintenance.set_native_backend_enabled(native_vmm_bootstrap_is_required(
                &global_vmm_effective,
            ));
            native_vmm_maintenance.start_retention_worker(runtime.handle());
            Ok(native_vmm_maintenance)
        },
    ) {
        Ok(maintenance) => Some(maintenance),
        Err(error) if !native_vmm_bootstrap_is_required(&global_vmm_effective) => {
            eprintln!(
                "[webui-service] optional native VMM bootstrap unavailable while inactive: {error}"
            );
            None
        }
        Err(error) => {
            return Err(ServiceBootstrapError::new("workspace", "Workspace", error));
        }
    };
    let task_runtime = Arc::new(TaskRuntime::new());
    task_runtime
        .replace_subagent_catalog(subagent_catalog.clone())
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
    let agent_runtime = AgentRuntimeBridge {
        provider_configuration: provider_configuration.clone(),
        public_ai_runtime,
        configured_inference_service,
        internal_ai_port,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        agent_profile_catalog,
        subagent_catalog: Arc::new(subagent_catalog),
        luaskills_runtime: agent_luaskills_runtime,
        vulcan_plugin_runtime: vulcan_plugin_runtime.clone(),
        extension_generation: Arc::new(ExtensionGeneration::new(
            vulcan_plugin_catalog.revision,
            Some(Arc::clone(&vulcan_plugin_catalog)),
            agent_mcp_runtime,
            Some(prompt_skill_catalog),
        )),
        task_runtime,
        question_runtime: Arc::new(QuestionRuntime::new()),
        command_session_runtime: Arc::new(CommandSessionRuntime::new()),
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
        "[webui-service] agent runtime bridge ready: provider/LuaSkills/MCP handles are mounted",
    ));
    trace_bootstrap_stage(
        "agent_runtime_assembly",
        agent_runtime_assembly_started_at,
        "result=ready",
    );

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

    // workspace_index_started_at measures only the indexed project/session summary load.
    // workspace_index_started_at 仅测量已索引项目与会话摘要加载。
    let workspace_index_started_at = start_bootstrap_stage("workspace_index");
    let mut workspace_snapshot = bootstrap_step(
        runtime.block_on(
            vulcan_workspace_store::workspace_store::load_or_seed_workspace_index(
                controller_runtime.clone(),
            ),
        ),
        "workspace",
        "Workspace",
    )?;
    trace_bootstrap_stage(
        "workspace_index",
        workspace_index_started_at,
        format!(
            "projects={} sessions={} archived_sessions={}",
            workspace_snapshot.task_projects.len(),
            workspace_snapshot.sessions.len(),
            workspace_snapshot.archived_sessions.len()
        ),
    );
    let selected_session_id = workspace_snapshot.selected_session;
    // Restore only the selected session's request metadata before publishing the first usable model picker.
    // 发布首个可用模型选择器前，仅恢复选中会话的请求 metadata。
    let session_registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| {
                ServiceBootstrapError::new(
                    "workspace",
                    "Workspace",
                    format!("failed to lock service state: {error}"),
                )
            })?
            .session_runtime_registry,
    );
    // Keep lifecycle ownership through initial publication; this does not load a Turn or its context.
    // 贯穿首次发布保留生命周期所有权；这不会加载回合或其上下文。
    let selected_model_projection = if selected_session_id != crate::SessionId::nil() {
        Some(bootstrap_step(
            runtime.block_on(
                crate::session_model_projection_v1::read_session_model_projection_v1(
                    &controller_runtime,
                    session_registry.as_ref(),
                    selected_session_id,
                ),
            ),
            "workspace",
            "Workspace",
        )?)
    } else {
        None
    };
    if let Some(projection) = selected_model_projection.as_ref() {
        bootstrap_step(
            projection.apply(
                session_registry.as_ref(),
                &mut workspace_snapshot.session_runtime_state,
            ),
            "workspace",
            "Workspace",
        )?;
    }
    // Initial reasoning controls follow the same selected-session metadata as the model, without rewriting global defaults.
    // 初始思考控件与模型使用同一选中会话 metadata，不改写全局默认值。
    let mut selected_provider_configuration = provider_configuration.clone();
    crate::restore_webui_runtime_controls_for_session(
        &mut selected_provider_configuration,
        workspace_snapshot
            .session_runtime_state
            .get(&selected_session_id),
    );
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
    // initial_projection_started_at measures selected-session locator validation and first workbench projection.
    // initial_projection_started_at 测量选中会话定位校验与首次工作台投影。
    let initial_projection_started_at = start_bootstrap_stage("initial_workbench_projection");
    let emitted_workbench = {
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
        let subagent_navigation =
            subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
                .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        let mcp_server_states = agent_runtime
            .extension_generation
            .mcp_runtime()
            .server_states();
        // vmm_session_states contains only controllers already owned by active in-memory work.
        // vmm_session_states 仅包含活动内存工作已经持有的控制器。
        // Startup never opens the selected session merely because its index row is selected.
        // 启动不会仅因某个会话索引被选中而打开该会话。
        let vmm_session_states = guard
            .vmm_controller_registry
            .snapshots()
            .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
            controller: &controller_runtime,
            command_session_runtime: Some(agent_runtime.command_session_runtime.as_ref()),
            luaskills_catalog: &luaskills_catalog,
            vulcan_plugin_catalog: Some(vulcan_plugin_catalog.as_ref()),
            subagent_catalog: agent_runtime.subagent_catalog.as_ref(),
            mcp_server_states: &mcp_server_states,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &selected_provider_configuration,
            public_ai_runtime: Some(agent_runtime.public_ai_runtime.as_ref()),
            workspace: &workspace_snapshot,
            pending_prompt_process_mirrors: &guard.pending_prompt_process_mirrors,
            vmm_session_states: &vmm_session_states,
            native_vmm_host_ai_status: native_vmm_maintenance
                .as_ref()
                .map(crate::native_vmm::NativeVmmMaintenance::host_ai_status),
            subagent_navigation,
            pending_questions: pending_question_snapshots_from_agent_runtime(Some(&agent_runtime))
                .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?,
            logs: &logs,
            live_context_budgets: &BTreeMap::new(),
            session_goal_summaries: &BTreeMap::new(),
            session_provider_usage_summaries: &BTreeMap::new(),
            selected_context_usage_facts: None,
            native_tools: native_tool_summaries_from_agent_runtime(Some(&agent_runtime)),
        });
        crate::reconcile_workbench_conversation_lifecycle_projection(
            &mut workbench,
            &guard,
            &workspace_snapshot,
        )
        .map_err(|error| ServiceBootstrapError::new("workspace", "Workspace", error))?;
        crate::preserve_project_settings_revisions(&mut workbench, &guard.workbench);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(workspace_snapshot);
        guard.agent_runtime = Some(agent_runtime);
        guard.native_vmm_maintenance = native_vmm_maintenance;
        guard.vce_runtime = vce_runtime;
        guard.vce_watcher = vce_watcher;
        guard.workspace_watcher = workspace_watcher;
        guard.service_runtime = Some(runtime.clone());
        guard.controller_runtime = Some(controller_runtime);
        guard.knowledge_runtime = Some(knowledge_runtime);
        guard.luaskills_runtime = luaskills_runtime;
        guard.cached_luaskills_catalog = Some(luaskills_catalog);
        guard.mcp_runtime = Some(mcp_runtime);
        guard.cached_mcp_catalog = Some(mcp_catalog);
        workbench
    };
    // Initial configuration publication is complete; later navigation acquires its own scoped guard.
    // 首次配置发布已完成；后续导航获取自己的作用域守卫。
    drop(selected_model_projection);
    trace_bootstrap_stage(
        "initial_workbench_projection",
        initial_projection_started_at,
        format!("selected={selected_session_id}"),
    );
    // VCE startup retains only its already-created service boundary; project indexing is explicit.
    // VCE 启动只保留已创建的服务边界；项目索引必须由用户明确触发。
    let vce_started_at = start_bootstrap_stage("vce");
    logs.push(String::from(
        "语义引擎服务已准备；项目索引等待用户明确创建或同步。",
    ));
    trace_bootstrap_stage("vce", vce_started_at, "result=lazy_ready");
    // runtime_guard makes the retained service runtime current while the notify loop is spawned.
    // runtime_guard 在生成 notify 循环期间使保留的服务运行时成为当前运行时。
    let _runtime_guard = runtime.enter();
    match crate::provider_manifest_sync::start_provider_manifest_sync_monitor(
        state.clone(),
        event_emitter.clone(),
    )
    .map_err(|error| ServiceBootstrapError::new("workspace", "Provider Manifest", error))?
    {
        true => logs.push(String::from("供应商清单异步启动同步与周期同步任务已启动。")),
        false => logs.push(String::from(
            "供应商清单远端 URL 或受信密钥未配置；未启动自动同步任务。",
        )),
    }
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
        guard.host_database_providers = Some(pending_host_database_providers.commit());
        guard.startup = startup.clone();
    }
    pending_luaskills_callbacks.commit();
    mark_webui_service_discovery_ready(state);
    emit_service_snapshots(event_emitter, &startup, &emitted_workbench);
    crate::vmm::start_vmm_health_monitor(state.clone(), event_emitter.clone());
    trace_bootstrap_stage("total", bootstrap_started_at, "result=ready");
    Ok(())
}

/// Decides whether native VMM bootstrap failure must stop the complete service startup.
/// 判断原生 VMM 启动失败是否必须终止完整服务启动。
///
/// # Parameters
/// `effective` is the authoritative global VMM configuration resolved for this startup.
/// `effective` 是本次启动解析出的权威全局 VMM 配置。
///
/// # Returns
/// Returns `true` only when enabled memory actively selects the native backend.
/// 仅当记忆已启用且当前选择原生后端时返回 `true`。
fn native_vmm_bootstrap_is_required(effective: &vulcan_vmm::VmmEffectiveConfig) -> bool {
    effective.enabled && effective.backend_mode == vulcan_vmm::VmmBackendMode::Native
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
    // Startup recovery has already staged persisted manifests. Refresh every affected source
    // catalog against that exact staged generation and publish once; using ordinary refresh
    // here would combine bundled provider metadata with recovered manifest rules.
    // 启动恢复已经暂存持久化清单。针对该精确暂存代际刷新每个受影响源目录并只发布一次；
    // 若此处使用普通刷新，会把内置供应商元数据与已恢复清单规则混合。
    // runtimeConfiguration removes every provider identity without executable authority from a
    // clone; the authoritative configuration and source bytes remain untouched for repair.
    // runtimeConfiguration 从克隆中移除全部缺少可执行权威的供应商身份；权威配置与源字节保持
    // 不变，以供修复。
    let runtime_configuration = runtime
        .runtime_configuration_snapshot(configuration)
        .map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("公共 AI 启动配置投影失败：{error}"),
            )
        })?;
    let connection_ids = runtime_configuration
        .connection_graph
        .connections
        .values()
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    async_runtime
        .block_on(
            runtime.activate_staged_manifest_catalogs(&runtime_configuration, &connection_ids),
        )
        .map_err(|error| {
            ServiceBootstrapError::new(
                "workspace",
                "Workspace",
                format!("公共 AI 启动代际发布失败：{error}"),
            )
        })?;
    let catalogs = runtime.catalog_snapshot().map_err(|error| {
        ServiceBootstrapError::new(
            "workspace",
            "Workspace",
            format!("公共 AI 启动目录读取失败：{error}"),
        )
    })?;
    for connection_id in connection_ids {
        logs.push(format!(
            "[webui-service] public AI catalog synchronized: connection={} models={}",
            connection_id,
            catalogs
                .get(&connection_id)
                .map_or(0, |catalog| catalog.models.len())
        ));
    }
    if configuration.custom_provider_manifest_failure_total_count > 0 {
        logs.push(format!(
            "[webui-service] isolated {} invalid custom provider manifest(s); healthy providers remain active",
            configuration.custom_provider_manifest_failure_total_count
        ));
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
    controller_runtime: &HostRuntimeHandle,
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
    // pluginStateRoot is shared by the manager metadata and in-process data isolation.
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
    // storageService routes plugin database calls through the already-open shared host runtime.
    // storageService 通过已连接的共享 Controller 客户端路由插件数据库调用。
    let storage_service = Arc::new(crate::plugin_storage::PluginStorageService::new(
        controller_runtime,
        service_runtime.clone(),
        plugin_state_root,
    ));
    // uiEventPublisher forwards revision-only invalidations through existing service event sinks.
    // uiEventPublisher 通过现有服务事件接收器转发仅含修订号的失效通知。
    let ui_event_publisher = Arc::new(crate::plugin_ui_events::ServicePluginUiEventPublisher::new(
        event_emitter,
    ));
    // vldbService exposes only host-registered logical bindings and never physical database paths.
    // vldbService 仅暴露宿主注册的逻辑绑定，绝不暴露物理数据库路径。
    let vldb_service = Arc::new(crate::plugin_vldb::PluginVldbService::new(
        controller_runtime.clone(),
        service_runtime,
    ));
    vulcan_plugin_runtime
        .install_host_services(vulcan_plugin_runtime::VulcanPluginHostServices {
            storage: Some(storage_service),
            vldb: Some(vldb_service),
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

#[cfg(test)]
mod native_vmm_bootstrap_tests {
    use super::native_vmm_bootstrap_is_required;

    /// Verifies only an enabled native selection turns local bootstrap failure into a service failure.
    /// 验证只有已启用的原生选择才会把本地启动失败升级为服务失败。
    #[test]
    fn inactive_or_external_vmm_does_not_require_native_bootstrap() {
        let mut effective = vulcan_vmm::VmmEffectiveConfig::default();
        assert!(!native_vmm_bootstrap_is_required(&effective));

        effective.enabled = true;
        effective.backend_mode = vulcan_vmm::VmmBackendMode::ExternalGrpc;
        assert!(!native_vmm_bootstrap_is_required(&effective));

        effective.backend_mode = vulcan_vmm::VmmBackendMode::Native;
        assert!(native_vmm_bootstrap_is_required(&effective));
    }
}

#[cfg(test)]
mod luaskills_startup_warning_tests {
    use super::{luaskills_startup_warning_message, startup_steps_with_status};
    use crate::service_bootstrap::StartupStepStatus;
    use time::OffsetDateTime;

    /// Verify an invalid user file reaches the displayed startup step with exact recovery context.
    /// 验证无效用户文件的精确路径、原因和恢复指引进入界面显示的启动步骤。
    #[test]
    fn invalid_settings_warning_survives_startup_projection() {
        let error = "invalid LuaSkills runtime settings `C:\\Vulcan\\output\\config\\luaskills-runtime-setting.json`: unsupported LuaSkills runtime settings schema: 2";
        let message = luaskills_startup_warning_message(error);
        let steps = startup_steps_with_status(
            OffsetDateTime::now_utc(),
            &[("luaskills", StartupStepStatus::Warning, message.as_str())],
        );
        let displayed = steps[0].message.as_deref().expect("startup step message");
        assert!(displayed.contains("C:\\Vulcan\\output\\config\\luaskills-runtime-setting.json"));
        assert!(displayed.contains("schema: 2"));
        assert!(displayed.contains("手动修复"));
        assert!(displayed.contains("重启应用"));
    }

    /// Verify unrelated engine failures do not receive a misleading settings-file diagnosis.
    /// 验证无关引擎故障不会被错误投影成设置文件问题。
    #[test]
    fn unrelated_failure_keeps_generic_warning() {
        let message = luaskills_startup_warning_message("创建 LuaSkills 引擎失败：VM 初始化失败");
        assert!(!message.contains("配置文件"));
        assert!(message.contains("启动日志"));
    }
}
