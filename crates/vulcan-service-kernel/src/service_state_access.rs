//! service state access helpers.
//! service state access 辅助逻辑。

use super::*;

/// Serializes every global Provider configuration transaction across service request families.
/// 串行化跨服务请求族的全部全局 Provider 配置事务。
static PROVIDER_CONFIGURATION_TRANSACTION_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// Serializes standalone public AI connection-file transactions.
/// 串行化独立公共 AI 连接文件事务。
static PUBLIC_AI_CONFIGURATION_TRANSACTION_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// Retains weak per-file locks so unrelated project configurations can be saved concurrently.
/// 保留按文件划分的弱引用锁，使无关项目配置可以并发保存。
static PROJECT_CONFIGURATION_TRANSACTION_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Weak<tokio::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// ProviderConfigurationTransaction owns exclusive global configuration persistence rights.
/// ProviderConfigurationTransaction 持有全局配置持久化的独占权限。
pub(crate) struct ProviderConfigurationTransaction {
    /// Process-wide asynchronous transaction guard retained for this transaction lifetime.
    /// 当前事务生命周期内保留的进程级异步事务守卫。
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

impl ProviderConfigurationTransaction {
    /// Persists one configuration while this transaction owns the global write right.
    /// 在当前事务持有全局写权限时持久化一份配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the fully validated next global Provider configuration.
    /// `configuration` 是已经完整校验的下一份全局 Provider 配置。
    /// `error_context` identifies the user action when persistence fails.
    /// `error_context` 在持久化失败时标识对应的用户动作。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` after the provider-config crate successfully saves the configuration file.
    /// Provider 配置 crate 成功保存配置文件后返回 `Ok(())`。
    pub(crate) fn persist(
        &self,
        configuration: &ProviderConfigurationFile,
        error_context: &str,
    ) -> Result<(), String> {
        vulcan_provider_config::config::save_configuration_file(configuration).map_err(|error| {
            if error_context.is_empty() {
                error
            } else {
                format!("{error_context}: {error}")
            }
        })
    }
}

/// Begins one process-wide global Provider configuration transaction.
/// 开始一个进程级全局 Provider 配置事务。
///
/// # Returns
/// 返回值
/// An exclusive transaction retained across validation, optional asynchronous preparation, persistence, and runtime commit.
/// 返回一个独占事务，并保持到校验、可选异步准备、持久化与运行时提交全部完成。
pub(crate) async fn begin_provider_configuration_transaction() -> ProviderConfigurationTransaction {
    ProviderConfigurationTransaction {
        _guard: PROVIDER_CONFIGURATION_TRANSACTION_LOCK.lock().await,
    }
}

/// Acquires the transaction guard for one exact project configuration file.
/// 获取单个精确项目配置文件的事务守卫。
///
/// # Parameters
/// 参数
/// `configuration_file` is the standard project configuration file path used as lock identity.
/// `configuration_file` 是作为锁身份的标准项目配置文件路径。
///
/// # Returns
/// 返回值
/// An owned asynchronous guard that serializes this file without blocking unrelated project files.
/// 返回一个自有异步守卫，用于串行化当前文件且不阻塞无关项目文件。
async fn begin_project_configuration_transaction(
    configuration_file: &Path,
) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
    // transaction_lock reuses an active per-file lock and discards expired registry entries.
    // transaction_lock 复用活跃的逐文件锁，并清理注册表中已经失效的条目。
    let transaction_lock = {
        let mut registry = PROJECT_CONFIGURATION_TRANSACTION_LOCKS
            .lock()
            .map_err(|error| format!("failed to lock project configuration registry: {error}"))?;
        registry.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = registry
            .get(configuration_file)
            .and_then(std::sync::Weak::upgrade)
        {
            lock
        } else {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            registry.insert(configuration_file.to_path_buf(), Arc::downgrade(&lock));
            lock
        }
    };
    Ok(transaction_lock.lock_owned().await)
}

/// Mutates and persists one project configuration under its canonical configuration-file lock.
/// 在规范项目配置文件对应的锁下变更并持久化单个项目配置。
///
/// # Parameters
/// 参数
/// `workspace_path` is the authoritative absolute workspace path returned by the retained project snapshot.
/// `workspace_path` 是保留项目快照返回的权威绝对工作区路径。
/// `save_error_context` identifies the user action when persistence fails.
/// `save_error_context` 在持久化失败时标识对应的用户动作。
/// `mutate` applies one validated project configuration mutation.
/// `mutate` 应用单个已经校验的项目配置变更。
/// `commit` applies cache or runtime side effects after persistence succeeds.
/// `commit` 在持久化成功后应用缓存或运行时副作用。
///
/// # Returns
/// 返回值
/// The value produced by `commit` after the latest project configuration is saved.
/// 保存最新项目配置后返回 `commit` 产生的值。
pub(crate) async fn mutate_project_configuration<Mutation, Commit, Output>(
    workspace_path: &Path,
    save_error_context: &str,
    mutate: Mutation,
    commit: Commit,
) -> Result<Output, String>
where
    Mutation: FnOnce(
        &mut vulcan_provider_config::config::ProjectVulcanCodeConfigurationFile,
    ) -> Result<(), String>,
    Commit: FnOnce(
        &vulcan_provider_config::config::ProjectVulcanCodeConfigurationFile,
    ) -> Result<Output, String>,
{
    // configuration_file is the exact persistence target and therefore the transaction identity.
    // configuration_file 是精确持久化目标，因此也是事务身份。
    let configuration_file =
        vulcan_provider_config::config::project_configuration_file_path(workspace_path);
    // transaction_guard serializes the full read-modify-save-commit cycle for this exact project file.
    // transaction_guard 串行化该精确项目文件的完整读取、变更、保存与提交周期。
    let _transaction_guard = begin_project_configuration_transaction(&configuration_file).await?;
    let mut configuration =
        vulcan_provider_config::config::load_project_configuration_file(workspace_path)?;
    mutate(&mut configuration)?;
    vulcan_provider_config::config::save_project_configuration_file(workspace_path, &configuration)
        .map_err(|error| format!("{save_error_context}: {error}"))?;
    commit(&configuration)
}

/// Clones the latest in-memory workspace snapshot retained by the service bridge.
/// 克隆服务桥接层保留的最近内存工作区快照。
pub(crate) fn current_workspace_snapshot_from_state(
    state: &SharedServiceBridgeState,
) -> Result<Option<WorkspaceSnapshot>, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workspace_snapshot.clone())
}

/// Clones the latest in-memory workspace snapshot from a shared service state handle.
/// 从共享服务状态句柄克隆最近的内存工作区快照。
#[cfg(test)]
pub(crate) fn current_workspace_snapshot_from_shared_state(
    state: &SharedServiceBridgeState,
) -> Result<Option<WorkspaceSnapshot>, String> {
    current_workspace_snapshot_from_state(state)
}

/// Applies one icon mutation to retained project snapshots.
/// 将单个图标变更应用到保留的项目快照。
pub(crate) fn update_project_icon_local_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    icon_pack: Option<String>,
    icon_key: Option<String>,
) -> Result<(), String> {
    mutate_retained_project_local_state(
        state,
        project_id,
        "设置图标",
        |project| {
            project.icon_pack = icon_pack.clone();
            project.icon_key = icon_key.clone();
        },
        |project| {
            project.icon_pack = icon_pack.clone();
            project.icon_key = icon_key.clone();
        },
    )
}

/// Applies one retained-project mutation to workspace and workbench projections.
/// 对工作区与工作台两份投影应用一次保留项目变更。
///
/// # Parameters
/// 参数
/// `state` owns the retained workspace and workbench snapshots.
/// `state` 持有保留的工作区与工作台快照。
/// `project_id` identifies the project summary to mutate in both projections.
/// `project_id` 标识需要在两份投影中变更的项目摘要。
/// `missing_project_action` describes the user-facing action used in the not-found error.
/// `missing_project_action` 描述未命中错误中使用的面向用户操作。
/// `mutate_task_project` mutates the workspace task-project summary.
/// `mutate_task_project` 变更工作区任务项目摘要。
/// `mutate_workbench_project` mutates the WebUI workbench project summary.
/// `mutate_workbench_project` 变更 WebUI 工作台项目摘要。
///
/// # Returns
/// 返回值
/// `Ok(())` after at least one projection is changed and the workbench revision is bumped.
/// 至少一份投影被变更且工作台版本推进后返回 `Ok(())`。
fn mutate_retained_project_local_state<TaskProjectMutation, WorkbenchProjectMutation>(
    state: &SharedServiceBridgeState,
    project_id: &str,
    missing_project_action: &str,
    mut mutate_task_project: TaskProjectMutation,
    mut mutate_workbench_project: WorkbenchProjectMutation,
) -> Result<(), String>
where
    TaskProjectMutation: FnMut(&mut TaskProjectSummary),
    WorkbenchProjectMutation: FnMut(&mut ProjectSummary),
{
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut found = false;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        for project in &mut workspace_snapshot.task_projects {
            if project.id == project_id {
                mutate_task_project(project);
                found = true;
            }
        }
    }
    for project in &mut guard.workbench.projects {
        if project.id == project_id {
            mutate_workbench_project(project);
            found = true;
        }
    }
    if found {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        Ok(())
    } else {
        Err(format!(
            "没有找到要{missing_project_action}的项目：{project_id}"
        ))
    }
}

/// Returns the retained LuaSkills handle for WebUI management commands.
/// 返回 WebUI 管理命令使用的保留 LuaSkills 句柄。
pub(crate) fn service_luaskills_handle(
    state: &SharedServiceBridgeState,
) -> Result<TauriLuaSkillsHandle, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard
        .luaskills_runtime
        .clone()
        .ok_or_else(|| String::from("service is still starting; LuaSkills runtime is not ready"))
}

/// Returns the long-lived generic Vulcan Plugin runtime retained by the Agent bridge.
/// 返回 Agent 桥接保留的长生命周期通用 Vulcan 插件运行时。
pub(crate) fn service_vulcan_plugin_runtime(
    state: &SharedServiceBridgeState,
) -> Result<vulcan_plugin_runtime::VulcanPluginRuntimeManager, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.vulcan_plugin_runtime.clone())
        .ok_or_else(|| String::from("Vulcan Plugin runtime is not initialized."))
}

/// Atomically synchronizes the current Vulcan Plugin hook surface into the retained dispatcher.
/// 将当前 Vulcan 插件 hook 面原子同步到保留的调度器。
pub(crate) fn synchronize_service_vulcan_plugin_hooks(
    state: &SharedServiceBridgeState,
    runtime: vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    catalog: &vulcan_plugin_runtime::VulcanPluginCatalogSnapshot,
) -> Result<(), String> {
    let conversation_hook_runtime = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| Arc::clone(&runtime.conversation_hook_runtime))
        .ok_or_else(|| String::from("Agent conversation hook runtime is not initialized."))?;
    let mut hook_runtime = conversation_hook_runtime
        .lock()
        .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
    vulcan_agent_runtime::agent_runtime::vulcan_plugin_hooks::synchronize_vulcan_plugin_hooks(
        hook_runtime.dispatcher_mut(),
        runtime,
        catalog,
    )
    .map_err(|error| error.to_string())
}

/// Validates one plugin's hook declarations against the retained host point registry.
/// 依据保留的宿主点位注册表校验单个插件的 hook 声明。
pub(crate) fn validate_service_vulcan_plugin_hooks(
    state: &SharedServiceBridgeState,
    hooks: &[vulcan_plugin_runtime::VulcanPluginHookDescriptor],
) -> Result<(), String> {
    let conversation_hook_runtime = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| Arc::clone(&runtime.conversation_hook_runtime))
        .ok_or_else(|| String::from("Agent conversation hook runtime is not initialized."))?;
    let mut hook_runtime = conversation_hook_runtime
        .lock()
        .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
    vulcan_agent_runtime::agent_runtime::vulcan_plugin_hooks::validate_vulcan_plugin_hooks(
        hook_runtime.dispatcher_mut(),
        hooks,
    )
    .map_err(|error| error.to_string())
}

/// Drains every Vulcan Plugin session owned by the supplied archived conversations.
/// 排空由给定已归档会话拥有的全部 Vulcan 插件会话。
pub(crate) async fn drain_vulcan_plugin_sessions_for_owners(
    state: &SharedServiceBridgeState,
    owner_session_ids: &[SessionId],
) -> Vec<String> {
    let Ok(runtime) = service_vulcan_plugin_runtime(state) else {
        return Vec::new();
    };
    let mut failures = Vec::new();
    for owner_session_id in owner_session_ids {
        match runtime
            .drain_owner_sessions(&owner_session_id.to_string())
            .await
        {
            Ok(report) => failures.extend(report.failures),
            Err(error) => failures.push(format!(
                "Failed to drain Vulcan Plugin sessions for owner `{owner_session_id}`: {error}"
            )),
        }
    }
    failures
}

/// Stores a refreshed LuaSkills catalog in the shared service state.
/// 在共享服务状态中存储刷新后的 LuaSkills 目录。
pub(crate) fn store_luaskills_catalog(
    state: &SharedServiceBridgeState,
    catalog: TauriLuaSkillsCatalog,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    sync_agent_luaskills_tool_cache(&mut guard)?;
    guard.cached_luaskills_catalog = Some(catalog);
    Ok(())
}

/// Stores a refreshed LuaSkills catalog and appends a diagnostic log line.
/// 存储刷新后的 LuaSkills 目录并追加诊断日志行。
pub(crate) fn store_luaskills_catalog_with_log(
    state: &SharedServiceBridgeState,
    catalog: TauriLuaSkillsCatalog,
    message: String,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    sync_agent_luaskills_tool_cache(&mut guard)?;
    guard.cached_luaskills_catalog = Some(catalog);
    guard
        .startup
        .recent_logs
        .push(format!("[webui-service] luaskills: {message}"));
    Ok(())
}

/// Synchronizes the Agent Runtime LuaSkills tool cache from the shared LuaEngine.
/// 从共享 LuaEngine 同步 Agent Runtime 的 LuaSkills 工具缓存。
pub(crate) fn sync_agent_luaskills_tool_cache(
    guard: &mut ServiceBridgeState,
) -> Result<(), String> {
    let Some(runtime_handle) = guard
        .agent_runtime
        .as_ref()
        .and_then(|runtime| runtime.luaskills_runtime.as_ref())
        .cloned()
    else {
        return Ok(());
    };
    let entries = {
        let tauri_handle = guard.luaskills_runtime.as_ref().ok_or_else(|| {
            String::from("service is still starting; LuaSkills runtime is not ready")
        })?;
        let engine = tauri_handle.engine.lock().map_err(|_| {
            String::from("LuaSkills 引擎锁已损坏，无法同步 Agent Runtime 工具缓存。")
        })?;
        engine
            .list_entries_for_authority(
                vulcan_service_runtime::luaskills::SkillManagementAuthority::System,
            )
            .map_err(|error| format!("读取 Agent Runtime 工具入口失败：{error}"))?
    };
    let entry_count = entries.len();
    let mut tool_entries = runtime_handle.tool_entries.lock().map_err(|_| {
        String::from("LuaSkills 工具缓存锁已损坏，无法同步 Agent Runtime 工具缓存。")
    })?;
    *tool_entries = entries;
    invalidate_runtime_prompt_luaskill_catalog_cache();
    guard.startup.recent_logs.push(format!(
        "[webui-service] luaskills: synced {entry_count} agent-runtime tool entries"
    ));
    Ok(())
}

/// Returns the current provider configuration stored by the agent runtime bridge.
/// 返回 Agent 运行时桥当前保存的 Provider 配置。
pub(crate) fn provider_configuration_from_state(
    state: &SharedServiceBridgeState,
) -> Result<ProviderConfigurationFile, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))
}

/// Mutates, persists, stores, and projects one global Provider configuration transaction.
/// 在单个全局 Provider 配置事务中完成变更、持久化、回存与投影。
///
/// # Parameters
/// 参数
/// `state` owns the retained provider configuration and runtime bridge.
/// `state` 持有保留的 Provider 配置与运行时桥接。
/// `save_error_context` identifies the user action when persistence fails.
/// `save_error_context` 在持久化失败时标识对应的用户动作。
/// `mutate` applies one validated configuration mutation.
/// `mutate` 应用单个已经校验的配置变更。
/// `commit` applies runtime and workbench side effects while service state is still locked.
/// `commit` 在服务状态仍被锁定时应用运行时与工作台副作用。
///
/// # Returns
/// 返回值
/// The value produced by `commit` after configuration persistence and in-memory replacement succeed.
/// 配置持久化与内存替换成功后返回 `commit` 产生的值。
pub(crate) async fn mutate_provider_configuration<Mutation, Commit, Output>(
    state: &SharedServiceBridgeState,
    save_error_context: &str,
    mutate: Mutation,
    commit: Commit,
) -> Result<Output, String>
where
    Mutation: FnOnce(&mut ProviderConfigurationFile) -> Result<(), String>,
    Commit: FnOnce(&mut ServiceBridgeState, &ProviderConfigurationFile) -> Output,
{
    mutate_provider_configuration_with_state_validation(
        state,
        save_error_context,
        |_| Ok(()),
        mutate,
        commit,
    )
    .await
}

/// Validates service-owned state, then mutates, persists, stores, and projects one Provider configuration transaction.
/// 先校验服务持有状态，再在单个 Provider 配置事务中完成变更、持久化、回存与投影。
///
/// # Parameters
/// 参数
/// `state` owns the retained provider configuration, workspace state, and runtime bridge.
/// `state` 持有保留的 Provider 配置、工作区状态与运行时桥接。
/// `save_error_context` identifies the user action when persistence fails.
/// `save_error_context` 在持久化失败时标识对应的用户动作。
/// `validate_state` checks state-dependent ownership while both transaction and service locks are held.
/// `validate_state` 在配置事务锁与服务状态锁均已持有时校验依赖状态的归属。
/// `mutate` applies one validated configuration mutation.
/// `mutate` 应用单个已经校验的配置变更。
/// `commit` applies runtime and workbench side effects while service state remains locked.
/// `commit` 在服务状态仍被锁定时应用运行时与工作台副作用。
///
/// # Returns
/// 返回值
/// The value produced by `commit`, or the validation/persistence failure before any projection is published.
/// 返回 `commit` 产生的值；若校验或持久化失败，则在发布任何投影前返回错误。
pub(crate) async fn mutate_provider_configuration_with_state_validation<
    ValidateState,
    Mutation,
    Commit,
    Output,
>(
    state: &SharedServiceBridgeState,
    save_error_context: &str,
    validate_state: ValidateState,
    mutate: Mutation,
    commit: Commit,
) -> Result<Output, String>
where
    ValidateState: FnOnce(&ServiceBridgeState) -> Result<(), String>,
    Mutation: FnOnce(&mut ProviderConfigurationFile) -> Result<(), String>,
    Commit: FnOnce(&mut ServiceBridgeState, &ProviderConfigurationFile) -> Output,
{
    // transaction keeps clone, validation, persistence, runtime replacement, and projection globally ordered.
    // transaction 使克隆、校验、持久化、运行时替换与投影在全局保持有序。
    let transaction = begin_provider_configuration_transaction().await;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // validateState runs after global serialization and before any durable write, so a target
    // session cannot be replaced by whichever conversation happens to be selected later.
    // validateState 在全局串行化之后、任何持久写入之前运行，因此目标会话不会被随后碰巧选中的
    // 其他会话替代。
    validate_state(&guard)?;
    // configuration starts from the latest retained runtime value after transaction serialization.
    // configuration 在事务串行化后从最新保留运行时值开始。
    let mut configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
    mutate(&mut configuration)?;
    transaction.persist(&configuration, save_error_context)?;
    guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
        .provider_configuration = configuration.clone();
    Ok(commit(&mut guard, &configuration))
}

/// Mutates, persists, and publishes one standalone public AI configuration transaction.
/// 变更、持久化并发布一次独立公共 AI 配置事务。
///
/// # Parameters
/// 参数
/// `state` resolves the installed runtime, `save_error_context` labels persistence failures, and
/// `mutate` validates and changes a cloned configuration.
/// `state` 解析已安装运行时，`save_error_context` 标记持久化失败，`mutate` 校验并变更克隆配置。
///
/// # Returns
/// 返回值
/// The exact persisted and installed configuration snapshot.
/// 返回精确的已持久化并已安装配置快照。
pub(crate) async fn mutate_public_ai_configuration<Mutation>(
    state: &SharedServiceBridgeState,
    save_error_context: &str,
    mutate: Mutation,
) -> Result<vulcan_provider_adapters::PersistedAiConfiguration, String>
where
    Mutation: FnOnce(&mut vulcan_provider_adapters::PersistedAiConfiguration) -> Result<(), String>,
{
    let _transaction = PUBLIC_AI_CONFIGURATION_TRANSACTION_LOCK.lock().await;
    mutate_public_ai_configuration_in_transaction(state, save_error_context, mutate)
}

/// Persists one public AI configuration while keeping standalone custom manifests in sync.
/// 持久化一份公共 AI 配置，同时保持独立自定义清单同步。
///
/// Custom provider bytes live in standalone manifest files below the application data root
/// (plan §R6.1/§R6.2); the centralized document stores only the reference set, so every save
/// writes one recoverable before/target journal, commits the standalone files and projected
/// configuration, then removes the journal.
/// 自定义供应商字节存于应用数据根目录下的独立清单文件（方案 §R6.1/§R6.2）；集中文档
/// 只保存引用集，因此每次保存先写入可恢复的 before/target 日志，再提交独立文件与投影
/// 配置，最后移除日志。
///
/// # Parameters
/// 参数
/// `configuration` is the authoritative in-memory snapshot and `save_error_context` labels
/// persistence failures for the user-facing message.
/// `configuration` 是权威内存快照，`save_error_context` 为面向用户的失败文案标注上下文。
fn persist_public_ai_configuration_with_custom_manifests(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    save_error_context: &str,
) -> Result<(), String> {
    let app_data_root = vulcan_workspace_store::data_plane::application_data_root();
    vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
        configuration,
        &app_data_root,
    )
    .map_err(|error| {
        if save_error_context.is_empty() {
            error
        } else {
            format!("{save_error_context}: {error}")
        }
    })
}

/// Installs one candidate runtime configuration only after its durable persistence succeeds.
/// 仅在候选运行时配置持久化成功后安装该配置。
///
/// # Parameters
/// 参数
/// `runtime` owns the currently visible snapshot, `configuration` is the complete candidate,
/// and `persist` durably commits that exact candidate before installation.
/// `runtime` 持有当前可见快照，`configuration` 是完整候选配置，`persist` 在安装前持久
/// 提交该精确候选。
///
/// # Returns
/// 返回值
/// The persisted and installed candidate, or the persistence/installation failure while the
/// previous runtime snapshot remains visible on persistence failure.
/// 返回已持久化并安装的候选配置；持久化失败时返回错误且先前运行时快照保持可见。
fn persist_then_install_public_ai_configuration<Persist>(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: vulcan_provider_adapters::PersistedAiConfiguration,
    mut persist: Persist,
) -> Result<vulcan_provider_adapters::PersistedAiConfiguration, String>
where
    Persist: FnMut(&vulcan_provider_adapters::PersistedAiConfiguration) -> Result<(), String>,
{
    // beforeImage is the exact durable/runtime configuration restored if activation rejects the candidate.
    // beforeImage 是激活拒绝候选时需要恢复的精确持久化/运行时配置。
    let before_image = runtime.configuration_snapshot()?;
    persist(&configuration)?;
    match runtime.replace_configuration(configuration.clone()) {
        Ok(()) => Ok(configuration),
        Err(primary) => {
            // persistenceCompensation prevents a successfully written candidate from surviving
            // when its matching immutable runtime generation was never published.
            // persistenceCompensation 防止候选写入成功、但其配对不可变运行时代际未发布时
            // 仍残留在持久层。
            let persistence_compensation = persist(&before_image);
            match persistence_compensation {
                Ok(()) => Err(format!(
                    "Public AI configuration activation failed and the durable before-image was restored: {primary}"
                )),
                Err(compensation) => Err(format!(
                    "Public AI configuration activation failed: {primary}; durable before-image restoration also failed: {compensation}"
                )),
            }
        }
    }
}

/// Acquires exclusive ownership of public AI configuration and credential mutations.
/// 获取公共 AI 配置与凭据变更的独占所有权。
///
/// # Returns
/// 返回值
/// A process-wide transaction guard that remains valid until dropped.
/// 返回一个在释放前始终有效的进程级事务守卫。
pub(crate) async fn begin_public_ai_configuration_transaction()
-> tokio::sync::MutexGuard<'static, ()> {
    PUBLIC_AI_CONFIGURATION_TRANSACTION_LOCK.lock().await
}

/// Mutates public AI configuration while the caller owns the transaction guard.
/// 在调用方持有事务守卫期间变更公共 AI 配置。
///
/// # Parameters
/// 参数
/// `state` resolves the installed runtime, `save_error_context` labels persistence failures, and
/// `mutate` validates and changes the latest runtime snapshot.
/// `state` 解析已安装运行时，`save_error_context` 标记持久化失败，`mutate` 校验并变更最新运行时快照。
///
/// # Returns
/// 返回值
/// The exact persisted and installed configuration snapshot.
/// 返回精确的已持久化并已安装配置快照。
pub(crate) fn mutate_public_ai_configuration_in_transaction<Mutation>(
    state: &SharedServiceBridgeState,
    save_error_context: &str,
    mutate: Mutation,
) -> Result<vulcan_provider_adapters::PersistedAiConfiguration, String>
where
    Mutation: FnOnce(&mut vulcan_provider_adapters::PersistedAiConfiguration) -> Result<(), String>,
{
    let runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        std::sync::Arc::clone(
            &guard
                .agent_runtime
                .as_ref()
                .ok_or_else(|| {
                    String::from("Service is still starting; public AI runtime is not ready.")
                })?
                .public_ai_runtime,
        )
    };
    let mut configuration = runtime.configuration_snapshot()?;
    mutate(&mut configuration)?;
    persist_then_install_public_ai_configuration(runtime.as_ref(), configuration, |configuration| {
        persist_public_ai_configuration_with_custom_manifests(configuration, save_error_context)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a failed durable write leaves the browser-visible runtime snapshot unchanged.
    /// 验证持久写入失败会让浏览器可见运行时快照保持不变。
    #[test]
    fn persistence_failure_does_not_install_candidate_public_ai_configuration() {
        let runtime = vulcan_provider_adapters::PublicAiRuntime::unavailable()
            .expect("public AI runtime should initialize");
        let before = runtime
            .configuration_snapshot()
            .expect("before configuration should read");
        let mut candidate = before.clone();
        candidate.usage_refresh_interval_seconds =
            before.usage_refresh_interval_seconds.saturating_add(1);

        let error =
            persist_then_install_public_ai_configuration(&runtime, candidate, |_configuration| {
                Err(String::from("injected durable write failure"))
            })
            .expect_err("injected persistence failure must surface");

        assert_eq!(error, "injected durable write failure");
        assert_eq!(
            runtime
                .configuration_snapshot()
                .expect("runtime configuration should remain readable"),
            before
        );
    }

    /// Verifies a second Provider configuration transaction cannot acquire ownership before the first releases it.
    /// 验证首个 Provider 配置事务释放前，第二个事务无法取得所有权。
    #[tokio::test]
    async fn provider_configuration_transactions_are_exclusive() {
        // first_transaction owns the process-wide configuration write right during the assertion window.
        // first_transaction 在断言窗口内持有进程级配置写权限。
        let first_transaction = begin_provider_configuration_transaction().await;
        // attempt channels prove the waiter started before exclusivity is asserted.
        // attempt 通道证明等待任务已经在互斥断言前启动。
        let (attempt_sender, attempt_receiver) = tokio::sync::oneshot::channel();
        // acquired channels report when the waiting transaction actually obtains ownership.
        // acquired 通道报告等待事务实际取得所有权的时刻。
        let (acquired_sender, mut acquired_receiver) = tokio::sync::oneshot::channel();
        // waiter attempts to acquire the same process-wide transaction in another Tokio task.
        // waiter 在另一个 Tokio 任务中尝试取得同一个进程级事务。
        let waiter = tokio::spawn(async move {
            attempt_sender
                .send(())
                .expect("transaction attempt receiver should remain alive");
            // second_transaction must remain pending until first_transaction is dropped.
            // second_transaction 必须保持等待，直到 first_transaction 被释放。
            let second_transaction = begin_provider_configuration_transaction().await;
            acquired_sender
                .send(())
                .expect("transaction acquired receiver should remain alive");
            drop(second_transaction);
        });

        attempt_receiver
            .await
            .expect("transaction waiter should report its attempt");
        assert!(matches!(
            acquired_receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        drop(first_transaction);
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut acquired_receiver)
            .await
            .expect("waiting transaction should acquire after release")
            .expect("waiting transaction should report acquisition");
        waiter.await.expect("transaction waiter should finish");
    }

    /// Verifies project configuration locks serialize one file while allowing a different file to proceed.
    /// 验证项目配置锁会串行化同一文件，同时允许不同文件继续执行。
    #[tokio::test]
    async fn project_configuration_transactions_are_keyed_by_file() {
        // project_a identifies the project configuration file held by the first transaction.
        // project_a 标识首个事务持有的项目配置文件。
        let project_a = PathBuf::from("project-a/.vulcan/code/config.json");
        // project_b identifies an unrelated project configuration file.
        // project_b 标识无关的另一个项目配置文件。
        let project_b = PathBuf::from("project-b/.vulcan/code/config.json");
        // first_guard owns project_a while both concurrency assertions run.
        // first_guard 在两项并发断言期间持有 project_a。
        let first_guard = begin_project_configuration_transaction(&project_a)
            .await
            .expect("first project transaction should start");
        // attempt channel proves the same-file waiter has started before its blocked state is asserted.
        // attempt 通道证明同文件等待任务已经在阻塞断言前启动。
        let (attempt_sender, attempt_receiver) = tokio::sync::oneshot::channel();
        // same_file_waiter attempts to acquire project_a from another task.
        // same_file_waiter 在另一个任务中尝试取得 project_a。
        let same_file_waiter = tokio::spawn(async move {
            attempt_sender
                .send(())
                .expect("same-file attempt receiver should remain alive");
            begin_project_configuration_transaction(&project_a).await
        });
        // different_guard proves project_b is not blocked by project_a ownership.
        // different_guard 证明 project_b 不会被 project_a 的所有权阻塞。
        let different_guard = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            begin_project_configuration_transaction(&project_b),
        )
        .await
        .expect("different project transaction should not block")
        .expect("different project transaction should start");
        attempt_receiver
            .await
            .expect("same-file waiter should report its attempt");
        assert!(!same_file_waiter.is_finished());
        drop(different_guard);
        drop(first_guard);
        same_file_waiter
            .await
            .expect("same-file waiter task should finish")
            .expect("same-file waiter should acquire after release");
    }

    /// Verifies a malformed project configuration is reported without running mutation or overwriting the file.
    /// 验证损坏的项目配置会被明确报告，且不会执行变更或覆盖原文件。
    #[tokio::test]
    async fn project_configuration_mutation_preserves_malformed_file() {
        // workspace_path isolates the malformed configuration fixture under the operating-system temporary directory.
        // workspace_path 将损坏配置夹具隔离在操作系统临时目录下。
        let workspace_path = std::env::temp_dir().join(format!(
            "vulcan-project-configuration-test-{}",
            uuid::Uuid::new_v4()
        ));
        // configuration_file uses the production path resolver so the test covers the exact persisted target.
        // configuration_file 使用生产路径解析器，使测试覆盖精确的持久化目标。
        let configuration_file =
            vulcan_provider_config::config::project_configuration_file_path(&workspace_path);
        // malformed_contents is deliberately invalid JSON and must survive the rejected transaction byte-for-byte.
        // malformed_contents 是刻意构造的非法 JSON，拒绝事务后必须逐字节保留。
        let malformed_contents = b"{ malformed project configuration";
        std::fs::create_dir_all(
            configuration_file
                .parent()
                .expect("project configuration file should have a parent directory"),
        )
        .expect("project configuration directory should be created");
        std::fs::write(&configuration_file, malformed_contents)
            .expect("malformed project configuration fixture should be written");

        // mutation_called records whether invalid persisted state incorrectly reaches business mutation.
        // mutation_called 记录无效持久化状态是否错误进入业务变更逻辑。
        let mut mutation_called = false;
        // result must preserve the loader failure instead of converting it into a default configuration.
        // result 必须保留加载器错误，而不是将其转换为默认配置。
        let result = mutate_project_configuration(
            &workspace_path,
            "save malformed project configuration test",
            |_| {
                mutation_called = true;
                Ok(())
            },
            |_| Ok(()),
        )
        .await;
        // persisted_contents is captured before cleanup so assertions do not leave temporary data behind.
        // persisted_contents 在清理前读取，使后续断言不会遗留临时数据。
        let persisted_contents = std::fs::read(&configuration_file)
            .expect("malformed project configuration fixture should remain readable");
        std::fs::remove_dir_all(&workspace_path)
            .expect("temporary project configuration workspace should be removed");

        assert!(result.is_err());
        assert!(!mutation_called);
        assert_eq!(persisted_contents, malformed_contents);
    }

    /// Test shell host that prevents unit tests from launching host shell actions.
    /// 防止单元测试启动宿主壳操作的测试宿主壳。
    struct ServiceStateAccessTestShellHost;

    impl ServiceShellHost for ServiceStateAccessTestShellHost {
        /// Rejects file-manager requests issued by accidental test paths.
        /// 拒绝意外测试路径发起的文件管理器请求。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Err(String::from(
                "service state access tests do not open host paths",
            ))
        }

        /// Rejects browser requests issued by accidental test paths.
        /// 拒绝意外测试路径发起的浏览器请求。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Err(String::from(
                "service state access tests do not open host URLs",
            ))
        }
    }

    /// Builds shared service state with the same project retained in both projections.
    /// 构建在两份投影中保留同一项目的共享服务状态。
    ///
    /// # Returns
    /// 返回值
    /// A shared state handle ready for retained project mutation tests.
    /// 返回可用于保留项目变更测试的共享状态句柄。
    fn service_state_with_retained_project() -> SharedServiceBridgeState {
        let state = create_service_bridge_state(Arc::new(ServiceStateAccessTestShellHost));
        let mut guard = state.lock().expect("test service state lock must be valid");
        guard.workbench_generation = 7;
        guard.workbench.revision = 7;
        guard.workbench.projects = vec![ProjectSummary {
            id: String::from("project-a"),
            name: String::from("Project A"),
            path: String::from(r"D:\project-a"),
            pinned: false,
            icon_pack: None,
            icon_key: None,
            vce_enabled: false,
            vce_settings_revision: 1,
        }];
        guard.workspace_snapshot = Some(WorkspaceSnapshot {
            task_projects: vec![TaskProjectSummary::new(
                "project-a",
                "Project A",
                r"D:\project-a",
                false,
                0,
                None,
                None,
                Vec::new(),
            )],
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        });
        drop(guard);
        state
    }

    /// Verifies workspace snapshot accessors share the same retained snapshot source.
    /// 验证工作区快照访问器共享同一个保留快照来源。
    #[test]
    fn current_workspace_snapshot_accessors_share_retained_snapshot() {
        let state = service_state_with_retained_project();

        let direct_snapshot = current_workspace_snapshot_from_state(&state)
            .expect("direct accessor should lock service state")
            .expect("direct accessor should return retained workspace");
        let shared_snapshot = current_workspace_snapshot_from_shared_state(&state)
            .expect("shared accessor should lock service state")
            .expect("shared accessor should return retained workspace");

        assert_eq!(direct_snapshot.task_projects[0].id, "project-a");
        assert_eq!(
            direct_snapshot.task_projects[0].id,
            shared_snapshot.task_projects[0].id
        );
    }

    /// Verifies icon updates stay synchronized across retained projections.
    /// 验证图标更新会在两份保留投影中保持同步。
    #[test]
    fn update_project_icon_local_state_updates_both_projections() {
        let state = service_state_with_retained_project();

        update_project_icon_local_state(
            &state,
            "project-a",
            Some(String::from("lucide")),
            Some(String::from("folder")),
        )
        .expect("icon update should mutate retained projections");

        let guard = state.lock().expect("test service state lock must be valid");
        let workspace_snapshot = guard
            .workspace_snapshot
            .as_ref()
            .expect("test workspace snapshot must exist");
        assert_eq!(
            Some(String::from("lucide")),
            workspace_snapshot.task_projects[0].icon_pack
        );
        assert_eq!(
            Some(String::from("folder")),
            workspace_snapshot.task_projects[0].icon_key
        );
        assert_eq!(
            Some(String::from("lucide")),
            guard.workbench.projects[0].icon_pack
        );
        assert_eq!(
            Some(String::from("folder")),
            guard.workbench.projects[0].icon_key
        );
        assert_eq!(8, guard.workbench_generation);
        assert_eq!(8, guard.workbench.revision);
    }

    /// Verifies missing retained projects do not advance the workbench revision.
    /// 验证未命中保留项目时不会推进工作台版本。
    #[test]
    fn update_project_local_state_missing_project_preserves_revision() {
        let state = service_state_with_retained_project();

        let error = update_project_icon_local_state(
            &state,
            "missing-project",
            Some(String::from("lucide")),
            Some(String::from("folder")),
        )
        .expect_err("missing project should return a retained-project diagnostic");

        let guard = state.lock().expect("test service state lock must be valid");
        assert_eq!("没有找到要设置图标的项目：missing-project", error);
        assert_eq!(7, guard.workbench_generation);
        assert_eq!(7, guard.workbench.revision);
    }
}
