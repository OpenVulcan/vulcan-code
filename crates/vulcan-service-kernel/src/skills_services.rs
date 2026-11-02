//! LuaSkills and AgentSkills service handlers.
//! LuaSkills 与 AgentSkills 服务处理器。

use super::*;
use vulcan_luaskills_config::LuaSkillsRuntimeSettings;
use vulcan_service_api::{
    LuaSkillsManagedRuntimeProgressSummary, ProjectSettingsDomain, SubagentDefinitionPrompts,
};

mod local_plugin_uninstall;
mod local_plugin_update;
mod luaskills_debug;
mod plugin_credentials;
mod plugin_pending_operations;
pub(crate) use local_plugin_uninstall::LocalPluginUninstallDrainDiagnostics;
pub(crate) use local_plugin_uninstall::LocalPluginUninstallProjection;
pub(crate) use local_plugin_uninstall::PendingLocalPluginUninstallRegistry;
pub(crate) use local_plugin_update::LocalPluginUpdateStatus;
pub(crate) use local_plugin_update::pending_owner::PendingLocalPluginUpdateRegistry;
pub(crate) use luaskills_debug::{
    KernelLuaSkillsDebugHost, service_luaskills_debug_install_status,
    service_luaskills_debug_status, service_start_luaskills_debug_install,
};
pub(crate) use plugin_credentials::service_set_vulcan_plugin_credentials;
pub(crate) use plugin_pending_operations::{
    PluginPendingContinuation, service_continue_plugin_pending_operation,
    service_plugin_pending_operations,
};
mod vulcan_plugin_lifecycle;

/// Serializes user-agent catalog file mutations across concurrent service requests.
/// 串行化并发服务请求之间的用户智能体目录文件变更。
static SUBAGENT_CATALOG_MUTATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serializes LuaSkills lifecycle requests through catalog storage and domain projection.
/// 将 LuaSkills 生命周期请求串行化到目录存储与领域投影完成为止。
static LUASKILLS_MANAGEMENT_MUTATION_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// Serializes Agent Skills package, catalog, configuration, and projection mutations.
/// 串行化 Agent Skills 技能包、目录、配置与投影变更。
static AGENT_SKILLS_CATALOG_MUTATION_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// One user-supplied Secret required to validate a local MCP server during installation.
/// 安装期间校验本地 MCP 服务器所需的单个用户提供 Secret。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VulcanPluginInitialSecretInput {
    /// Plugin-local MCP server identifier from the inspected permission preview.
    /// 来自已检查权限预览的插件内 MCP 服务器标识。
    server_id: String,
    /// Exact manifest-declared Secret key.
    /// 清单声明的精确 Secret 键。
    secret_key: String,
    /// Non-empty Secret value written only to the operating-system credential store.
    /// 仅写入操作系统凭据库的非空 Secret 值。
    value: String,
}

/// Acquires the process-wide user-agent catalog mutation guard.
/// 获取进程级用户智能体目录变更守卫。
///
/// # Returns
/// 返回值
/// A guard that keeps catalog discovery, file mutation, and state reprojection in one transaction.
/// 返回一个守卫，使目录发现、文件变更与状态重投影保持在同一事务中。
fn lock_subagent_catalog_mutation() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    SUBAGENT_CATALOG_MUTATION_LOCK
        .lock()
        .map_err(|error| format!("failed to lock subagent catalog mutation: {error}"))
}

/// Acquires the process-wide LuaSkills lifecycle transaction guard.
/// 获取进程级 LuaSkills 生命周期事务守卫。
///
/// # Returns
/// 返回值
/// An asynchronous guard retained until runtime mutation, catalog storage, and projection finish.
/// 返回一个异步守卫，并保持到运行时变更、目录存储与投影全部结束。
async fn lock_luaskills_management_mutation() -> tokio::sync::MutexGuard<'static, ()> {
    LUASKILLS_MANAGEMENT_MUTATION_LOCK.lock().await
}

/// Runs one LuaSkills management transaction in a service-owned asynchronous task.
/// 在服务自有的异步任务中运行一次 LuaSkills 管理事务。
///
/// # Parameters
/// 参数
/// `label` identifies join failures and `operation` owns the complete mutation, persistence,
/// projection, and progress-finalization span.
/// `label` 标识联接失败，`operation` 拥有完整的变更、持久化、投影与进度终结区间。
///
/// # Returns
/// 返回值
/// Returns the transaction result after the service-owned task completes.
/// 在服务自有任务完成后返回事务结果。
///
/// Dropping an RPC waiter detaches a Tokio join handle but must not release the management
/// lock while a nested `spawn_blocking` mutation is still running. The inner task therefore
/// owns both the lock guard and the whole post-mutation projection. Physical mutation owners
/// must separately survive task panic or cancellation in their service registries.
/// 丢弃 RPC 等待者会分离 Tokio 联接句柄，但不得在嵌套的 `spawn_blocking` 变更仍运行时释放
/// 管理锁。因此内部任务同时拥有锁守卫与完整后置投影。物理变更所有者必须通过独立服务注册表
/// 跨任务 panic 或取消存续。
async fn run_luaskills_management_owned<T, F, Fut>(
    label: &'static str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, String>> + Send + 'static,
{
    // Capture the explicitly scoped test destination before Tokio creates a task that does not inherit task locals.
    // 在 Tokio 创建不继承任务局部值的任务之前，捕获明确设置作用域的测试目标。
    #[cfg(test)]
    let test_configuration_destination =
        crate::tests::provider_configuration_persistence_scope::current_path();
    // The same service task owns serialization and the complete operation after the caller drops its join observer.
    // 调用方丢弃联接观测者之后，相同服务任务拥有串行化及完整操作。
    let owned_operation = async move {
        // Keep the existing management guard alive throughout persistence and final projection.
        // 在持久化及最终投影期间持续保留既有管理守卫。
        let _management_guard = lock_luaskills_management_mutation().await;
        operation().await
    };
    // Optional test context is explicitly defined by current_path; only an actual installed scope is propagated.
    // 可选测试上下文由 current_path 明确定义；仅传递实际已安装作用域。
    #[cfg(test)]
    let owned_operation = async move {
        match test_configuration_destination {
            Some(path) => {
                crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH
                    .scope(path, owned_operation)
                    .await
            }
            None => owned_operation.await,
        }
    };
    spawn_service_task(owned_operation).await.map_err(|error| {
        format!("LuaSkills management transaction `{label}` task ended abnormally: {error}")
    })?
}

/// Acquires the complete Agent Skills catalog transaction guard.
/// 获取完整 Agent Skills 目录事务守卫。
pub(crate) async fn lock_agent_skills_catalog_mutation() -> tokio::sync::MutexGuard<'static, ()> {
    AGENT_SKILLS_CATALOG_MUTATION_LOCK.lock().await
}

/// Refreshes the committed LuaSkills projection and broadcasts it through the sole service publisher.
/// 刷新已提交的 LuaSkills 投影，并通过唯一服务发布出口广播。
///
/// # Parameters
/// 参数
/// `app` broadcasts the committed domain update to every connected endpoint.
/// `app` 将已提交领域更新广播给每个已连接端点。
/// `state` owns the authoritative LuaSkills catalog and workbench generation.
/// `state` 持有权威 LuaSkills 目录与工作台版本。
///
/// # Returns
/// 返回值
/// Returns the same revisioned update delivered to subscribers, or the projection error.
/// 返回与订阅端收到内容相同的带版本更新，或投影错误。
async fn refresh_luaskills_local_state_and_emit(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // update is created once so the RPC response and broadcast share one exact revision.
    // update 只创建一次，使 RPC 响应与广播共享同一个精确版本。
    let update = refresh_luaskills_local_state(state).await?;
    emit_luaskills_catalog_delta(app, update.clone());
    Ok(update)
}

/// Refreshes every plugin-owned catalog domain and publishes one atomic replacement event.
/// 刷新全部插件所有目录领域，并发布一条原子替换事件。
///
/// # Parameters
/// 参数
/// `app` broadcasts the committed event and `state` owns the authoritative plugin snapshot.
/// `app` 广播已提交事件，`state` 持有权威插件快照。
///
/// # Returns
/// 返回值
/// Returns the legacy LuaSkills response projected at the same revision as the atomic event.
/// 返回与原子事件使用同一版本投影的兼容 LuaSkills 响应。
async fn refresh_plugin_catalog_local_state_and_emit(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // update is created once so the event and RPC response cannot observe different plugin snapshots.
    // update 只创建一次，确保事件与 RPC 响应不会观察到不同插件快照。
    let update = refresh_plugin_catalog_local_state(state).await?;
    emit_plugin_catalog_delta(app, &update);
    Ok(update.lua_skills_update())
}

/// Runs one blocking Agent Skills filesystem operation on the blocking pool.
/// 在阻塞线程池上执行一次阻塞式 Agent Skills 文件系统操作。
///
/// Package and category writes walk and rewrite whole directories, so they must not occupy an async
/// worker. Returns the operation result, or an error when the blocking task was cancelled or panicked.
/// 技能包与分类写入会遍历并重写整个目录，因此不得占用异步工作线程。返回操作结果；
/// 阻塞任务被取消或 panic 时返回错误。
async fn run_agent_skill_file_operation<T, F>(operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| format!("Agent Skills file operation did not complete: {error}"))?
}

/// Refreshes the committed Agent Skills projection and broadcasts it through the sole service publisher.
/// 刷新已提交的 Agent Skills 投影，并通过唯一服务发布出口广播。
///
/// # Parameters
/// 参数
/// `app` broadcasts the committed domain update to every connected endpoint.
/// `app` 将已提交领域更新广播给每个已连接端点。
/// `state` owns the authoritative Agent Skills projection and workbench generation.
/// `state` 持有权威 Agent Skills 投影与工作台版本。
///
/// # Returns
/// 返回值
/// Returns the same revisioned update delivered to subscribers, or the projection error.
/// 返回与订阅端收到内容相同的带版本更新，或投影错误。
async fn refresh_agent_skills_local_state_and_emit(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogUpdate, String> {
    // update is created once so RPC response and broadcast share one exact revision.
    // update 只创建一次，使 RPC 响应与广播共享同一个精确版本。
    let update = refresh_agent_skills_local_state(state).await?;
    emit_agent_skills_catalog_delta(app, update.clone());
    Ok(update)
}

/// Publishes one already committed subagent catalog update and returns the identical response value.
/// 发布一份已提交的智能体目录更新，并返回完全相同的响应值。
///
/// # Parameters
/// 参数
/// `app` broadcasts the committed update to every connected endpoint.
/// `app` 将已提交更新广播给每个已连接端点。
/// `update` is the exact catalog revision already committed to shared state.
/// `update` 是已经提交到共享状态的精确目录版本。
///
/// # Returns
/// 返回值
/// Returns the unchanged update after one authoritative broadcast.
/// 在执行一次权威广播后返回未改变的更新。
fn publish_subagent_catalog_update(
    app: &ServiceEventEmitter,
    update: SubagentCatalogUpdate,
) -> SubagentCatalogUpdate {
    emit_subagent_catalog_delta(app, update.clone());
    update
}

/// Refreshes and publishes one subagent catalog projection through the sole service publisher.
/// 通过唯一服务发布出口刷新并发布一份智能体目录投影。
///
/// # Parameters
/// 参数
/// `app` broadcasts the committed update to every connected endpoint.
/// `app` 将已提交更新广播给每个已连接端点。
/// `state` owns the authoritative subagent catalog projection.
/// `state` 持有权威智能体目录投影。
///
/// # Returns
/// 返回值
/// Returns the committed update after broadcasting it exactly once.
/// 在恰好一次广播后返回已提交更新。
fn refresh_subagent_catalog_local_state_and_emit(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    let update = refresh_subagent_catalog_local_state(state)?;
    Ok(publish_subagent_catalog_update(app, update))
}

/// Runs one blocking extension-management operation outside the asynchronous service executor.
/// 在异步服务执行器之外运行单个阻塞式扩展管理操作。
///
/// # Parameters
/// 参数
/// `label` identifies the operation in diagnostics and `operation` performs the blocking work.
/// `label` 在诊断中标识操作，`operation` 执行阻塞工作。
///
/// # Returns
/// 返回值
/// Returns the operation value or a contextual task/join error.
/// 返回操作结果，或带上下文的任务及联接错误。
pub(crate) async fn run_extension_management_blocking<T, F>(
    label: &'static str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    spawn_service_blocking_task(operation)
        .await
        .map_err(|error| {
            format!("Extension management operation `{label}` task ended abnormally: {error}")
        })?
}

/// Refreshes the LuaSkills catalog used by the WebUI skill page.
/// 刷新 WebUI 技能页面使用的 LuaSkills 目录。
pub(crate) async fn service_refresh_luaskills_catalog(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("refresh catalog", move || async move {
        // Whole discovery must not invalidate an original installation's callback generation.
        // 整个发现不能使原安装的回调代次失效。
        {
            // Discovery must preserve both installation routes and an update's retained actual Full/catalog identity.
            // 发现必须同时保留安装路由及更新保留的实际 Full／目录身份。
            let guard = state.lock().map_err(|error| error.to_string())?;
            crate::service_state_access::require_plugin_update_full_publication_admission(
                &guard, None,
            )?;
        }
        let handle = service_luaskills_handle(&state)?;
        let provider_configuration = provider_configuration_from_state(&state)?;
        let vulcan_plugin_runtime = service_vulcan_plugin_runtime(&state)?;
        let preview_catalog = vulcan_plugin_runtime::load_vulcan_plugin_catalog(
            vulcan_plugin_runtime.vulcan_plugin_root(),
            Some(vulcan_plugin_runtime.user_plugin_root()),
            &provider_configuration.luaskills.disabled_vulcan_plugins,
            &provider_configuration.luaskills.enabled_vulcan_plugins,
            0,
        );
        let preview_hooks = preview_catalog
            .enabled_hooks
            .values()
            .cloned()
            .collect::<Vec<_>>();
        validate_service_vulcan_plugin_hooks(&state, &preview_hooks)
            .map_err(|error| format!("Vulcan plugin hook refresh precheck failed: {error}"))?;
        let catalog = run_extension_management_blocking("refresh catalog", move || {
            vulcan_service_runtime::luaskills_bridge::refresh_luaskills_catalog(&handle)
        })
        .await?;
        store_luaskills_catalog(&state, catalog)?;
        let vulcan_plugin_catalog = vulcan_plugin_runtime
            .refresh_catalog(
                &provider_configuration.luaskills.disabled_vulcan_plugins,
                &provider_configuration.luaskills.enabled_vulcan_plugins,
            )
            .map_err(|error| error.to_string())?;
        synchronize_service_vulcan_plugin_hooks(
            &state,
            vulcan_plugin_runtime,
            &vulcan_plugin_catalog,
        )?;
        refresh_plugin_catalog_local_state_and_emit(&app, &state).await
    })
    .await
}

/// Loads one host-safe LuaSkills package configuration descriptor for the WebUI.
/// 为 WebUI 加载单个宿主安全的 LuaSkills 技能包配置描述。
///
/// `state` owns the retained engine and `skill_id` selects one effective package.
/// `state` 持有保留引擎，`skill_id` 选择一个生效技能包。
///
/// Returns no descriptor when the package declares no configuration.
/// 当技能包未声明配置时返回空。
pub(crate) async fn service_describe_luaskill_config(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<Option<LuaSkillConfigDescriptor>, String> {
    let handle = service_luaskills_handle(&state)?;
    let descriptor = run_extension_management_blocking("describe package config", move || {
        vulcan_service_runtime::luaskills_bridge::describe_luaskill_package_config(
            &handle, &skill_id,
        )
    })
    .await?;
    descriptor
        .map(project_luaskill_config_descriptor)
        .transpose()
}

/// Validates one LuaSkills package configuration without changing persisted values.
/// 校验单个 LuaSkills 技能包配置且不修改持久化值。
///
/// `state` owns the retained engine and `skill_id` selects one effective package.
/// `state` 持有保留引擎，`skill_id` 选择一个生效技能包。
///
/// Returns the authoritative completeness and validity result.
/// 返回权威的完整性与合法性结果。
pub(crate) async fn service_validate_luaskill_config(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<LuaSkillConfigValidation, String> {
    let handle = service_luaskills_handle(&state)?;
    let status = run_extension_management_blocking("validate package config", move || {
        vulcan_service_runtime::luaskills_bridge::validate_luaskill_package_config(
            &handle, &skill_id,
        )
    })
    .await?;
    Ok(project_luaskill_config_validation(status))
}

/// Atomically updates one typed LuaSkills package configuration batch and refreshes catalog state.
/// 原子更新一组类型化 LuaSkills 技能包配置并刷新目录状态。
///
/// `request` carries the exact skill id, observed revision, and non-empty typed values.
/// `request` 携带精确技能标识、已观察修订号与非空类型化值。
///
/// Returns the freshly projected host-safe descriptor after a successful CAS transaction.
/// CAS 事务成功后返回重新投影的宿主安全描述。
pub(crate) async fn service_set_luaskill_config(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: LuaSkillConfigSetRequest,
) -> Result<LuaSkillConfigSetResult, String> {
    if request.values.is_empty() {
        return Err(String::from(
            "CONFIG_BATCH_EMPTY: package configuration updates require at least one value",
        ));
    }
    let values = request
        .values
        .into_iter()
        .map(|(key, value)| project_luaskill_config_input(value).map(|projected| (key, projected)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    run_luaskills_management_owned("set package config", move || async move {
        let handle = service_luaskills_handle(&state)?;
        let skill_id = request.skill_id;
        let expected_revision = request.expected_revision;
        let (result, descriptor) =
            run_extension_management_blocking("set package config", move || {
                vulcan_service_runtime::luaskills_bridge::set_luaskill_package_config(
                    &handle,
                    &skill_id,
                    values,
                    &expected_revision,
                )
            })
            .await?;
        refresh_luaskills_local_state_and_emit(&app, &state).await?;
        project_luaskill_config_set_result(result, descriptor)
    })
    .await
}

/// Projects one upstream write result while removing canonical values for sensitive declarations.
/// 投影单个上游写入结果，并移除敏感声明对应的规范值。
///
/// `result` is the committed transaction result and `descriptor` is the matching post-write snapshot.
/// `result` 是已提交事务结果，`descriptor` 是对应的写入后快照。
///
/// Returns a host-safe mutation result with restart metadata.
/// 返回包含重启元数据的宿主安全变更结果。
fn project_luaskill_config_set_result(
    result: luaskills::SkillConfigWriteResult,
    descriptor: luaskills::RuntimeSkillPackageConfigDescriptor,
) -> Result<LuaSkillConfigSetResult, String> {
    // sensitive_keys is derived from the authoritative post-write declaration snapshot.
    // sensitive_keys 从权威的写入后声明快照派生。
    let sensitive_keys = descriptor
        .items
        .iter()
        .filter(|item| item.sensitive)
        .map(|item| item.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    // restart_required_keys is limited to values that actually changed in this transaction.
    // restart_required_keys 仅包含当前事务中实际发生变化的值。
    let restart_required_keys = descriptor
        .items
        .iter()
        .filter(|item| item.restart_required && result.changed_keys.contains(&item.key))
        .map(|item| item.key.clone())
        .collect();
    // values retains canonical results only for declarations allowed by host disclosure policy.
    // values 仅为宿主披露策略允许的声明保留规范结果。
    let values = result
        .values
        .into_iter()
        .filter(|(key, _)| !sensitive_keys.contains(key.as_str()))
        .collect();
    Ok(LuaSkillConfigSetResult {
        descriptor: project_luaskill_config_descriptor(descriptor)?,
        revision: result.revision,
        changed: result.changed,
        values,
        changed_keys: result.changed_keys,
        restart_required_keys,
    })
}

/// Deletes one explicit or orphaned LuaSkills package configuration value and refreshes catalog state.
/// 删除单个显式或遗留 LuaSkills 技能包配置值并刷新目录状态。
///
/// `request` carries the exact skill id, key, and observed revision.
/// `request` 携带精确技能标识、配置键与已观察修订号。
///
/// Returns the freshly projected host-safe descriptor after a successful CAS transaction.
/// CAS 事务成功后返回重新投影的宿主安全描述。
pub(crate) async fn service_delete_luaskill_config(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: LuaSkillConfigDeleteRequest,
) -> Result<LuaSkillConfigDeleteResult, String> {
    run_luaskills_management_owned("delete package config", move || async move {
        let handle = service_luaskills_handle(&state)?;
        let skill_id = request.skill_id;
        let key = request.key;
        let expected_revision = request.expected_revision;
        let (result, descriptor) =
            run_extension_management_blocking("delete package config", move || {
                vulcan_service_runtime::luaskills_bridge::delete_luaskill_package_config_value(
                    &handle,
                    &skill_id,
                    &key,
                    &expected_revision,
                )
            })
            .await?;
        refresh_luaskills_local_state_and_emit(&app, &state).await?;
        project_luaskill_config_delete_result(result, descriptor)
    })
    .await
}

/// Projects one upstream deletion result together with its matching host-safe snapshot.
/// 把单个上游删除结果及其对应宿主安全快照一并投影。
///
/// `result` is the committed deletion and `descriptor` is the post-delete snapshot.
/// `result` 是已提交删除结果，`descriptor` 是删除后快照。
///
/// Returns the complete host-safe deletion result.
/// 返回完整宿主安全删除结果。
fn project_luaskill_config_delete_result(
    result: luaskills::SkillConfigDeleteResult,
    descriptor: luaskills::RuntimeSkillPackageConfigDescriptor,
) -> Result<LuaSkillConfigDeleteResult, String> {
    Ok(LuaSkillConfigDeleteResult {
        descriptor: project_luaskill_config_descriptor(descriptor)?,
        revision: result.revision,
        deleted: result.deleted,
        key: result.key,
    })
}

/// Explicitly refreshes one selected LuaSkills configuration store or both stores.
/// 显式刷新一个选定 LuaSkills 配置存储或两个存储。
///
/// `request` contains only the optional exact upstream store scope.
/// `request` 仅包含可选的精确上游存储作用域。
///
/// Returns one value-free refresh result per selected store and updates the catalog when changed.
/// 每个选定存储返回一个不含值的刷新结果，并在发生变化时更新目录。
pub(crate) async fn service_refresh_luaskill_config(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: LuaSkillConfigRefreshRequest,
) -> Result<Vec<LuaSkillConfigStoreRefresh>, String> {
    run_luaskills_management_owned("refresh package config", move || async move {
        let handle = service_luaskills_handle(&state)?;
        let refreshes = run_extension_management_blocking("refresh package config", move || {
            vulcan_service_runtime::luaskills_bridge::refresh_luaskill_package_config(
                &handle,
                request.store_scope.as_deref(),
            )
        })
        .await?;
        if refreshes.iter().any(|refresh| refresh.changed) {
            refresh_luaskills_local_state_and_emit(&app, &state).await?;
        }
        Ok(refreshes
            .into_iter()
            .map(|refresh| LuaSkillConfigStoreRefresh {
                store_scope: refresh.store_scope,
                revision: refresh.revision,
                changed: refresh.changed,
            })
            .collect())
    })
    .await
}

/// Polls ordered LuaSkills configuration events and synchronizes catalog state after observed changes.
/// 轮询有序 LuaSkills 配置事件，并在观察到变更后同步目录状态。
///
/// `after_sequence` is an optional engine-local cursor.
/// `after_sequence` 是可选的引擎内游标。
///
/// Returns a bounded event batch with a new cursor.
/// 返回带新游标的有界事件批次。
pub(crate) async fn service_poll_luaskill_config_events(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    after_sequence: Option<String>,
) -> Result<LuaSkillConfigEventBatch, String> {
    let handle = service_luaskills_handle(&state)?;
    let (batch, cursor_recovered) =
        run_extension_management_blocking("poll package config events", move || {
            poll_luaskill_config_events_to_current_boundary(&handle, after_sequence)
        })
        .await?;
    if cursor_recovered || !batch.events.is_empty() {
        // The projection owns its narrower configuration/state ordering. A read-only,
        // three-second UI poll must not enter the package-management mutation queue.
        // 投影自身拥有更窄的配置及状态顺序。三秒一次的只读 UI 轮询不得进入技能包管理变更队列。
        refresh_luaskills_local_state_and_emit(&app, &state).await?;
    }
    Ok(project_luaskill_config_event_batch(batch))
}

/// Polls every currently retained configuration event through bounded upstream pages.
/// 通过有界上游分页轮询当前保留的全部配置事件。
///
/// `handle` owns the retained engine and `after_sequence` is the caller's last committed cursor.
/// `handle` 持有保留引擎，`after_sequence` 是调用方最后提交的游标。
///
/// Returns one aggregated bounded queue snapshot and whether an expired cursor required resynchronization.
/// 返回聚合后的有界队列快照，以及是否因游标过期执行了重新同步。
fn poll_luaskill_config_events_to_current_boundary(
    handle: &vulcan_service_runtime::luaskills_bridge::TauriLuaSkillsHandle,
    after_sequence: Option<String>,
) -> Result<(luaskills::RuntimeSkillConfigEventBatch, bool), String> {
    // page_limit bounds each call according to the upstream event contract.
    // page_limit 按上游事件契约限制每次调用。
    const PAGE_LIMIT: usize = 128;
    // cursor advances only after one complete upstream page has been retained locally.
    // cursor 仅在完整保留一个上游分页后推进。
    let mut cursor = after_sequence;
    // events accumulates at most the upstream retained queue capacity.
    // events 最多累积到上游保留队列容量。
    let mut events = Vec::new();
    // cursor_recovered records one explicit summary-resynchronization requirement.
    // cursor_recovered 记录一次明确的摘要重新同步要求。
    let mut cursor_recovered = false;
    loop {
        let batch =
            match vulcan_service_runtime::luaskills_bridge::poll_luaskill_package_config_events(
                handle,
                cursor.as_deref(),
                PAGE_LIMIT,
            ) {
                Ok(batch) => batch,
                Err(error) if !cursor_recovered => {
                    let Some(recovery_cursor) =
                        expired_luaskill_config_event_recovery_cursor(&error)
                    else {
                        return Err(error);
                    };
                    cursor = Some(recovery_cursor);
                    cursor_recovered = true;
                    continue;
                }
                Err(error) => return Err(error),
            };
        // page_length decides whether another upstream page is currently available.
        // page_length 决定当前是否还有下一个上游分页。
        let page_length = batch.events.len();
        cursor = Some(batch.next_sequence.clone());
        events.extend(batch.events);
        if page_length < PAGE_LIMIT || events.len() >= luaskills::SKILL_CONFIG_EVENT_QUEUE_CAPACITY
        {
            return Ok((
                luaskills::RuntimeSkillConfigEventBatch {
                    events,
                    next_sequence: cursor.unwrap_or_else(|| String::from("0")),
                },
                cursor_recovered,
            ));
        }
    }
}

/// Extracts the exact recovery boundary from one upstream expired-cursor diagnostic.
/// 从单个上游游标过期诊断中提取精确恢复边界。
///
/// `error` must use LuaSkills 0.5.5's stable expired-cursor message.
/// `error` 必须使用 LuaSkills 0.5.5 的稳定游标过期消息。
///
/// Returns the canonical cursor immediately before the oldest retained event.
/// 返回最老保留事件之前的规范游标。
fn expired_luaskill_config_event_recovery_cursor(error: &str) -> Option<String> {
    // marker is the exact boundary label emitted by LuaSkills after its stable error code.
    // marker 是 LuaSkills 在稳定错误码之后输出的精确边界标签。
    const MARKER: &str = " precedes the oldest retained sequence ";
    if !error.starts_with("CONFIG_EVENT_CURSOR_EXPIRED: event cursor ") {
        return None;
    }
    let (_, oldest) = error.rsplit_once(MARKER)?;
    oldest
        .parse::<u64>()
        .ok()?
        .checked_sub(1)
        .map(|sequence| sequence.to_string())
}

/// Converts one JSON scalar into the exact LuaSkills typed configuration input.
/// 把单个 JSON 标量转换为精确的 LuaSkills 类型化配置输入。
///
/// `value` must be a string, signed integer, finite float, or boolean.
/// `value` 必须是字符串、有符号整数、有限浮点数或布尔值。
///
/// Returns one upstream input scalar or an explicit type error.
/// 返回一个上游输入标量或显式类型错误。
fn project_luaskill_config_input(
    value: serde_json::Value,
) -> Result<luaskills::SkillPackageConfigInputValue, String> {
    match value {
        serde_json::Value::String(value) => {
            Ok(luaskills::SkillPackageConfigInputValue::String(value))
        }
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                return Ok(luaskills::SkillPackageConfigInputValue::Integer(value));
            }
            let value = value.as_f64().ok_or_else(|| {
                String::from(
                    "CONFIG_VALUE_TYPE_INVALID: configuration values must be finite JSON scalars",
                )
            })?;
            if !value.is_finite() {
                return Err(String::from(
                    "CONFIG_VALUE_TYPE_INVALID: configuration values must be finite JSON scalars",
                ));
            }
            Ok(luaskills::SkillPackageConfigInputValue::Float(value))
        }
        serde_json::Value::Bool(value) => {
            Ok(luaskills::SkillPackageConfigInputValue::Boolean(value))
        }
        _ => Err(String::from(
            "CONFIG_VALUE_TYPE_INVALID: configuration values must be string, integer, float, or boolean scalars",
        )),
    }
}

/// Projects one upstream package configuration descriptor into the host WebUI contract.
/// 把单个上游技能包配置描述投影为宿主 WebUI 契约。
///
/// `descriptor` has already had every sensitive effective value removed by the service runtime.
/// `descriptor` 已由服务运行时移除所有敏感有效值。
///
/// Returns a camel-case wire descriptor or a serialization failure for declaration metadata.
/// 返回 camelCase 线协议描述，或声明元数据序列化失败。
fn project_luaskill_config_descriptor(
    descriptor: luaskills::RuntimeSkillPackageConfigDescriptor,
) -> Result<LuaSkillConfigDescriptor, String> {
    let items = descriptor
        .items
        .into_iter()
        .map(|item| {
            // item_key preserves the declaration identity for both the row and any attached validation issue.
            // item_key 同时为配置项行与其附加校验问题保留声明身份。
            let item_key = item.key.clone();
            let value_type = item.value_type.as_str().to_string();
            let constraints = serde_json::to_value(item.constraints).map_err(|error| {
                format!("Failed to serialize the skill package configuration constraints: {error}")
            })?;
            let format = item
                .format
                .map(|format| {
                    serde_json::to_value(format).map_err(|error| {
                        format!(
                            "Failed to serialize the skill package configuration format: {error}"
                        )
                    })
                })
                .transpose()?
                .and_then(|value| value.as_str().map(str::to_string));
            let state = serde_json::to_value(item.state)
                .map_err(|error| {
                    format!("Failed to serialize the skill package configuration state: {error}")
                })?
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| {
                    String::from(
                        "The skill package configuration state did not serialize to a string.",
                    )
                })?;
            Ok(LuaSkillConfigItem {
                key: item.key,
                value_type,
                required: item.required,
                sensitive: item.sensitive,
                description: item.description,
                constraints,
                options: item
                    .options
                    .into_iter()
                    .map(|option| LuaSkillConfigEnumOption {
                        value: option.value,
                        label: option.label,
                        description: option.description,
                    })
                    .collect(),
                default: item.default,
                title: item.title,
                group: item.group,
                order: item.order,
                advanced: item.advanced,
                placeholder: item.placeholder,
                example: item.example,
                format,
                restart_required: item.restart_required,
                deprecated: item.deprecated,
                deprecation_message: item.deprecation_message,
                state,
                satisfied: item.satisfied,
                validation_error: item.validation_error.map(|error| LuaSkillConfigIssue {
                    key: Some(item_key),
                    code: error.code,
                    message: error.message,
                }),
                value: item.value,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(LuaSkillConfigDescriptor {
        skill_id: descriptor.skill_id,
        skill_version: descriptor.skill_version,
        complete: descriptor.complete,
        revision: descriptor.revision,
        store_scope: descriptor.store_scope,
        missing_count: descriptor.missing_count,
        invalid_count: descriptor.invalid_count,
        business_issue_count: descriptor.business_issue_count,
        orphaned_count: descriptor.orphaned_count,
        orphaned: descriptor.orphaned,
        items,
    })
}

/// Projects one upstream package configuration validation status into the WebUI contract.
/// 把单个上游技能包配置校验状态投影为 WebUI 契约。
///
/// `status` is the authoritative immutable LuaSkills validation snapshot.
/// `status` 是权威的 LuaSkills 不可变校验快照。
///
/// Returns the complete host validation result.
/// 返回完整宿主校验结果。
fn project_luaskill_config_validation(
    status: luaskills::RuntimeSkillPackageConfigStatus,
) -> LuaSkillConfigValidation {
    LuaSkillConfigValidation {
        skill_id: status.skill_id,
        complete: status.complete,
        revision: status.revision,
        store_scope: status.store_scope,
        missing: status
            .missing
            .into_iter()
            .map(|issue| LuaSkillConfigIssue {
                key: Some(issue.key),
                code: issue.code,
                message: issue.message,
            })
            .collect(),
        invalid: status
            .invalid
            .into_iter()
            .map(|issue| LuaSkillConfigIssue {
                key: Some(issue.key),
                code: issue.code,
                message: issue.message,
            })
            .collect(),
        business_issues: status
            .business_issues
            .into_iter()
            .map(|issue| LuaSkillConfigIssue {
                key: issue.key,
                code: issue.code,
                message: issue.message,
            })
            .collect(),
        orphaned: status.orphaned,
        orphaned_count: status.orphaned_count,
    }
}

/// Projects one upstream ordered event batch into the WebUI contract.
/// 把单个上游有序事件批次投影为 WebUI 契约。
///
/// `batch` contains no persisted configuration values.
/// `batch` 不包含任何持久化配置值。
///
/// Returns the complete event batch and cursor.
/// 返回完整事件批次与游标。
fn project_luaskill_config_event_batch(
    batch: luaskills::RuntimeSkillConfigEventBatch,
) -> LuaSkillConfigEventBatch {
    LuaSkillConfigEventBatch {
        events: batch
            .events
            .into_iter()
            .map(|event| LuaSkillConfigEvent {
                sequence: event.sequence,
                event_type: event.event_type,
                store_scope: event.store_scope,
                skill_id: event.skill_id,
                revision: event.revision,
                changed_keys: event.changed_keys,
                source: event.source,
                restart_required_keys: event.restart_required_keys,
                complete: event.complete,
                error: event.error.map(|error| LuaSkillConfigIssue {
                    key: None,
                    code: error.code,
                    message: error.message,
                }),
            })
            .collect(),
        next_sequence: batch.next_sequence,
    }
}

/// Inspects one local user plugin package and returns its complete authorization preview.
/// 检查一个本地用户插件包并返回其完整授权预览。
pub(crate) async fn service_inspect_local_plugin(
    state: SharedServiceBridgeState,
    source_path: String,
) -> Result<vulcan_plugin_runtime::VulcanPluginPermissionPreview, String> {
    // source_path is the exact directory selected by the user and is canonicalized by the runtime.
    // source_path 是用户选择的精确目录，并由运行时执行规范化。
    let source_path = PathBuf::from(source_path);
    let runtime = service_vulcan_plugin_runtime(&state)?;
    run_extension_management_blocking("inspect local plugin", move || {
        runtime
            .inspect_local_user_plugin(&source_path)
            .map_err(|error| error.to_string())
    })
    .await
}

/// Returns the authorization preview of one installed plugin, for comparison with an update candidate.
/// 返回单个已安装插件的授权预览，用于与更新候选比较。
pub(crate) async fn service_installed_plugin_permission_preview(
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<vulcan_plugin_runtime::VulcanPluginPermissionPreview, String> {
    let runtime = service_vulcan_plugin_runtime(&state)?;
    run_extension_management_blocking("read installed plugin permissions", move || {
        runtime
            .permission_preview(&plugin_id)
            .map_err(|error| error.to_string())
    })
    .await
}

/// Validates installation Secret inputs against the exact inspected local-MCP requirement set.
/// 根据已检查的精确本地 MCP 要求集合校验安装 Secret 输入。
///
/// # Parameters
/// 参数
/// `preview` owns the host-derived allowlist and `inputs` contains user-supplied values.
/// `preview` 持有宿主派生白名单，`inputs` 包含用户提供的值。
///
/// # Returns
/// 返回值
/// Returns a deterministic map containing every requirement exactly once and no undeclared entry.
/// 返回一份确定性映射，其中每项要求恰好出现一次且不含未声明条目。
fn validate_initial_plugin_mcp_secrets(
    preview: &vulcan_plugin_runtime::VulcanPluginPermissionPreview,
    inputs: Vec<VulcanPluginInitialSecretInput>,
) -> Result<BTreeMap<(String, String), String>, String> {
    // required_keys is the complete host-derived local MCP installation allowlist.
    // required_keys 是宿主派生的完整本地 MCP 安装白名单。
    let required_keys = preview
        .install_secret_requirements
        .iter()
        .map(|requirement| {
            (
                requirement.server_id.clone(),
                requirement.secret_key.clone(),
            )
        })
        .collect::<BTreeSet<_>>();
    // values rejects duplicate inputs before any credential-store mutation occurs.
    // values 在凭据库发生任何变更前拒绝重复输入。
    let mut values = BTreeMap::new();
    for input in inputs {
        let key = (input.server_id, input.secret_key);
        if !required_keys.contains(&key) {
            return Err(format!(
                "The install request of plugin `{}` contains undeclared local MCP secret `{}:{}`.",
                preview.plugin_id, key.0, key.1
            ));
        }
        if input.value.is_empty() {
            return Err(format!(
                "Plugin `{}` has an empty local MCP secret `{}:{}`.",
                preview.plugin_id, key.0, key.1
            ));
        }
        if values.insert(key.clone(), input.value).is_some() {
            return Err(format!(
                "Plugin `{}` submitted local MCP secret `{}:{}` more than once.",
                preview.plugin_id, key.0, key.1
            ));
        }
    }
    let submitted_keys = values.keys().cloned().collect::<BTreeSet<_>>();
    if submitted_keys != required_keys {
        let missing = required_keys
            .difference(&submitted_keys)
            .map(|(server_id, secret_key)| format!("{server_id}:{secret_key}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Plugin `{}` is missing the secrets required for local MCP install verification: {missing}.",
            preview.plugin_id
        ));
    }
    Ok(values)
}

/// Installs one explicitly authorized local user plugin and leaves it disabled for review.
/// 安装一个经过明确授权的本地用户插件，并使其保持停用以供复核。
///
/// # Parameters
/// 参数
/// `app` publishes the resulting catalog, `state` owns service runtime state, `source_path` selects the package,
/// `expected_content_fingerprint` binds the user's inspection, `approved_permissions` and
/// `allow_registry_dependencies` record exact consent, and `initial_secrets` supplies required local MCP credentials.
/// `app` 发布结果目录，`state` 持有服务运行时状态，`source_path` 选择插件包，
/// `expected_content_fingerprint` 绑定用户检查结果，`approved_permissions` 与
/// `allow_registry_dependencies` 记录精确授权，`initial_secrets` 提供必需的本地 MCP 凭据。
///
/// # Returns
/// 返回值
/// Returns the revisioned catalog update only after installation, validation, lifecycle, and publication succeed.
/// 仅在安装、校验、生命周期与发布全部成功后返回带版本的目录更新。
pub(crate) async fn service_install_local_plugin(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    source_path: String,
    expected_content_fingerprint: String,
    approved_permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("install local plugin", move || async move {
        local_plugin_update::install(
            &app,
            &state,
            source_path,
            expected_content_fingerprint,
            approved_permissions,
            allow_registry_dependencies,
            initial_secrets,
        )
        .await
    })
    .await
}

/// Updates one disabled user plugin through candidate validation and one atomic generation publication.
/// 通过候选校验与一次原子代次发布更新单个已停用用户插件。
///
/// # Parameters
/// 参数
/// `app` publishes the resulting catalog, `state` owns service runtime state, `source_path` selects the candidate,
/// `expected_content_fingerprint` binds the user's inspection, `approved_permissions` and
/// `allow_registry_dependencies` record exact consent, and `initial_secrets` supplies candidate MCP credentials.
/// `app` 发布结果目录，`state` 持有服务运行时状态，`source_path` 选择候选包，
/// `expected_content_fingerprint` 绑定用户检查结果，`approved_permissions` 与
/// `allow_registry_dependencies` 记录精确授权，`initial_secrets` 提供候选 MCP 凭据。
///
/// # Returns
/// 返回值
/// Returns the revisioned catalog update only after candidate validation and atomic generation publication succeed.
/// 仅在候选校验与原子代次发布全部成功后返回带版本的目录更新。
pub(crate) async fn service_update_local_plugin(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    source_path: String,
    expected_content_fingerprint: String,
    approved_permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("update local plugin", move || async move {
        local_plugin_update::update(
            &app,
            &state,
            source_path,
            expected_content_fingerprint,
            approved_permissions,
            allow_registry_dependencies,
            initial_secrets,
        )
        .await
    })
    .await
}

/// Read sanitized original update records of every namespace without starting work or creating a store.
/// 读取所有命名空间的脱敏原更新记录，不启动工作或创建存储。
/// State selects the configured runtime; returns durable status rows including finished history.
/// state 选择配置运行时；返回持久状态行，包含已结束历史。
pub(crate) async fn local_plugin_update_records(
    state: SharedServiceBridgeState,
) -> Result<Vec<LocalPluginUpdateStatus>, String> {
    // Read-only status must remain available while the mutation owner is busy.
    // 变更所有者忙碌时，只读状态必须仍可用。
    let runtime = service_vulcan_plugin_runtime(&state)?;
    // Read-only query snapshots the actual registry without applying mutation shutdown admission.
    // 只读查询快照实际注册表，不应用变更关闭准入。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_updates,
    );
    let mut rows =
        run_extension_management_blocking("inspect original plugin updates", move || {
            local_plugin_update::list_all_owned(&runtime, &registry)
        })
        .await?;
    // Read the actual strong owner map without joining or waiting for its mutation executor.
    // 不联接或等待变更执行器，读取实际强所有者映射。
    local_plugin_update::project_service_owners(&state, &mut rows)?;
    Ok(rows)
}

/// Resume only an explicitly named already-published update through the service-owned management task.
/// 仅通过服务自有管理任务恢复明确命名的已发布更新。
/// App/state own publication and plugin_id/request_id identify the original request; returns its catalog projection.
/// app/state 拥有发布，plugin_id/request_id 标识原请求；返回其目录投影。
pub(crate) async fn service_resume_local_plugin_update(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    plugin_id: String,
    request_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("resume published plugin update", move || async move {
        local_plugin_update::resume_published(&app, &state, &plugin_id, &request_id).await
    })
    .await
}

/// Abandon an explicit unpublished request while preserving user data and external-effect history.
/// 放弃明确的尚未发布请求，同时保留用户数据及外部副作用历史。
/// State and identities select the original owner; acknowledgement is required when installation effects were attempted.
/// state 及身份选择原所有者；已尝试安装副作用时要求明确确认。
/// Returns retained status rather than a claim that external effects were rolled back.
/// 返回保留状态，而非声称外部副作用已回滚。
pub(crate) async fn service_abort_local_plugin_update(
    state: SharedServiceBridgeState,
    plugin_id: String,
    request_id: String,
    acknowledge_external_effects: bool,
) -> Result<LocalPluginUpdateStatus, String> {
    run_luaskills_management_owned("abandon original plugin update", move || async move {
        // Original live ownership takes precedence over disk recovery; the global management fence is held once.
        // 原存活所有权优先于磁盘恢复；全局管理屏障仅持有一次。
        local_plugin_update::abort_owned(
            &state,
            &plugin_id,
            &request_id,
            acknowledge_external_effects,
        )
        .await
    })
    .await
}

/// Uninstall through one retained actual original; app/state publish and plugin_id selects its namespace.
/// 通过单个保留实际原实例卸载；app／state 负责发布，plugin_id 选择其命名空间。
/// Returns the catalog only after real callback retirement, Full removal and physical collection.
/// 仅在真实回调退役、完整移除及物理回收之后返回目录。
pub(crate) async fn service_uninstall_user_plugin(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("uninstall local plugin", move || async move {
        local_plugin_uninstall::uninstall(&app, &state, plugin_id).await
    })
    .await
}

/// Read every actual retained removal without mutation admission; state selects the live registry.
/// 读取每个实际保留移除，不取得变更准入；state 选择存活注册表。
/// Returns sanitized live ownership, including executing owners, while shutdown is closed.
/// 关闭期间返回脱敏实时所有权，包含执行中所有者。
pub(crate) fn local_plugin_uninstall_records(
    state: SharedServiceBridgeState,
) -> Result<Vec<local_plugin_uninstall::LocalPluginUninstallStatus>, String> {
    // Release service exclusion before trying resource borrows for read-only projection.
    // 尝试只读投影资源借用之前释放服务互斥。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_uninstalls,
    );
    registry.statuses()
}

/// Continue the exact live original; app/state publish, identities locate ownership and acknowledgement accepts retired failure.
/// 继续精确存活原实例；app／state 负责发布，身份定位所有权，确认接受已退役失败。
/// Returns the usual catalog response without reconstructing files or replaying callbacks.
/// 返回普通目录响应，不重建文件或重放回调。
pub(crate) async fn service_resume_local_plugin_uninstall(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    plugin_id: String,
    request_id: String,
    acknowledge_external_effects: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("continue original plugin removal", move || async move {
        local_plugin_uninstall::resume(
            &app,
            &state,
            &plugin_id,
            &request_id,
            acknowledge_external_effects,
        )
        .await
    })
    .await
}

/// Saves Vulcan-owned LuaSkills startup settings and refreshes the WebUI projection.
/// 保存 Vulcan 自有 LuaSkills 启动设置并刷新 WebUI 投影。
///
/// # Parameters
/// 参数
/// `state` is the shared service state and `settings` is the complete next settings document.
/// `state` 是共享服务状态，`settings` 是完整的下一份设置文档。
///
/// # Returns
/// 返回值
/// Returns the committed LuaSkills catalog including the restart-required comparison.
/// 返回已提交的 LuaSkills 目录，其中包含是否需要重启的比较结果。
pub(crate) async fn service_save_luaskills_runtime_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    settings: LuaSkillsRuntimeSettings,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Serialize package settings writes with every LuaSkills lifecycle mutation.
    // 将包设置写入与所有 LuaSkills 生命周期变更串行化。
    run_luaskills_management_owned("save runtime settings", move || async move {
        let handle = service_luaskills_handle(&state)?;
        // AI tool switches are copied before the complete settings document moves into the blocking save.
        // AI 工具开关在完整设置文档移入阻塞保存前完成复制。
        let runlua_enabled = settings.ai_tools.enable_run_lua;
        let runlua_session_enabled = settings.ai_tools.enable_run_lua_session;
        let catalog = run_extension_management_blocking("save runtime settings", move || {
            vulcan_service_runtime::luaskills_bridge::save_luaskills_runtime_settings(
                &handle, &settings,
            )
        })
        .await?;
        // agent_luaskills_runtime is cloned without holding service state across asynchronous lease cleanup.
        // agent_luaskills_runtime 在异步租约清理期间不持有服务状态锁的前提下完成克隆。
        let agent_luaskills_runtime = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            guard
                .agent_runtime
                .as_ref()
                .and_then(|runtime| runtime.luaskills_runtime.clone())
        };
        if let Some(agent_luaskills_runtime) = agent_luaskills_runtime {
            vulcan_agent_runtime::luaskills_runtime::set_runlua_ai_tools_enabled(
                agent_luaskills_runtime,
                runlua_enabled,
                runlua_session_enabled,
            )
            .await?;
        }
        store_luaskills_catalog(&state, catalog)?;
        refresh_luaskills_local_state_and_emit(&app, &state).await
    })
    .await
}

/// Retries the packaged managed Python and Node bootstrap and reprojects its exact outcome.
/// 重试随包受管 Python 与 Node 初始化并重新投影其精确结果。
///
/// # Parameters
/// 参数
/// `state` owns the retained LuaSkills handle and the WebUI workbench projection.
/// `state` 持有保留的 LuaSkills 句柄与 WebUI 工作台投影。
/// `target` is the exact Python or Node.js runtime group requested by the user.
/// `target` 是用户请求的精确 Python 或 Node.js 运行时组。
///
/// # Returns
/// 返回值
/// Returns the committed LuaSkills catalog even when optional managed runtimes remain unavailable.
/// 即使可选受管运行时仍不可用，也返回已提交的 LuaSkills 目录。
pub(crate) async fn service_retry_luaskills_managed_runtimes(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    target: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Serialize downloads with every LuaSkills lifecycle mutation so one shared root has one writer.
    // 将下载与所有 LuaSkills 生命周期变更串行化，使共享根始终只有一个写入者。
    run_luaskills_management_owned("retry managed runtimes", move || async move {
        // handle preserves the startup-resolved roots and the shared bootstrap report.
        // handle 保留启动时解析的目录与共享初始化报告。
        let handle = service_luaskills_handle(&state)?;
        // install_target rejects unknown wire values before any filesystem mutation begins.
        // install_target 在任何文件系统变更开始前拒绝未知传输值。
        let install_target =
        vulcan_service_runtime::managed_runtime_bootstrap::ManagedRuntimeInstallTarget::from_key(
            &target,
        )?;
        // progress_state and progress_app publish native byte progress without waiting for the blocking install to finish.
        // progress_state 与 progress_app 会立即发布原生字节进度，无需等待阻塞安装结束。
        let progress_state = state.clone();
        let progress_app = app.clone();
        // catalog contains the committed result even when the optional runtimes remain unavailable.
        // catalog 即使在可选运行时仍不可用时也包含已提交结果。
        let catalog = match run_extension_management_blocking("retry managed runtimes", move || {
            vulcan_service_runtime::luaskills_bridge::retry_managed_runtimes(
                &handle,
                install_target,
                |progress| {
                    // progress_summary is the exact focused domain value compared before revision allocation.
                    // progress_summary 是分配版本前用于比较的精确聚焦领域值。
                    let progress_summary = LuaSkillsManagedRuntimeProgressSummary {
                        phase: progress.phase.clone(),
                        component: progress.component.clone(),
                        message: progress.message.clone(),
                        completed_bytes: progress.completed_bytes,
                        total_bytes: progress.total_bytes,
                        overall_percent: progress.overall_percent,
                    };
                    // progress_commit deliberately ignores callback-only diagnostics because terminal error handling clears retained UI state.
                    // progress_commit 有意忽略仅回调诊断，因为终态错误处理会清理保留的界面状态。
                    let _progress_commit = commit_and_emit_luaskills_managed_runtime_progress(
                        &progress_app,
                        &progress_state,
                        Some(progress_summary),
                    );
                },
            )
        })
        .await
        {
            Ok(catalog) => catalog,
            Err(error) => {
                return Err(luaskills_retry_error_after_progress_clear(
                    &app, &state, error,
                ));
            }
        };
        // message persists the exact retry outcome after transient UI progress is gone.
        // message 在临时界面进度消失后持久记录精确重试结果。
        let message = format!(
            "managed runtime retry completed: status={} diagnostic={}",
            catalog.managed_runtime_report.status.as_key(),
            catalog.managed_runtime_report.diagnostic,
        );
        if let Err(error) = store_luaskills_catalog_with_log(&state, catalog, message) {
            return Err(luaskills_retry_error_after_progress_clear(
                &app, &state, error,
            ));
        }
        match refresh_luaskills_local_state_and_emit(&app, &state).await {
            Ok(update) => Ok(update),
            Err(error) => Err(luaskills_retry_error_after_progress_clear(
                &app, &state, error,
            )),
        }
    })
    .await
}

/// Clears transient LuaSkills progress after a retry failure while preserving the primary error.
/// 在重试失败后清理临时 LuaSkills 进度，同时保留主要错误。
///
/// # Parameters
/// 参数
/// `app` broadcasts the terminal progress clear to every connected client.
/// `app` 将终态进度清理广播给每个已连接客户端。
/// `state` owns the retained workbench progress projection.
/// `state` 持有保留的工作台进度投影。
/// `error` is the primary retry, persistence, or projection failure.
/// `error` 是主要的重试、持久化或投影失败。
///
/// # Returns
/// 返回值
/// The primary error, augmented only when committing the terminal clear also fails.
/// 返回主要错误；仅当提交终态清理也失败时追加诊断。
fn luaskills_retry_error_after_progress_clear(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    error: String,
) -> String {
    match commit_and_emit_luaskills_managed_runtime_progress(app, state, None) {
        Ok(_) => error,
        Err(clear_error) => {
            format!("{error}; additionally failed to clear managed runtime progress: {clear_error}")
        }
    }
}

/// Returns the retained LuaSkills operation progress for one correlation id.
/// 返回某个关联标识保留的 LuaSkills 操作进度。
///
/// This is the production service entry for querying one exact retained operation by
/// the stable id its caller already holds, instead of re-deriving state from catalog
/// contents or timestamps. The returned summary is the same service API type the
/// catalog and the delta carry, so all three expose one contract.
/// 这是按调用方已持有的稳定标识查询某个精确保留操作的生产服务入口，而不是从目录内容或
/// 时间戳重新推断状态。返回的摘要是目录与增量所携带的同一个 service API 类型，因此三者只
/// 暴露一份契约。
///
/// No WebUI caller exists today: a client that reconnects reads the latest operation from
/// the catalog snapshot, which already carries it. This entry stays reachable for any
/// client that holds a specific id, including one whose operation is no longer the latest.
/// 当前没有 WebUI 调用方：重连的客户端从目录快照读取最新操作，而该快照本就携带它。本入口
/// 对任何持有一个具体标识的调用方保持可达，包括其操作已不是最新一次的情形。
pub(crate) fn luaskill_operation_progress(
    correlation_id: &str,
) -> Result<vulcan_service_api::LuaSkillsOperationProgressSummary, String> {
    crate::luaskills_install_progress::global_operation_progress_view(correlation_id).ok_or_else(
        || {
            format!(
                "No retained LuaSkills operation progress for correlation id `{correlation_id}`."
            )
        },
    )
}

/// Finalizes one LuaSkills management operation, publishes it, and diagnoses it.
/// 结束一次 LuaSkills 管理操作、发布它并输出诊断。
///
/// The request result is authoritative for the outcome; the upstream event stream
/// only supplies phase progress. The diagnostic is deliberately aggregate: it
/// reports counts and the latest phase instead of raw event payloads, so no
/// source locator or local directory can reach a log line.
/// 请求结果是结果的权威；上游事件流只提供阶段进度。诊断刻意保持聚合：只报告计数与最新
/// 阶段，而不是原始事件载荷，因此任何来源定位值或本地目录都不会进入日志。
fn finalize_luaskills_operation_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    correlation_id: Option<&str>,
    mut result: Result<LuaSkillsCatalogUpdate, String>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    use crate::luaskills_install_progress::{
        LuaSkillsOperationOutcome, finish_global_operation, global_foreign_event_count,
        global_operation_progress, global_orphaned_event_count,
    };

    let Some(correlation_id) = correlation_id else {
        return result;
    };
    let outcome = if result.is_ok() {
        LuaSkillsOperationOutcome::Succeeded
    } else {
        LuaSkillsOperationOutcome::Failed
    };
    finish_global_operation(correlation_id, outcome);
    // terminal_progress is read by exact identity so a later operation can never replace
    // this request's result between finalization and retained-state publication.
    // terminal_progress 按精确身份读取，确保后续操作不会在终结与保留状态发布之间替换本请求结果。
    let terminal_progress =
        crate::luaskills_install_progress::global_operation_progress_view(correlation_id);
    // terminal_generation commits the terminal value before the RPC response returns.
    // This prevents a reconnect snapshot or the complete RPC payload from regressing a
    // newer focused terminal event back to the pre-finalization in-flight value.
    // terminal_generation 在 RPC 返回前提交终态值，防止重连快照或完整 RPC 载荷把较新的聚焦
    // 终态事件回退为终结前的进行中值。
    match crate::settings_runtime_projection::commit_and_emit_luaskills_operation_progress(
        app,
        state,
        terminal_progress.clone(),
    ) {
        Ok(Some(generation)) => {
            if let Ok(update) = result.as_mut() {
                update.revision = generation;
                update.lua_skills.operation_progress = terminal_progress;
            }
        }
        Ok(None) => {
            if let Ok(update) = result.as_mut() {
                update.lua_skills.operation_progress = terminal_progress;
            }
        }
        Err(publication_error) => {
            result = Err(match result {
                Ok(_) => format!(
                    "LuaSkills operation completed but terminal progress publication failed: {publication_error}"
                ),
                Err(primary_error) => format!(
                    "{primary_error}; additionally failed to publish terminal LuaSkills progress: {publication_error}"
                ),
            });
        }
    }
    let Some(progress) = global_operation_progress(correlation_id) else {
        return result;
    };
    // `foreign` is a defect signal, not routine noise: a non-zero value means an
    // upstream event source exists that the management lock did not serialize.
    // `foreign` 是缺陷信号而非日常噪声：非零值说明存在管理锁未能串行化的上游事件来源。
    tracing::info!(
        "[webui-service] luaskills {} {} outcome={:?} phases={} latest={} upstream_terminal={} \
         overflowed={} orphaned={} foreign={}",
        crate::luaskills_install_progress::action_label(progress.action),
        correlation_id,
        outcome,
        progress.events.len(),
        progress
            .latest()
            .map(|event| event.phase.as_str())
            .unwrap_or("none"),
        progress.saw_upstream_terminal,
        progress.overflowed,
        global_orphaned_event_count(),
        global_foreign_event_count(),
    );
    result
}

/// Installs one LuaSkill package into the USER layer and refreshes the WebUI snapshot.
/// 将单个 LuaSkill 包安装到 USER 层并刷新 WebUI 快照。
pub(crate) async fn service_install_luaskill(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    skill_id: Option<String>,
    source: String,
    source_type: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("install skill", move || async move {
        // The correlation id is generated only after the service-owned task acquires
        // the lock, so no queued or cancelled waiter can become the active binding.
        // 关联标识只在服务自有任务取得锁之后生成，因此排队或取消的等待者不会成为活动绑定。
        let correlation_id = crate::luaskills_install_progress::begin_global_operation(
            luaskills::SkillLifecycleAction::Install,
        );
        let outcome = async {
            let handle = service_luaskills_handle(&state)?;
            let (message, catalog) =
                run_extension_management_blocking("install skill", move || {
                    vulcan_service_runtime::luaskills_bridge::install_luaskill(
                        &handle,
                        skill_id,
                        source,
                        source_type,
                    )
                })
                .await?;
            store_luaskills_catalog_with_log(&state, catalog, message)?;
            refresh_luaskills_local_state_and_emit(&app, &state).await
        }
        .await;
        finalize_luaskills_operation_progress(&app, &state, correlation_id.as_deref(), outcome)
    })
    .await
}

/// Searches the LuaSkills Hub that `official_hub` installs resolve against.
/// 搜索 `official_hub` 安装所解析的 LuaSkills Hub。
///
/// The search is a read-only network call, so it does not take the LuaSkills
/// management lock and never changes the installed catalog.
/// 搜索是只读网络调用，因此不获取 LuaSkills 管理锁，也绝不改变已安装目录。
pub(crate) async fn service_search_luaskills_hub(
    state: SharedServiceBridgeState,
    query: Option<String>,
    cursor: Option<String>,
    locale: Option<String>,
) -> Result<vulcan_service_runtime::luaskills_hub::LuaSkillsHubSearchPage, String> {
    let handle = service_luaskills_handle(&state)?;
    tokio::task::spawn_blocking(move || {
        vulcan_service_runtime::luaskills_hub::search_official_hub(
            &handle,
            query.as_deref().unwrap_or_default(),
            cursor.as_deref(),
            locale.as_deref(),
        )
    })
    .await
    .map_err(|error| format!("LuaSkills Hub search task failed: {error}"))?
}

/// Reads one skill's detail, README included, from the same Hub as the search.
/// 从与搜索相同的 Hub 读取一个技能的详情（含 README）。
///
/// `locale` is the client's interface language; the Hub returns its translation when one exists.
/// `locale` 是客户端界面语言；存在译文时 Hub 返回译文。
pub(crate) async fn service_get_luaskills_hub_skill(
    state: SharedServiceBridgeState,
    skill_id: String,
    locale: Option<String>,
) -> Result<vulcan_service_runtime::luaskills_hub::LuaSkillsHubSkillDetail, String> {
    let handle = service_luaskills_handle(&state)?;
    tokio::task::spawn_blocking(move || {
        vulcan_service_runtime::luaskills_hub::get_official_hub_skill(
            &handle,
            &skill_id,
            locale.as_deref(),
        )
    })
    .await
    .map_err(|error| format!("LuaSkills Hub skill detail task failed: {error}"))?
}

/// Updates one managed LuaSkill package and refreshes the WebUI snapshot.
/// 更新单个受管 LuaSkill 包并刷新 WebUI 快照。
///
/// `source_type` and `source` together move the package to a new source; both or neither are given.
/// `source_type` 与 `source` 同时给出时把技能包迁移到新来源；二者须同时给出或同时省略。
pub(crate) async fn service_update_luaskill(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    skill_id: String,
    source_type: Option<String>,
    source: Option<String>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    let replacement = match (source_type, source) {
        (Some(source_type), Some(source)) => Some((source_type, source)),
        (None, None) => None,
        _ => {
            return Err(String::from(
                "Changing a skill source needs both sourceType and source.",
            ));
        }
    };
    run_luaskills_management_owned("update skill", move || async move {
        let correlation_id = crate::luaskills_install_progress::begin_global_operation(
            luaskills::SkillLifecycleAction::Update,
        );
        let outcome = async {
            let handle = service_luaskills_handle(&state)?;
            let (message, catalog) = run_extension_management_blocking("update skill", move || {
                vulcan_service_runtime::luaskills_bridge::update_luaskill(
                    &handle,
                    skill_id,
                    replacement,
                )
            })
            .await?;
            store_luaskills_catalog_with_log(&state, catalog, message)?;
            refresh_luaskills_local_state_and_emit(&app, &state).await
        }
        .await;
        finalize_luaskills_operation_progress(&app, &state, correlation_id.as_deref(), outcome)
    })
    .await
}

/// Checks managed LuaSkill packages for newer versions without changing anything.
/// 检查受管 LuaSkill 包是否有新版本，不做任何修改。
///
/// The check only reads source metadata, so it does not take the management lock; it waits
/// only while a running install or update holds the engine.
/// 检查只读取来源元数据，因此不获取管理锁；只在进行中的安装或更新占用引擎时等待。
pub(crate) async fn service_check_luaskill_updates(
    state: SharedServiceBridgeState,
    skill_ids: Option<Vec<String>>,
) -> Result<Vec<vulcan_service_runtime::luaskills_bridge::TauriLuaSkillUpdateCheck>, String> {
    let handle = service_luaskills_handle(&state)?;
    tokio::task::spawn_blocking(move || {
        vulcan_service_runtime::luaskills_bridge::check_luaskill_updates(&handle, skill_ids)
    })
    .await
    .map_err(|error| format!("LuaSkills update check task failed: {error}"))?
}

/// Returns the built-in GitHub mirror catalog shipped with LuaSkills.
/// 返回 LuaSkills 随附的内置 GitHub 镜像目录。
pub(crate) fn service_luaskills_github_mirrors()
-> Vec<vulcan_service_runtime::luaskills_network::LuaSkillsGithubMirror> {
    vulcan_service_runtime::luaskills_network::github_mirror_catalog()
}

/// Probes GitHub mirrors in parallel; the result only measures and never changes settings.
/// 并行测速 GitHub 镜像；结果只做测量，绝不修改设置。
pub(crate) async fn service_probe_luaskills_github_mirrors(
    targets: Vec<vulcan_service_runtime::luaskills_network::LuaSkillsGithubMirrorProbeTarget>,
) -> Result<Vec<vulcan_service_runtime::luaskills_network::LuaSkillsGithubMirrorProbeResult>, String>
{
    tokio::task::spawn_blocking(move || {
        vulcan_service_runtime::luaskills_network::probe_mirrors(targets)
    })
    .await
    .map_err(|error| format!("GitHub mirror probe task failed: {error}"))?
}

/// Saves or clears the LuaSkills GitHub token and applies it to the running engine.
/// 保存或清除 LuaSkills GitHub 令牌，并将其应用到运行中引擎。
///
/// `token` is `None` to clear. The value goes to the OS credential store and engine memory
/// only; the returned catalog reports just whether a token is configured.
/// `token` 为 `None` 时清除。令牌只进入操作系统凭据库与引擎内存；返回的目录只报告是否已配置。
pub(crate) async fn service_set_luaskills_github_token(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    token: Option<String>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("set GitHub token", move || async move {
        let handle = service_luaskills_handle(&state)?;
        let catalog = run_extension_management_blocking("set GitHub token", move || {
            let stored = match token {
                Some(token) => {
                    Some(crate::luaskills_github_token_store::store_luaskills_github_token(&token)?)
                }
                None => {
                    crate::luaskills_github_token_store::delete_luaskills_github_token()?;
                    None
                }
            };
            vulcan_service_runtime::luaskills_bridge::set_luaskills_github_token(&handle, stored)
        })
        .await?;
        store_luaskills_catalog(&state, catalog)?;
        refresh_luaskills_local_state_and_emit(&app, &state).await
    })
    .await
}

/// Enables or disables one LuaSkill package and refreshes the WebUI snapshot.
/// 启用或停用单个 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_set_luaskill_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    skill_id: String,
    enabled: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep lifecycle state mutation and its returned catalog projection in one ordered transaction.
    // 将生命周期状态变更及其返回的目录投影保持在同一个有序事务中。
    run_luaskills_management_owned("set skill enabled", move || async move {
        let handle = service_luaskills_handle(&state)?;
        let (message, catalog) =
            run_extension_management_blocking("set skill enabled", move || {
                vulcan_service_runtime::luaskills_bridge::set_luaskill_enabled(
                    &handle, skill_id, enabled,
                )
            })
            .await?;
        store_luaskills_catalog_with_log(&state, catalog, message)?;
        refresh_luaskills_local_state_and_emit(&app, &state).await
    })
    .await
}

/// Enables or disables one known host-owned Vulcan Plugin.
/// 启用或停用单个已知的宿主自有 Vulcan 插件。
///
/// # Parameters
/// 参数
/// `state` retains service configuration, `plugin_id` identifies the plugin, and `enabled` is the requested state.
/// `state` 保留服务配置，`plugin_id` 标识插件，`enabled` 是请求状态。
///
/// # Returns
/// 返回值
/// A revisioned LuaSkills domain projection after persistence succeeds.
/// 持久化成功后的带版本 LuaSkills 领域投影。
pub(crate) async fn service_set_vulcan_plugin_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    plugin_id: String,
    enabled: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    run_luaskills_management_owned("set plugin enabled", move || async move {
        // Refuse before discovery, callbacks or policy changes can invalidate a retained first installation.
        // 在发现、回调或策略变更能够使保留首安装失效之前拒绝。
        {
            // Refuse catalog-changing enable/disable before entering any original management work.
            // 在进入任何原管理工作之前拒绝改变目录的启用／停用。
            let guard = state.lock().map_err(|error| error.to_string())?;
            crate::service_state_access::require_plugin_update_full_publication_admission(
                &guard, None,
            )?;
        }
        // update is emitted here so the lifecycle module remains a pure transaction without a second publisher.
        // update 在此发送，使生命周期模块保持为不包含第二发布出口的纯事务。
        let update =
            vulcan_plugin_lifecycle::set_vulcan_plugin_enabled(state, plugin_id, enabled).await?;
        emit_plugin_catalog_delta(&app, &update);
        Ok(update.lua_skills_update())
    })
    .await
}

/// Resets one plugin's stored configuration to its current package defaults.
/// 将单个插件的已存配置重置为其当前插件包默认值。
///
/// # Parameters
/// 参数
/// `app` broadcasts the refreshed catalog, `state` owns the plugin runtime and `plugin_id` selects the
/// plugin whose settings are replaced.
/// `app` 广播刷新后的目录，`state` 持有插件运行时，`plugin_id` 选择其设置被替换的插件。
///
/// # Returns
/// 返回值
/// Returns the refreshed catalog update; an unfinished package transaction refuses the reset instead of waiting.
/// 返回刷新后的目录更新；未完成的插件包交易会拒绝重置而不等待。
pub(crate) async fn service_reset_vulcan_plugin_config(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // runtime owns the settings section and republishes the edited plugin itself.
    // runtime 持有设置段，并自行重新发布被编辑的插件。
    let runtime = service_vulcan_plugin_runtime(&state)?;
    run_extension_management_blocking("reset plugin configuration", move || {
        runtime
            .reset_plugin_config(&plugin_id)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await?;
    refresh_plugin_catalog_local_state_and_emit(&app, &state).await
}

/// Reads the exact current plugin environment readiness without installing dependencies.
/// 只读检查当前精确插件环境的就绪状态，不安装依赖。
///
/// # Parameters
/// 参数
/// `state` owns the plugin catalog and `plugin_id` selects its original identity.
/// `state` 持有插件目录，`plugin_id` 选择其中的原始身份。
///
/// # Returns
/// 返回值
/// Returns true only when the current generation can execute from its prepared environment.
/// 仅当当前代次可从已准备环境执行时返回真。
pub(crate) async fn service_plugin_environment_ready(
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<bool, String> {
    // runtime is the sole authority for exact package generation and environment roots.
    // runtime 是精确插件包代次及环境根目录的唯一权威。
    let runtime = service_vulcan_plugin_runtime(&state)?;
    run_extension_management_blocking("inspect plugin environment", move || {
        runtime
            .plugin_environment_ready(&plugin_id)
            .map_err(|error| error.to_string())
    })
    .await
}

/// Explicitly installs the current disabled plugin environment before enabling the plugin.
/// 在启用插件前显式安装当前已停用插件的环境。
///
/// # Parameters
/// 参数
/// `state` owns the plugin catalog and `plugin_id` selects its original identity.
/// `state` 持有插件目录，`plugin_id` 选择其中的原始身份。
///
/// # Returns
/// 返回值
/// Returns true after the exact generation is prepared and independently checked.
/// 在精确代次准备完成并经过独立检查后返回真。
pub(crate) async fn service_prepare_plugin_environment(
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<bool, String> {
    run_luaskills_management_owned("prepare plugin environment", move || async move {
        // runtime and plugin are resolved while the management lock excludes lifecycle mutations.
        // runtime 和 plugin 在管理锁排除生命周期变更期间解析。
        let runtime = service_vulcan_plugin_runtime(&state)?;
        // snapshot fixes the package generation and current desired state for this install.
        // snapshot 固定本次安装的插件包代次和当前期望状态。
        let snapshot = runtime.snapshot();
        // plugin is the exact current catalog descriptor, never a caller-supplied path.
        // plugin 是当前目录的精确描述符，绝不是调用方提供的路径。
        let plugin = snapshot
            .plugins
            .get(&plugin_id)
            .ok_or_else(|| format!("Vulcan plugin not found: {plugin_id}"))?;
        if plugin.desired_enabled
            || plugin.runtime_state != vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
        {
            return Err(format!(
                "Vulcan plugin `{plugin_id}` must be disabled before its execution environment can be installed."
            ));
        }
        if !matches!(
            plugin.availability,
            vulcan_plugin_runtime::VulcanPluginAvailability::Enabled
                | vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
        ) {
            return Err(format!(
                "Vulcan plugin `{plugin_id}` cannot be installed right now: {}",
                plugin.diagnostics.join("; ")
            ));
        }
        run_extension_management_blocking("install plugin environment", move || {
            runtime
                .prepare_plugin_environment(&plugin_id)
                .map_err(|error| error.to_string())?;
            runtime
                .plugin_environment_ready(&plugin_id)
                .map_err(|error| error.to_string())
        })
        .await
    })
    .await
}

/// Starts one detached, deduplicated plugin environment installation task.
/// 启动一个分离运行且按插件目录代次去重的插件环境安装任务。
///
/// # Parameters
/// 参数
/// `state` owns the Core task registry and the validated plugin runtime.
/// `state` 持有 Core 任务登记表及已校验的插件运行时。
/// `plugin_id` selects the exact disabled catalog entry.
/// `plugin_id` 选择精确的已停用目录条目。
///
/// # Returns
/// 返回值
/// Returns the existing task status for a duplicate generation, or the newly queued status.
/// 对重复代次返回已有任务状态，否则返回新排队任务状态。
pub(crate) async fn service_start_plugin_environment_install(
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<PluginEnvironmentInstallTaskStatus, String> {
    let runtime = service_vulcan_plugin_runtime(&state)?;
    let snapshot = runtime.snapshot();
    let plugin = snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin not found: {plugin_id}"))?;
    if plugin.desired_enabled
        || plugin.runtime_state != vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
    {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` must be disabled before its execution environment can be installed."
        ));
    }
    if !matches!(
        plugin.availability,
        vulcan_plugin_runtime::VulcanPluginAvailability::Enabled
            | vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
    ) {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` cannot be installed right now: {}",
            plugin.diagnostics.join("; ")
        ));
    }
    // key fixes one request to the exact source directory generation observed at enqueue time.
    // key 将一次请求固定到排队时观察到的精确源目录代次。
    let key = PluginEnvironmentInstallKey {
        plugin_id: plugin.id.clone(),
        package_root: plugin.package_root.clone(),
        catalog_generation: plugin.generation,
        content_fingerprint: plugin.content_fingerprint.clone(),
    };
    // expected_key binds the detached worker to the exact generation accepted by this request.
    // expected_key 将分离 worker 绑定到本次请求接受的精确代次。
    let expected_key = key.clone();
    let registry = plugin_environment_install_task_registry().clone();
    let enqueue = registry.enqueue(key)?;
    if !enqueue.should_start {
        return Ok(enqueue.status);
    }
    let task_id = enqueue.status.task_id.clone();
    let task_plugin_id = plugin_id.clone();
    let task_runtime = runtime;
    let task_registry = registry;
    // Detached ownership keeps installation alive when an RPC waiter or GUI disappears.
    // 分离任务拥有安装生命周期，即使 RPC 等待者或 GUI 消失也继续执行。
    spawn_service_task(async move {
        let _ = task_registry.update(&task_id, "running", "preparing", None);
        let operation_task_id = task_id.clone();
        let operation_registry = task_registry.clone();
        let result =
            run_luaskills_management_owned("prepare plugin environment task", move || async move {
                let current = task_runtime.snapshot();
                let current_plugin = current
                    .plugins
                    .get(&task_plugin_id)
                    .ok_or_else(|| format!("Vulcan plugin not found: {task_plugin_id}"))?;
                if current_plugin.desired_enabled
                    || current_plugin.runtime_state
                        != vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
                {
                    return Err(format!(
                        "Vulcan plugin `{task_plugin_id}` must stay disabled during installation."
                    ));
                }
                let current_key = PluginEnvironmentInstallKey {
                    plugin_id: current_plugin.id.clone(),
                    package_root: current_plugin.package_root.clone(),
                    catalog_generation: current_plugin.generation,
                    content_fingerprint: current_plugin.content_fingerprint.clone(),
                };
                if current_key != expected_key {
                    return Err(format!(
                        "The catalog generation of Vulcan plugin `{task_plugin_id}` changed while queued; refresh the catalog and retry."
                    ));
                }
                let install_plugin_id = task_plugin_id.clone();
                let verify_plugin_id = task_plugin_id.clone();
                operation_registry.update(&operation_task_id, "running", "preparing", None)?;
                let prepare_runtime = task_runtime.clone();
                run_extension_management_blocking("install plugin environment task", move || {
                    prepare_runtime
                        .prepare_plugin_environment(&install_plugin_id)
                        .map_err(|error| error.to_string())
                })
                .await?;
                let prepared_key = {
                    let prepared_snapshot = task_runtime.snapshot();
                    let prepared_plugin = prepared_snapshot
                        .plugins
                        .get(&task_plugin_id)
                        .ok_or_else(|| format!("Vulcan plugin not found: {task_plugin_id}"))?;
                    PluginEnvironmentInstallKey {
                        plugin_id: prepared_plugin.id.clone(),
                        package_root: prepared_plugin.package_root.clone(),
                        catalog_generation: prepared_plugin.generation,
                        content_fingerprint: prepared_plugin.content_fingerprint.clone(),
                    }
                };
                if prepared_key != expected_key {
                    return Err(format!(
                        "The catalog generation of Vulcan plugin `{task_plugin_id}` changed during installation; the stale task result was rejected."
                    ));
                }
                operation_registry.update(&operation_task_id, "running", "verifying", None)?;
                let verify_runtime = task_runtime.clone();
                let verification_result = run_extension_management_blocking(
                    "verify plugin environment task",
                    move || {
                    verify_runtime
                        .plugin_environment_ready(&verify_plugin_id)
                        .map_err(|error| error.to_string())
                    },
                )
                .await?;
                let verified_key = {
                    let verified_snapshot = task_runtime.snapshot();
                    let verified_plugin = verified_snapshot
                        .plugins
                        .get(&task_plugin_id)
                        .ok_or_else(|| format!("Vulcan plugin not found: {task_plugin_id}"))?;
                    PluginEnvironmentInstallKey {
                        plugin_id: verified_plugin.id.clone(),
                        package_root: verified_plugin.package_root.clone(),
                        catalog_generation: verified_plugin.generation,
                        content_fingerprint: verified_plugin.content_fingerprint.clone(),
                    }
                };
                if verified_key != expected_key {
                    return Err(format!(
                        "The catalog generation of Vulcan plugin `{task_plugin_id}` changed during verification; the stale task result was rejected."
                    ));
                }
                Ok(verification_result)
            })
            .await;
        match result {
            Ok(true) => {
                let _ = task_registry.update(&task_id, "succeeded", "completed", None);
            }
            Ok(false) => {
                let _ = task_registry.update(
                    &task_id,
                    "failed",
                    "failed",
                    Some("The plugin execution environment failed the integrity check after installation."),
                );
            }
            Err(error) => {
                let _ = task_registry.update(&task_id, "failed", "failed", Some(&error));
            }
        }
    });
    Ok(enqueue.status)
}

/// Reads one detached plugin environment installation task without changing it.
/// 只读查询一个分离运行的插件环境安装任务，不改变任务状态。
///
/// # Parameters
/// 参数
/// `state` owns the Core task registry; `task_id` identifies the exact task.
/// `state` 持有 Core 任务登记表；`task_id` 标识精确任务。
///
/// # Returns
/// 返回值
/// Returns the retained status, or an explicit error when the task was evicted or unknown.
/// 返回保留的状态；任务被淘汰或未知时返回明确错误。
pub(crate) fn service_plugin_environment_install_status(
    _state: SharedServiceBridgeState,
    task_id: String,
) -> Result<PluginEnvironmentInstallTaskStatus, String> {
    let registry = plugin_environment_install_task_registry().clone();
    registry
        .status(&task_id)?
        .ok_or_else(|| format!("Plugin environment install task not found: {task_id}"))
}

/// Reads the latest task for the plugin's current catalog directory generation.
/// 只读查询插件当前目录代次对应的最新任务。
///
/// # Parameters
/// 参数
/// `state` owns the validated plugin runtime; `plugin_id` selects the exact catalog entry.
/// `state` 持有已校验插件运行时；`plugin_id` 选择精确目录条目。
///
/// # Returns
/// 返回值
/// Returns `None` when this generation has never been requested.
/// 当此代次从未被请求时返回 `None`。
pub(crate) fn service_plugin_environment_install_status_for_plugin(
    state: SharedServiceBridgeState,
    plugin_id: String,
) -> Result<Option<PluginEnvironmentInstallTaskStatus>, String> {
    let runtime = service_vulcan_plugin_runtime(&state)?;
    let snapshot = runtime.snapshot();
    let plugin = snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin not found: {plugin_id}"))?;
    let key = PluginEnvironmentInstallKey {
        plugin_id: plugin.id.clone(),
        package_root: plugin.package_root.clone(),
        catalog_generation: plugin.generation,
        content_fingerprint: plugin.content_fingerprint.clone(),
    };
    plugin_environment_install_task_registry().status_for_key(&key)
}

/// Uninstalls one ordinary LuaSkill package and refreshes the WebUI snapshot.
/// 卸载单个普通 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_uninstall_luaskill(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep uninstall and its returned catalog projection in one ordered transaction.
    // 将卸载及其返回的目录投影保持在同一个有序事务中。
    run_luaskills_management_owned("uninstall skill", move || async move {
        let handle = service_luaskills_handle(&state)?;
        let (message, catalog) = run_extension_management_blocking("uninstall skill", move || {
            vulcan_service_runtime::luaskills_bridge::uninstall_luaskill(&handle, skill_id)
        })
        .await?;
        store_luaskills_catalog_with_log(&state, catalog, message)?;
        refresh_luaskills_local_state_and_emit(&app, &state).await
    })
    .await
}

/// Persists one LuaSkill tool registration state and refreshes the WebUI snapshot.
/// 持久化单个 LuaSkill 工具注册状态并刷新 WebUI 快照。
pub(crate) async fn service_set_luaskill_tool_registration_state(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    skill_id: String,
    local_name: String,
    registration_state: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep tool registration ordered with lifecycle catalog mutations so every response projects one coherent domain.
    // 使工具注册与生命周期目录变更保持有序，让每个响应都投影同一个一致领域。
    run_luaskills_management_owned("set skill tool registration", move || async move {
        let next_state = parse_luaskill_tool_registration_state(&registration_state)?;
        mutate_provider_configuration(
            &state,
            "Failed to save the LuaSkills tool registration",
            |configuration| {
                configuration.luaskills.set_tool_registration_state(
                    &skill_id,
                    &local_name,
                    next_state,
                );
                Ok(())
            },
            |_guard, _configuration| (),
        )
        .await?;
        refresh_luaskills_local_state_and_emit(&app, &state).await
    })
    .await
}

/// Refreshes only the Agent Skills domain and returns its committed revision.
/// 仅刷新 Agent Skills 领域并返回其已提交版本。
pub(crate) async fn service_refresh_agent_skills_catalog(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Opens the configured user Agent Skills directory and returns the refreshed catalog.
/// 打开配置后的用户 Agent Skills 目录并返回刷新后的目录。
pub(crate) async fn service_open_agent_skills_user_directory(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let (user_root, _) = configured_agent_skill_roots(&state)?;
    let created_root = user_root.clone();
    run_agent_skill_file_operation(move || {
        std::fs::create_dir_all(&created_root)
            .map_err(|error| format!("Failed to create the Agent Skills user directory: {error}"))
    })
    .await?;
    let update = refresh_agent_skills_local_state(&state).await?;
    shell_host_from_state(&state)?.open_path(
        &vulcan_runtime_assets::native_host_path_text_from_path(&user_root),
    )?;
    emit_agent_skills_catalog_delta(&app, update.clone());
    Ok(update)
}

/// Saves one canonical user Agent Skill package and its per-skill state.
/// 保存一个规范用户 Agent Skill 技能包及其单技能状态。
pub(crate) async fn service_save_agent_skill(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: AgentSkillSaveRequest,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    if let Some(source_skill_key) = request.source_skill_key.as_deref() {
        reject_plugin_skill_mutation(&state, source_skill_key)?;
    }
    if let Some(original_name) = request.original_name.as_deref() {
        reject_plugin_skill_mutation(&state, original_name)?;
    }
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    let name = require_canonical_agent_skill_name(&request.name)?.to_string();
    let original_name = request
        .original_name
        .as_deref()
        .map(require_canonical_agent_skill_name)
        .transpose()?
        .map(str::to_string);
    let source_skill_key = request.source_skill_key.as_deref();
    // catalog captures the exact existing source record used to preserve or derive provenance.
    // catalog 捕获用于保留或推导来源的精确现有来源记录。
    let catalog = {
        let (system_root, user_root) = (system_root.clone(), user_root.clone());
        run_agent_skill_file_operation(move || {
            Ok(vulcan_prompt_skills::load_prompt_skills_from_roots(
                system_root,
                user_root,
            ))
        })
        .await?
    };
    // origin is host-assigned and cannot be forged by the WebUI payload.
    // origin 由宿主分配，WebUI 载荷无法伪造。
    let origin = managed_agent_skill_origin(&catalog, original_name.as_deref(), &name);
    // resource_source_dir is resolved from the exact source-aware catalog record selected by the user.
    // resource_source_dir 从用户选中的精确来源感知目录记录解析。
    let resource_source_dir = source_skill_key
        .map(|skill_key| {
            exact_agent_skill_source_record(&catalog, skill_key).map(|skill| skill.base_dir.clone())
        })
        .transpose()?;
    // enabled is captured before request fields move into the package document.
    // enabled 在请求字段移入技能包文档前被捕获。
    let enabled = request.enabled;
    let input = vulcan_prompt_skills::PromptSkillPackageInput {
        original_name: original_name.clone(),
        resource_source_dir,
        name: name.clone(),
        description: request.description,
        license: request.license,
        compatibility: request.compatibility,
        metadata: request.metadata,
        body: request.body,
        interface: vulcan_prompt_skills::PromptSkillInterfaceMetadata {
            display_name: request.interface.display_name,
            short_description: request.interface.short_description,
            emoji: request.interface.emoji,
            color: request.interface.color,
            // User-authored skills are single-language; only bundled system skills are localized.
            // 用户创作的技能只有单一语言；仅内置系统技能提供本地化。
            locales: std::collections::BTreeMap::new(),
        },
        catalog: vulcan_prompt_skills::PromptSkillCatalogMetadata {
            category_id: request.category_id,
            tags: request.tags,
        },
        origin,
    };
    // configuration_before captures exact states required for transactional rollback.
    // configuration_before 捕获事务回滚所需的精确状态。
    let configuration_before = provider_configuration_from_state(&state)?;
    // previous_target_state is the original target-name override.
    // previous_target_state 是原目标名称覆盖状态。
    let previous_target_state = configuration_before
        .prompt_skills
        .skill_overrides
        .get(&name)
        .copied();
    // previous_original_state is the original renamed-source override.
    // previous_original_state 是原重命名来源覆盖状态。
    let previous_original_state = original_name.as_deref().and_then(|value| {
        configuration_before
            .prompt_skills
            .skill_overrides
            .get(value)
            .copied()
    });
    let state_override = if enabled {
        PersistedPromptSkillState::Enabled
    } else {
        PersistedPromptSkillState::Disabled
    };
    let configuration_original_name = original_name.clone();
    let configuration_target_name = name.clone();
    crate::service_state_access::mutate_provider_configuration_with_state_validation(
        &state,
        "Failed to save the Agent Skill state",
        |guard| {
            crate::service_state_access::require_plugin_update_full_publication_admission(
                guard, None,
            )
        },
        move |configuration| {
            if let Some(original_name) = configuration_original_name
                .as_deref()
                .filter(|value| *value != configuration_target_name)
            {
                configuration
                    .prompt_skills
                    .set_skill_state(original_name, None);
            }
            configuration
                .prompt_skills
                .set_skill_state(&configuration_target_name, Some(state_override));
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    let package_root = user_root.clone();
    if let Err(file_error) = run_agent_skill_file_operation(move || {
        vulcan_prompt_skills::save_user_prompt_skill_package(&package_root, input)
    })
    .await
    {
        // rollback_original_name retains the exact source key restored after a file failure.
        // rollback_original_name 保留文件失败后恢复的精确来源键。
        let rollback_original_name = original_name.clone();
        // rollback_target_name retains the exact target key restored after a file failure.
        // rollback_target_name 保留文件失败后恢复的精确目标键。
        let rollback_target_name = name.clone();
        let rollback_result = mutate_provider_configuration(
            &state,
            "Failed to roll back the Agent Skill state",
            move |configuration| {
                configuration
                    .prompt_skills
                    .set_skill_state(&rollback_target_name, previous_target_state);
                if let Some(original_name) = rollback_original_name
                    .as_deref()
                    .filter(|value| *value != rollback_target_name)
                {
                    configuration
                        .prompt_skills
                        .set_skill_state(original_name, previous_original_state);
                }
                Ok(())
            },
            |_guard, _configuration| (),
        )
        .await;
        return match rollback_result {
            Ok(()) => Err(file_error),
            Err(rollback_error) => Err(format!(
                "{file_error}; Agent Skill configuration rollback also failed: {rollback_error}"
            )),
        };
    }
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Deletes one user Agent Skill package and removes its obsolete per-skill state.
/// 删除一个用户 Agent Skill 技能包并移除其过期单技能状态。
pub(crate) async fn service_delete_agent_skill(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    name: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    reject_plugin_skill_mutation(&state, &name)?;
    let name = require_canonical_agent_skill_name(&name)?.to_string();
    let (user_root, _) = configured_agent_skill_roots(&state)?;
    // previous_state captures the exact override restored when filesystem deletion fails.
    // previous_state 捕获文件系统删除失败时恢复的精确覆盖状态。
    let previous_state = provider_configuration_from_state(&state)?
        .prompt_skills
        .skill_overrides
        .get(&name)
        .copied();
    let state_name = name.clone();
    crate::service_state_access::mutate_provider_configuration_with_state_validation(
        &state,
        "Failed to delete the Agent Skill state",
        |guard| {
            crate::service_state_access::require_plugin_update_full_publication_admission(
                guard, None,
            )
        },
        move |configuration| {
            configuration
                .prompt_skills
                .set_skill_state(&state_name, None);
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    let (package_root, package_name) = (user_root.clone(), name.clone());
    if let Err(file_error) = run_agent_skill_file_operation(move || {
        vulcan_prompt_skills::delete_user_prompt_skill_package(&package_root, &package_name)
    })
    .await
    {
        let rollback_name = name.clone();
        let rollback_result = mutate_provider_configuration(
            &state,
            "Failed to roll back the Agent Skill deletion state",
            move |configuration| {
                configuration
                    .prompt_skills
                    .set_skill_state(&rollback_name, previous_state);
                Ok(())
            },
            |_guard, _configuration| (),
        )
        .await;
        return match rollback_result {
            Ok(()) => Err(file_error),
            Err(rollback_error) => Err(format!(
                "{file_error}; Agent Skill deletion rollback also failed: {rollback_error}"
            )),
        };
    }
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Restores a system Agent Skill by deleting its exact user override package.
/// 通过删除精确用户覆盖技能包恢复系统 Agent Skill。
pub(crate) async fn service_restore_agent_skill_default(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    name: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let name = require_canonical_agent_skill_name(&name)?.to_string();
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    run_agent_skill_file_operation(move || {
        if !system_root
            .join(&name)
            .join(vulcan_prompt_skills::SKILL_FILE_NAME)
            .is_file()
        {
            return Err(format!(
                "Agent Skill `{name}` has no system version to restore."
            ));
        }
        vulcan_prompt_skills::delete_user_prompt_skill_package(&user_root, &name)
    })
    .await?;
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Saves one user-defined logical Agent Skills category.
/// 保存一个用户定义逻辑 Agent Skills 分类。
pub(crate) async fn service_save_agent_skill_category(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: AgentSkillCategorySaveRequest,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    if request.category_id == vulcan_prompt_skills::PLUGIN_CATEGORY_ID {
        return Err(String::from(
            "The `plugins` category is host-owned and cannot be modified.",
        ));
    }
    let (user_root, _) = configured_agent_skill_roots(&state)?;
    let category = vulcan_prompt_skills::PromptSkillCategory {
        category_id: request.category_id,
        name: request.name,
        description: request.description,
        emoji: request.emoji,
        color: request.color,
        order: request.order,
        builtin: false,
        // User categories are single-language; only built-in categories are localized.
        // 用户分类只有单一语言；仅内置分类提供本地化。
        locales: std::collections::BTreeMap::new(),
    };
    run_agent_skill_file_operation(move || {
        vulcan_prompt_skills::save_user_prompt_skill_category(&user_root, category)
    })
    .await?;
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Deletes one user-defined Agent Skills category and unassigns its skills.
/// 删除一个用户定义 Agent Skills 分类并解除其技能归属。
pub(crate) async fn service_delete_agent_skill_category(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    category_id: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    if category_id == vulcan_prompt_skills::PLUGIN_CATEGORY_ID {
        return Err(String::from(
            "The `plugins` category is host-owned and cannot be deleted.",
        ));
    }
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    run_agent_skill_file_operation(move || {
        vulcan_prompt_skills::delete_user_prompt_skill_category(
            &system_root,
            &user_root,
            &category_id,
        )
    })
    .await?;
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Assigns one effective Agent Skill to one exact logical category.
/// 将一个有效 Agent Skill 归入一个精确逻辑分类。
pub(crate) async fn service_set_agent_skill_category(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    name: String,
    category_id: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    reject_plugin_skill_mutation(&state, &name)?;
    if category_id == vulcan_prompt_skills::PLUGIN_CATEGORY_ID {
        return Err(String::from(
            "The `plugins` category is host-owned and cannot receive manual assignments.",
        ));
    }
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    run_agent_skill_file_operation(move || {
        vulcan_prompt_skills::set_user_prompt_skill_category(
            &system_root,
            &user_root,
            &name,
            &category_id,
        )
    })
    .await?;
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Resolves the exact configured user and system Agent Skills roots.
/// 解析精确配置的用户与系统 Agent Skills 根目录。
fn configured_agent_skill_roots(
    state: &SharedServiceBridgeState,
) -> Result<(PathBuf, PathBuf), String> {
    let configuration = provider_configuration_from_state(state)?;
    let runtime_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &configuration.prompt_skills,
    );
    Ok((
        vulcan_prompt_skills::configured_user_skill_root(&runtime_config),
        vulcan_prompt_skills::configured_system_skill_root(&runtime_config),
    ))
}

/// Resolves auditable provenance for one managed user package save.
/// 为一次受管用户技能包保存解析可审计来源信息。
fn managed_agent_skill_origin(
    catalog: &vulcan_prompt_skills::PromptSkillCatalog,
    original_name: Option<&str>,
    target_name: &str,
) -> vulcan_prompt_skills::PromptSkillOriginMetadata {
    // timestamp is the canonical UTC audit time for this exact save.
    // timestamp 是本次精确保存的规范 UTC 审计时间。
    let timestamp = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("current UTC time must format as RFC 3339");
    // existing_user is the only source whose provenance survives an edit or rename.
    // existing_user 是编辑或重命名时唯一保留来源信息的来源。
    let existing_user = original_name.and_then(|name| {
        catalog.records.iter().find(|skill| {
            skill.scope == vulcan_prompt_skills::PromptSkillScope::User && skill.name == name
        })
    });
    if let Some(existing_user) = existing_user {
        let mut origin = existing_user.origin.clone();
        origin.updated_at = Some(timestamp);
        return origin;
    }
    // system_source identifies an explicit same-name user override creation.
    // system_source 标识一次明确的同名用户覆盖创建。
    let system_source = catalog.records.iter().any(|skill| {
        skill.scope == vulcan_prompt_skills::PromptSkillScope::System && skill.name == target_name
    });
    if system_source {
        return vulcan_prompt_skills::PromptSkillOriginMetadata {
            kind: String::from("local-override"),
            label: Some(String::from("User override")),
            managed: false,
            installed_at: Some(timestamp.clone()),
            updated_at: Some(timestamp),
            ..vulcan_prompt_skills::PromptSkillOriginMetadata::default()
        };
    }
    vulcan_prompt_skills::PromptSkillOriginMetadata {
        kind: String::from("local-created"),
        label: Some(String::from("User-created")),
        managed: false,
        installed_at: Some(timestamp.clone()),
        updated_at: Some(timestamp),
        ..vulcan_prompt_skills::PromptSkillOriginMetadata::default()
    }
}

/// Refreshes the agent catalog and updates the shared task runtime snapshot.
/// 刷新智能体目录并更新共享任务运行时快照。
pub(crate) async fn service_refresh_subagent_catalog(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    // Prevent refresh from committing a catalog loaded between another request's file mutation and projection.
    // 防止刷新提交在其他请求文件变更与投影之间加载到的目录。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    refresh_subagent_catalog_local_state_and_emit(&app, &state)
}

/// Opens the user-owned subagent definition directory in the host file manager.
/// 在宿主文件管理器中打开用户拥有的智能体定义目录。
pub(crate) async fn service_open_subagent_user_directory(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    let user_definition_root = user_subagent_definition_root();
    std::fs::create_dir_all(&user_definition_root).map_err(|error| {
        format!(
            "Failed to create the user agent definition directory `{}`: {error}",
            user_definition_root.display()
        )
    })?;
    // Keep the directory refresh ordered with all catalog file mutations before opening the projected path.
    // 在打开已投影路径前，使目录刷新与全部目录文件变更保持有序。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let snapshot = refresh_subagent_catalog_local_state(&state)?;
    // Release catalog serialization before the external file-manager request, which does not mutate catalog state.
    // 在不变更目录状态的外部文件管理器请求前释放目录串行守卫。
    drop(_catalog_mutation_guard);
    shell_host_from_state(&state)?.open_path(
        &vulcan_runtime_assets::native_host_path_text_from_path(&user_definition_root),
    )?;
    Ok(publish_subagent_catalog_update(&app, snapshot))
}

/// Persists one agent definition enabled state through the sidecar configuration layer.
/// 通过 sidecar 配置层持久化单个智能体定义启停状态。
pub(crate) async fn service_set_subagent_definition_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    agent_id: String,
    enabled: bool,
) -> Result<SubagentCatalogUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from(
            "The agent definition identifier must not be empty.",
        ));
    }
    reject_plugin_agent_mutation(&state, agent_id)?;
    // Keep the filesystem override and both retained runtime projections ordered as one catalog transaction.
    // 将文件系统覆盖与两份保留的运行时投影按同一个目录事务排序。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !guard
            .workbench
            .subagent_catalog
            .definitions
            .iter()
            .any(|definition| definition.agent_id == agent_id)
        {
            return Err(format!("Agent definition not found: {agent_id}"));
        }
    }
    vulcan_task_session::write_user_subagent_enabled_override(
        user_subagent_root().as_path(),
        agent_id,
        enabled,
    )
    .map_err(|error| {
        format!(
            "Failed to write the agent enable override `{}`: {}",
            error.path.display(),
            error.message
        )
    })?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(definition) = guard
        .workbench
        .subagent_catalog
        .definitions
        .iter_mut()
        .find(|definition| definition.agent_id == agent_id)
    else {
        return Err(format!("Agent definition not found: {agent_id}"));
    };
    definition.enabled = enabled;
    if let Some(runtime) = guard.agent_runtime.as_ref() {
        runtime
            .task_runtime
            .set_subagent_definition_enabled(agent_id, enabled)
            .map_err(|error| format!("Failed to update the runtime agent enable state: {error}"))?;
    }
    let notice = subagent_catalog_notice_from_summary(&guard.workbench.subagent_catalog);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.subagent_catalog.notice = notice;
    let update = SubagentCatalogUpdate {
        revision: guard.workbench_generation,
        subagent_catalog: guard.workbench.subagent_catalog.clone(),
    };
    drop(guard);
    Ok(publish_subagent_catalog_update(&app, update))
}

/// Saves one user-authored agent definition into the user catalog.
/// 将单个用户编写的智能体定义保存到用户目录。
pub(crate) async fn service_save_subagent_definition(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SubagentDefinitionSaveRequest,
) -> Result<SubagentCatalogUpdate, String> {
    let requested_agent_id = normalize_agent_file_name_input(&request.agent_id);
    if requested_agent_id.is_empty() {
        return Err(String::from("The agent identifier must not be empty."));
    }
    let mode = vulcan_task_session::SubagentDefinitionMode::Expert;
    let target_package_id = normalized_optional_user_package_id(request.package_id.as_deref())?;
    let submitted_original_agent_id = request
        .original_agent_id
        .as_deref()
        .map(normalize_agent_file_name_input)
        .filter(|value| !value.is_empty());
    let original_agent_id = submitted_original_agent_id
        .as_deref()
        .unwrap_or(requested_agent_id.as_str());
    reject_plugin_agent_mutation(&state, original_agent_id)?;
    // Prevent concurrent save, delete, restore, and toggle requests from observing an intermediate catalog.
    // 防止并发保存、删除、恢复与启停请求观察到目录中间态。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let catalog = load_current_subagent_catalog();
    let existing = submitted_original_agent_id
        .as_deref()
        .and_then(|original_agent_id| {
            catalog
                .definitions
                .iter()
                .find(|definition| definition.id == original_agent_id)
                .cloned()
        });
    let (public_ai_runtime, public_ai_configuration) = public_ai_runtime_and_configuration(&state)?;
    let public_ai_catalogs = public_ai_runtime.catalog_snapshot()?;
    let requested_provider_model = validate_subagent_provider_model_request_with_existing(
        &request,
        existing.as_ref(),
        &public_ai_configuration,
        &public_ai_catalogs,
    )?;
    if let Some(existing_definition) = existing
        .as_ref()
        .filter(|definition| subagent_definition_is_system_backed(definition))
    {
        if requested_agent_id != original_agent_id {
            return Err(String::from("System agents cannot change their agent_id."));
        }
        if target_package_id.is_some() {
            return Err(String::from(
                "System agents cannot be moved to a user package.",
            ));
        }
        let mut metadata_override = existing_definition.clone();
        apply_request_tool_policy(&mut metadata_override.tool_exposure, &request);
        apply_request_prompt_skill_policy(&mut metadata_override.prompt_skills, &request);
        metadata_override.provider_model = requested_provider_model;
        metadata_override.system_prompt_append = request
            .system_prompt_append
            .as_deref()
            .and_then(non_empty_request_text);
        metadata_override.enabled = existing_definition.enabled;
        vulcan_task_session::write_user_subagent_metadata_override(
            user_subagent_root().as_path(),
            &metadata_override,
        )
        .map_err(|error| {
            format!(
                "Failed to save the user metadata override of system agent `{}`: {}",
                existing_definition.id, error.message
            )
        })?;
        return refresh_subagent_catalog_local_state_and_emit(&app, &state);
    }
    let target_agent_id = target_agent_id_for_save(
        &requested_agent_id,
        existing.as_ref(),
        target_package_id.as_deref(),
    )?;
    // Preserve the host-owned plugin category for immutable plugin contributions only.
    // 仅为不可变插件贡献保留宿主拥有的插件分类。
    if vulcan_task_session::display_subagent_category_id(
        &target_agent_id,
        target_package_id.as_deref(),
    )
    .as_deref()
        == Some(vulcan_task_session::PLUGIN_SUBAGENT_CATEGORY_ID)
    {
        return Err(String::from(
            "The `plugins` category is reserved by the plugin system; ordinary agents cannot write to it.",
        ));
    }
    let target_conflicts_with_other_definition = catalog
        .definitions
        .iter()
        .any(|definition| definition.id == target_agent_id && definition.id != original_agent_id);
    let new_definition_conflicts_with_existing = existing.is_none()
        && catalog
            .definitions
            .iter()
            .any(|definition| definition.id == target_agent_id);
    if target_conflicts_with_other_definition || new_definition_conflicts_with_existing {
        return Err(format!(
            "Target agent `{target_agent_id}` already exists; use another file name or edit the existing agent first."
        ));
    }
    let user_file = user_subagent_definition_path(&target_agent_id)?;
    let mut definition = existing.unwrap_or_else(|| vulcan_task_session::SubagentDefinition {
        id: target_agent_id.clone(),
        name: target_agent_id.clone(),
        description: None,
        locales: std::collections::BTreeMap::new(),
        origin: None,
        emoji: String::from("🤖"),
        color: String::from("gray"),
        vibe: None,
        mode,
        system_prompt: None,
        system_prompt_append: None,
        provider_model: Default::default(),
        tool_exposure: Default::default(),
        prompt_skills: Default::default(),
        context_compaction: Default::default(),
        runtime_policy: Default::default(),
        max_depth: None,
        enabled: true,
        source: vulcan_task_session::SubagentDefinitionSource {
            scope: vulcan_task_session::SubagentDefinitionScope::User,
            file: user_file.clone(),
            sidecar_file: None,
            source_key: format!("user:{target_agent_id}"),
            category_id: vulcan_task_session::display_subagent_category_id(
                &target_agent_id,
                target_package_id.as_deref(),
            ),
            package_id: target_package_id.clone(),
            remote_url: None,
            override_of: None,
            source_hash: None,
            version: None,
            etag: None,
            last_modified: None,
            updated_at: None,
        },
        extra: serde_json::Value::Null,
    });
    let previous_agent_id = definition.id.clone();
    let previous_source = definition.source.clone();
    let previous_scope = definition.source.scope;
    definition.id = target_agent_id.clone();
    apply_request_tool_policy(&mut definition.tool_exposure, &request);
    apply_request_prompt_skill_policy(&mut definition.prompt_skills, &request);
    apply_user_definition_display_metadata(&mut definition, &request, &target_agent_id);
    apply_defaulted_text_patch(request.emoji, &mut definition.emoji, "🤖");
    apply_defaulted_text_patch(request.color, &mut definition.color, "gray");
    apply_optional_text_patch(request.vibe, &mut definition.vibe);
    definition.mode = mode;
    definition.enabled = request.enabled;
    definition.system_prompt = request
        .system_prompt
        .as_deref()
        .and_then(non_empty_request_text);
    definition.system_prompt_append = request
        .system_prompt_append
        .as_deref()
        .and_then(non_empty_request_text);
    definition.provider_model = requested_provider_model;
    definition.tool_exposure.luaskills_overrides = request.luaskills_overrides;
    definition.tool_exposure.mcp_server_overrides = request.mcp_server_overrides;
    definition.source.scope = vulcan_task_session::SubagentDefinitionScope::User;
    definition.source.file = user_file;
    definition.source.sidecar_file = None;
    definition.source.source_key = format!("user:{target_agent_id}");
    definition.source.category_id = vulcan_task_session::display_subagent_category_id(
        &target_agent_id,
        target_package_id.as_deref(),
    );
    definition.source.package_id = target_package_id;
    definition.source.updated_at = Some(current_unix_millis_text());
    definition.source.override_of = None;
    let written_user_file = vulcan_task_session::write_user_subagent_definition(
        user_subagent_root().as_path(),
        &definition,
    )
    .map_err(|error| {
        format!(
            "Failed to save agent definition `{}`: {}",
            error.path.display(),
            error.message
        )
    })?;
    remove_user_subagent_file_if_inside_root(&user_subagent_override_path(&target_agent_id)?)?;
    if previous_agent_id != target_agent_id
        && previous_scope != vulcan_task_session::SubagentDefinitionScope::System
    {
        remove_stale_user_subagent_files_for_source(
            &previous_agent_id,
            &previous_source,
            &written_user_file,
        )?;
    }
    refresh_subagent_catalog_local_state_and_emit(&app, &state)
}

/// Deletes one user-owned agent definition from the user catalog.
/// 从用户目录删除单个用户拥有的智能体定义。
pub(crate) async fn service_delete_subagent_definition(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<SubagentCatalogUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from(
            "The agent definition identifier must not be empty.",
        ));
    }
    reject_plugin_agent_mutation(&state, agent_id)?;
    // Keep catalog discovery and deletion under the same process-wide mutation guard.
    // 在同一个进程级变更守卫下完成目录发现与删除。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let catalog = load_current_subagent_catalog();
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("Agent definition not found: {agent_id}"))?;
    if !matches!(
        definition.source.scope,
        vulcan_task_session::SubagentDefinitionScope::User
            | vulcan_task_session::SubagentDefinitionScope::Package
    ) || definition.source.override_of.is_some()
    {
        return Err(String::from("Only user-defined agents can be deleted."));
    }
    let delete_candidates = user_definition_delete_candidates(agent_id, definition)?;
    let mut deleted_any = false;
    for path in delete_candidates {
        if user_subagent_file_exists_inside_root(&path)? {
            remove_user_subagent_file_if_inside_root(&path)?;
            deleted_any = true;
        }
    }
    if !deleted_any {
        return Err(format!("No deletable user agent file found: {agent_id}"));
    }
    refresh_subagent_catalog_local_state_and_emit(&app, &state)
}

/// Reads the prompt text of one agent definition from the retained runtime catalog.
/// 从保留的运行时目录读取单个智能体定义的提示词正文。
pub(crate) async fn service_subagent_definition_prompts(
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<SubagentDefinitionPrompts, String> {
    let catalog = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime = guard.agent_runtime.as_ref().ok_or_else(|| {
            String::from("service is still starting; the agent catalog is not ready")
        })?;
        Arc::clone(&runtime.subagent_catalog)
    };
    subagent_definition_prompts_from_catalog(&catalog, agent_id.trim())
}

/// Projects the prompt text of one definition in an already loaded catalog.
/// 从已加载目录中投影单个定义的提示词正文。
fn subagent_definition_prompts_from_catalog(
    catalog: &vulcan_task_session::SubagentCatalog,
    agent_id: &str,
) -> Result<SubagentDefinitionPrompts, String> {
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("Agent definition not found: {agent_id}"))?;
    Ok(SubagentDefinitionPrompts {
        agent_id: definition.id.clone(),
        system_prompt: definition.system_prompt.clone(),
        system_prompt_append: definition.system_prompt_append.clone(),
    })
}

/// Restores one system-backed agent definition by deleting its user override.
/// 通过删除用户覆盖来恢复单个系统集成智能体定义。
pub(crate) async fn service_restore_subagent_definition_default(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<SubagentCatalogUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from(
            "The agent definition identifier must not be empty.",
        ));
    }
    reject_plugin_agent_mutation(&state, agent_id)?;
    // Keep restore eligibility checks and override deletion in one serialized catalog transaction.
    // 将恢复资格检查与覆盖删除保持在同一个串行目录事务中。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let catalog = load_current_subagent_catalog();
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("Agent definition not found: {agent_id}"))?;
    let restore_candidates = restore_default_user_file_candidates(agent_id, definition)?;
    let has_user_layer = restore_candidates.iter().try_fold(false, |found, path| {
        user_subagent_file_exists_inside_root(path).map(|exists| found || exists)
    })?;
    let can_restore = has_user_layer
        && (system_subagent_definition_exists(agent_id)
            || definition
                .source
                .override_of
                .as_deref()
                .is_some_and(system_subagent_definition_exists));
    if !can_restore {
        return Err(format!(
            "The current agent has no system or package default definition to restore: {agent_id}"
        ));
    }
    for path in restore_candidates {
        remove_user_subagent_file_if_inside_root(&path)?;
    }
    refresh_subagent_catalog_local_state_and_emit(&app, &state)
}

/// Persists one Agent Skill state and refreshes only the Agent Skills catalog.
/// 持久化单个 Agent Skill 状态并仅刷新 Agent Skills 目录。
pub(crate) async fn service_set_agent_skill_enabled(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    name: String,
    enabled: bool,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    reject_plugin_skill_mutation(&state, &name)?;
    let name = require_existing_agent_skill_name(&state, &name)?;
    let state_override = if enabled {
        PersistedPromptSkillState::Enabled
    } else {
        PersistedPromptSkillState::Disabled
    };
    crate::service_state_access::mutate_provider_configuration_with_state_validation(
        &state,
        "Failed to save the Agent Skill state",
        |guard| {
            crate::service_state_access::require_plugin_update_full_publication_admission(
                guard, None,
            )
        },
        |configuration| {
            configuration
                .prompt_skills
                .set_skill_state(&name, Some(state_override));
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    refresh_agent_skills_local_state_and_emit(&app, &state).await
}

/// Requires one exact canonical Agent Skills identity without rewriting caller input.
/// 要求一个精确的 Agent Skills 规范身份，不改写调用方输入。
fn require_canonical_agent_skill_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name != name.trim() {
        return Err(String::from(
            "Agent Skill name must already be canonical and cannot be empty or contain surrounding whitespace.",
        ));
    }
    Ok(name)
}

/// Rejects every direct mutation targeting a plugin-owned Agent Skill.
/// 拒绝所有指向插件自有 Agent Skill 的直接变更。
///
/// # Parameters
/// 参数
/// - `state`: Shared service state containing the authoritative plugin catalog.
/// - `state`：包含权威插件目录的共享服务状态。
/// - `skill_identity`: Canonical skill name or source-aware `plugin:` key.
/// - `skill_identity`：规范技能名称或带来源的 `plugin:` 键。
///
/// # Returns
/// 返回值
/// Returns unit for non-plugin identities, otherwise an ownership diagnostic.
/// 非插件标识返回空值，否则返回所有权诊断。
fn reject_plugin_skill_mutation(
    state: &SharedServiceBridgeState,
    skill_identity: &str,
) -> Result<(), String> {
    // Plugin skill names always contain the manifest-defined namespace separator after any source-key prefix.
    // 插件技能名称在可选来源键前缀之后始终包含清单定义的命名空间分隔符。
    let canonical_name = skill_identity
        .strip_prefix("plugin:")
        .unwrap_or(skill_identity);
    if !canonical_name.contains(':') {
        return Ok(());
    }
    // A plugin-namespaced mutation must fail closed when ownership cannot be resolved.
    // 插件命名空间变更在无法解析所有权时必须以失败关闭。
    let runtime = service_vulcan_plugin_runtime(state)?;
    let snapshot = runtime.snapshot();
    reject_plugin_skill_mutation_in_catalog(snapshot.as_ref(), skill_identity)
}

/// Rejects one direct skill mutation against an exact immutable plugin catalog snapshot.
/// 根据一份精确不可变插件目录快照拒绝单次直接 Skill 变更。
///
/// # Parameters
/// 参数
/// `snapshot` is the authoritative plugin generation and `skill_identity` is a canonical or source-aware key.
/// `snapshot` 是权威插件代次，`skill_identity` 是规范标识或带来源键。
///
/// # Returns
/// 返回值
/// Returns unit for non-plugin identities, otherwise a stable ownership diagnostic.
/// 非插件标识返回空值，否则返回稳定的所有权诊断。
fn reject_plugin_skill_mutation_in_catalog(
    snapshot: &vulcan_plugin_runtime::VulcanPluginCatalogSnapshot,
    skill_identity: &str,
) -> Result<(), String> {
    // canonical_name removes only the host-owned source-key prefix used by catalog mutation APIs.
    // canonical_name 仅移除目录变更 API 使用的宿主自有来源键前缀。
    let canonical_name = skill_identity
        .strip_prefix("plugin:")
        .unwrap_or(skill_identity);
    let owner = snapshot.plugins.values().find(|plugin| {
        plugin
            .skills
            .iter()
            .any(|skill| skill.name == canonical_name)
    });
    if let Some(plugin) = owner {
        return Err(format!(
            "Agent Skill `{canonical_name}` is managed by plugin `{}` and is read-only; change it through plugin lifecycle management.",
            plugin.id
        ));
    }
    if canonical_name.contains(':') {
        return Err(format!(
            "Agent Skill `{canonical_name}` uses a plugin namespace that is not present in the committed plugin catalog."
        ));
    }
    Ok(())
}

/// Rejects every direct mutation targeting a plugin-owned subagent definition.
/// 拒绝所有指向插件自有子代理定义的直接变更。
///
/// # Parameters
/// 参数
/// - `state`: Shared service state containing the authoritative plugin catalog.
/// - `state`：包含权威插件目录的共享服务状态。
/// - `agent_id`: Canonical namespaced agent identifier.
/// - `agent_id`：规范命名空间智能体标识。
///
/// # Returns
/// 返回值
/// Returns unit for non-plugin identities, otherwise an ownership diagnostic.
/// 非插件标识返回空值，否则返回所有权诊断。
fn reject_plugin_agent_mutation(
    state: &SharedServiceBridgeState,
    agent_id: &str,
) -> Result<(), String> {
    if !agent_id.contains(':') {
        return Ok(());
    }
    // A plugin-namespaced mutation must fail closed when ownership cannot be resolved.
    // 插件命名空间变更在无法解析所有权时必须以失败关闭。
    let runtime = service_vulcan_plugin_runtime(state)?;
    let snapshot = runtime.snapshot();
    reject_plugin_agent_mutation_in_catalog(snapshot.as_ref(), agent_id)
}

/// Rejects one direct agent mutation against an exact immutable plugin catalog snapshot.
/// 根据一份精确不可变插件目录快照拒绝单次直接 Agent 变更。
///
/// # Parameters
/// 参数
/// `snapshot` is the authoritative plugin generation and `agent_id` is the canonical namespaced identity.
/// `snapshot` 是权威插件代次，`agent_id` 是规范命名空间标识。
///
/// # Returns
/// 返回值
/// Returns unit for non-plugin identities, otherwise a stable ownership diagnostic.
/// 非插件标识返回空值，否则返回稳定的所有权诊断。
fn reject_plugin_agent_mutation_in_catalog(
    snapshot: &vulcan_plugin_runtime::VulcanPluginCatalogSnapshot,
    agent_id: &str,
) -> Result<(), String> {
    let owner = snapshot
        .plugins
        .values()
        .find(|plugin| plugin.agents.iter().any(|agent| agent.id == agent_id));
    if let Some(plugin) = owner {
        return Err(format!(
            "Agent `{agent_id}` is managed by plugin `{}` and is read-only; change it through plugin lifecycle management.",
            plugin.id
        ));
    }
    if agent_id.contains(':') {
        return Err(format!(
            "Agent `{agent_id}` uses a plugin namespace that is not present in the committed plugin catalog."
        ));
    }
    Ok(())
}

/// Resolves one exact source-aware Agent Skill record without effective-source substitution.
/// 解析一条精确的来源感知 Agent Skill 记录且不替换为有效来源。
fn exact_agent_skill_source_record<'catalog>(
    catalog: &'catalog vulcan_prompt_skills::PromptSkillCatalog,
    skill_key: &str,
) -> Result<&'catalog vulcan_prompt_skills::PromptSkill, String> {
    if skill_key.is_empty() || skill_key != skill_key.trim() {
        return Err(String::from(
            "Agent Skill source key must be exact and cannot be empty or contain surrounding whitespace.",
        ));
    }
    catalog
        .source_record(skill_key)
        .ok_or_else(|| format!("Agent Skill source record `{skill_key}` does not exist."))
}

/// Resolves one canonical name only when the current effective catalog contains it.
/// 仅当当前有效目录包含技能时解析其规范名称。
fn require_existing_agent_skill_name(
    state: &SharedServiceBridgeState,
    name: &str,
) -> Result<String, String> {
    let name = require_canonical_agent_skill_name(name)?;
    let (user_root, system_root) = configured_agent_skill_roots(state)?;
    let catalog = vulcan_prompt_skills::load_prompt_skills_from_roots(system_root, user_root);
    if catalog.skill(name).is_none() {
        return Err(format!("Agent Skill `{name}` does not exist."));
    }
    Ok(name.to_string())
}

/// Loads project-level Agent Skill overrides for the WebUI project settings modal.
/// 为 WebUI 项目设置弹窗加载项目级 Agent Skill 覆盖。
pub(crate) fn service_load_project_agent_skill_overrides(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<BTreeMap<String, String>, String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let project_configuration = load_project_configuration(&project_path);
    Ok(project_configuration
        .prompt_skills
        .skill_overrides
        .into_iter()
        .map(|(name, state)| {
            let value = if state.is_enabled() {
                "enabled"
            } else {
                "disabled"
            };
            (name, value.to_string())
        })
        .collect())
}

/// Persists one project-level Agent Skill tri-state override and broadcasts its invalidation.
/// 持久化单个项目级 Agent Skill 三态覆盖并广播其失效通知。
///
/// # Parameters
/// 参数
/// `app` broadcasts the project-scoped invalidation to every connected client.
/// `app` 将项目级失效通知广播给每个已连接客户端。
/// `state` resolves the exact project path and owns the workbench generation.
/// `state` 解析准确项目路径并持有工作台版本。
/// `project_id` identifies the project configuration to mutate.
/// `project_id` 标识需要修改的项目配置。
/// `name` identifies the exact Agent Skill override.
/// `name` 标识准确的 Agent Skill 覆盖项。
/// `state_key` is the requested inherit, enabled, or disabled state.
/// `state_key` 是请求的继承、启用或禁用状态。
///
/// # Returns
/// 返回值
/// `Ok(())` after persistence and cross-client invalidation both succeed.
/// 持久化与跨客户端失效通知均成功后返回 `Ok(())`。
pub(crate) async fn service_set_agent_skill_project_state(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: String,
    name: String,
    state_key: String,
) -> Result<(), String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let stable_name = name.trim().to_string();
    if stable_name.is_empty() {
        return Err(String::from(
            "The project-level Agent Skill name must not be empty.",
        ));
    }
    reject_plugin_skill_mutation(&state, &stable_name)?;
    let state_override = parse_agent_skill_project_state(&state_key)?;
    mutate_project_configuration(
        &PathBuf::from(&project_path),
        "Failed to save the project Agent Skill state",
        move |project_configuration| {
            match state_override {
                Some(state_override) => {
                    project_configuration
                        .prompt_skills
                        .skill_overrides
                        .insert(stable_name.to_string(), state_override);
                }
                None => {
                    project_configuration
                        .prompt_skills
                        .skill_overrides
                        .remove(stable_name.as_str());
                }
            }
            Ok(())
        },
        |_project_configuration| Ok(()),
    )
    .await?;
    commit_and_emit_project_settings_changed_delta(
        &app,
        &state,
        Some(project_id),
        ProjectSettingsDomain::AgentSkills,
    )
}

/// Reprojects agent catalog after a focused catalog write or refresh.
/// 在聚焦的目录写入或刷新后重新投影智能体目录。
fn refresh_subagent_catalog_local_state(
    state: &SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    refresh_subagent_catalog_local_state_with_errors(state, Vec::new())
}

/// Reprojects agent catalog and appends transient refresh errors.
/// 重新投影智能体目录并追加临时刷新错误。
fn refresh_subagent_catalog_local_state_with_errors(
    state: &SharedServiceBridgeState,
    transient_errors: Vec<vulcan_task_session::SubagentCatalogLoadError>,
) -> Result<SubagentCatalogUpdate, String> {
    let mut catalog = load_current_subagent_catalog();
    let runtime = service_vulcan_plugin_runtime(state)?;
    // plugin_catalog is captured once so the refresh cannot mix plugin generations.
    // plugin_catalog 仅捕获一次，确保刷新不会混用不同插件代次。
    let plugin_catalog = runtime.snapshot();
    catalog
        .merge_plugin_definitions(plugin_catalog.enabled_agents.values().cloned())
        .map_err(|error| format!("Failed to merge the plugin agent catalog: {error}"))?;
    catalog.errors.extend(transient_errors);
    let available_model_selectors = public_ai_available_model_selectors(state)?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_mut() {
        runtime
            .task_runtime
            .replace_subagent_catalog(catalog.clone())
            .map_err(|error| format!("Failed to refresh the runtime agent catalog: {error}"))?;
        // subagentCatalog preserves the exact refresh generation for projection without another filesystem traversal.
        // subagentCatalog 为投影保留本次精确刷新代次，避免再次遍历文件系统。
        runtime.subagent_catalog = Arc::new(catalog.clone());
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.subagent_catalog =
        subagent_catalog_summary_from_catalog(&catalog, Some(&available_model_selectors));
    Ok(SubagentCatalogUpdate {
        revision: guard.workbench_generation,
        subagent_catalog: guard.workbench.subagent_catalog.clone(),
    })
}

/// Builds the agent catalog notice from the current projected summary.
/// 从当前已投影摘要构建智能体目录提示。
fn subagent_catalog_notice_from_summary(catalog: &SubagentCatalogSummary) -> String {
    let enabled_count = catalog
        .definitions
        .iter()
        .filter(|definition| definition.enabled)
        .count();
    format!(
        "{} agent definitions, {} enabled, {} loading warnings.",
        catalog.definitions.len(),
        enabled_count,
        catalog.errors.len()
    )
}

/// Loads the current two-layer agent catalog from system and user roots.
/// 从系统根与用户根加载当前双层智能体目录。
fn load_current_subagent_catalog() -> vulcan_task_session::SubagentCatalog {
    vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        user_subagent_root(),
    )
}

/// Returns whether a system-integrated agent definition exists before user overrides are applied.
/// 返回应用用户覆盖前是否存在系统集成智能体定义。
fn system_subagent_definition_exists(agent_id: &str) -> bool {
    let catalog = vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        PathBuf::new(),
    );
    catalog.definitions.iter().any(|definition| {
        definition.id == agent_id
            && definition.source.scope == vulcan_task_session::SubagentDefinitionScope::System
    })
}

/// Returns whether one effective definition still represents a system-integrated agent.
/// 返回单个生效定义是否仍代表系统集成智能体。
fn subagent_definition_is_system_backed(
    definition: &vulcan_task_session::SubagentDefinition,
) -> bool {
    definition.source.scope == vulcan_task_session::SubagentDefinitionScope::System
        || definition
            .source
            .override_of
            .as_deref()
            .is_some_and(system_subagent_definition_exists)
        || (definition.source.package_id.is_none()
            && system_subagent_definition_exists(&definition.id))
}

/// Returns the user-owned agent catalog root.
/// 返回用户拥有的智能体目录根路径。
fn user_subagent_root() -> PathBuf {
    vulcan_workspace_store::data_plane::application_data_root().join("agents")
}

/// Returns the user-owned agent definition directory.
/// 返回用户拥有的智能体定义目录。
fn user_subagent_definition_root() -> PathBuf {
    user_subagent_root().join("definitions")
}

/// Returns the user-owned file path for one agent definition id.
/// 返回单个智能体定义标识对应的用户文件路径。
fn user_subagent_definition_path(agent_id: &str) -> Result<PathBuf, String> {
    vulcan_task_session::user_subagent_definition_path(user_subagent_root().as_path(), agent_id)
        .map_err(|error| error.message)
}

/// Returns the user-owned sidecar file path for one agent definition id.
/// 返回单个智能体定义标识对应的用户 sidecar 文件路径。
fn user_subagent_sidecar_path(agent_id: &str) -> Result<PathBuf, String> {
    vulcan_task_session::user_subagent_sidecar_path(user_subagent_root().as_path(), agent_id)
        .map_err(|error| error.message)
}

/// Returns the user-owned override file path for one agent definition id.
/// 返回单个智能体定义标识对应的用户覆盖文件路径。
fn user_subagent_override_path(agent_id: &str) -> Result<PathBuf, String> {
    vulcan_task_session::user_subagent_override_path(user_subagent_root().as_path(), agent_id)
        .map_err(|error| error.message)
}

/// Applies the single-language display metadata of one user-owned agent definition.
/// 应用单个用户拥有智能体定义的单语言展示元数据。
///
/// # Parameters
/// 参数
/// `definition` is the user-owned definition being saved, `request` carries the submitted name and
/// description, and `fallback_name` is used when the submitted name is blank.
/// `definition` 是正在保存的用户定义，`request` 携带提交的名称与说明，`fallback_name` 在提交名称为空时使用。
///
/// User-owned definitions are single-language, so any per-locale metadata is cleared; only bundled
/// system definitions are localized.
/// 用户拥有的定义只有单一语言，因此会清除按语言的元数据；仅内置系统定义提供本地化。
pub(crate) fn apply_user_definition_display_metadata(
    definition: &mut vulcan_task_session::SubagentDefinition,
    request: &SubagentDefinitionSaveRequest,
    fallback_name: &str,
) {
    definition.name =
        non_empty_request_text(&request.name).unwrap_or_else(|| fallback_name.to_string());
    definition.description = request
        .description
        .as_deref()
        .and_then(non_empty_request_text);
    definition.locales = std::collections::BTreeMap::new();
}

/// Returns trimmed request text only when it is non-empty.
/// 仅在请求文本非空时返回裁剪后的文本。
fn non_empty_request_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Applies one tri-state text patch to a defaulted string field.
/// 将一个三态文本补丁应用到带默认值的字符串字段。
fn apply_defaulted_text_patch(patch: OptionalTextPatch, value: &mut String, default_value: &str) {
    match patch {
        OptionalTextPatch::Missing => {}
        OptionalTextPatch::Clear => {
            *value = default_value.to_string();
        }
        OptionalTextPatch::Set(next_value) => {
            *value =
                non_empty_request_text(&next_value).unwrap_or_else(|| default_value.to_string());
        }
    }
}

/// Applies one tri-state text patch to an optional string field.
/// 将一个三态文本补丁应用到可选字符串字段。
fn apply_optional_text_patch(patch: OptionalTextPatch, value: &mut Option<String>) {
    match patch {
        OptionalTextPatch::Missing => {}
        OptionalTextPatch::Clear => {
            *value = None;
        }
        OptionalTextPatch::Set(next_value) => {
            *value = non_empty_request_text(&next_value);
        }
    }
}

/// Validates a fixed provider/model request against existing metadata and runtime-discovered models.
/// 根据既有元数据与运行时发现的模型校验固定供应商/模型请求。
pub(crate) fn validate_subagent_provider_model_request_with_existing(
    request: &SubagentDefinitionSaveRequest,
    existing: Option<&vulcan_task_session::SubagentDefinition>,
    public_ai_configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    public_ai_catalogs: &std::collections::BTreeMap<
        String,
        vulcan_provider_adapters::ConnectionModelCatalog,
    >,
) -> Result<vulcan_session_core::ProviderModelSnapshot, String> {
    let provider_id = request
        .provider_model
        .provider_id
        .as_deref()
        .and_then(non_empty_request_text);
    let model_id = request
        .provider_model
        .model_id
        .as_deref()
        .and_then(non_empty_request_text);
    let snapshot = match (provider_id, model_id) {
        (None, None) => vulcan_session_core::ProviderModelSnapshot::default(),
        (Some(provider_id), Some(model_id)) => vulcan_session_core::ProviderModelSnapshot {
            provider_id: Some(provider_id),
            model_id: Some(model_id),
            reasoning_effort: None,
            execution_mode: None,
        },
        _ => {
            return Err(String::from(
                "A pinned model needs both a provider and a model; to follow the session model instead, clear both.",
            ));
        }
    };
    let Some(provider_id) = snapshot.provider_id.as_deref() else {
        return Ok(snapshot);
    };
    let Some(model_id) = snapshot.model_id.as_deref() else {
        return Ok(snapshot);
    };
    if existing
        .map(|definition| subagent_provider_model_pair_matches(definition, provider_id, model_id))
        .unwrap_or(false)
    {
        return Ok(snapshot);
    }
    let available = public_ai_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .filter_map(|connection| public_ai_catalogs.get(connection.id.as_str()))
        .any(|catalog| catalog.models.iter().any(|model| model.id == model_id));
    if available {
        Ok(snapshot)
    } else {
        Err(format!(
            "Pinned model `{provider_id}/{model_id}` is not in the currently enabled model list; select an available model or follow the session model again."
        ))
    }
}

/// Returns whether one existing definition already carries the same provider/model pair.
/// 返回既有定义是否已经携带同一组供应商/模型。
fn subagent_provider_model_pair_matches(
    definition: &vulcan_task_session::SubagentDefinition,
    provider_id: &str,
    model_id: &str,
) -> bool {
    definition.provider_model.provider_id.as_deref() == Some(provider_id)
        && definition.provider_model.model_id.as_deref() == Some(model_id)
}

/// Applies the save request's unified tool policy to one exposure snapshot.
/// 将保存请求中的统一工具策略应用到可见性快照。
fn apply_request_tool_policy(
    snapshot: &mut vulcan_session_core::ToolExposureSnapshot,
    request: &SubagentDefinitionSaveRequest,
) {
    let policy = request_capability_policy(
        request.tool_policy_mode.as_deref(),
        request.tool_policy_tools.clone(),
        request.allowed_tools.clone(),
        request.disabled_tools.clone(),
    );
    vulcan_session_core::apply_tool_capability_policy(snapshot, policy);
    vulcan_session_core::sanitize_child_agent_tool_exposure(snapshot);
    snapshot.allowed_ai_capabilities.clear();
    snapshot.disabled_ai_capabilities.clear();
}

/// Applies the save request's unified prompt-skill policy to one snapshot.
/// 将保存请求中的统一提示词技能策略应用到快照。
fn apply_request_prompt_skill_policy(
    snapshot: &mut vulcan_session_core::PromptSkillSnapshot,
    request: &SubagentDefinitionSaveRequest,
) {
    let policy = request_capability_policy(
        request.prompt_skill_policy_mode.as_deref(),
        request.prompt_skill_policy_skills.clone(),
        request.enabled_prompt_skills.clone(),
        request.disabled_prompt_skills.clone(),
    );
    vulcan_session_core::apply_prompt_skill_capability_policy(snapshot, policy);
}

/// Builds a capability policy from new request fields or legacy allow/deny lists.
/// 从新版请求字段或旧版允许/禁止列表构建能力策略。
fn request_capability_policy(
    mode: Option<&str>,
    selectors: Vec<String>,
    legacy_allowed: Vec<String>,
    legacy_disabled: Vec<String>,
) -> vulcan_session_core::CapabilityPolicySnapshot {
    vulcan_session_core::capability_policy_from_mode_or_selector_lists(
        mode,
        selectors,
        legacy_allowed,
        legacy_disabled,
    )
}

/// Normalizes one package id for filesystem storage.
/// 规范化一个用于文件系统存储的包标识。
fn normalized_package_id(value: &str) -> Result<String, String> {
    let package_id = value.trim();
    if package_id.is_empty()
        || package_id == "."
        || package_id == ".."
        || package_id.contains('/')
        || package_id.contains('\\')
        || package_id.contains(':')
    {
        return Err(format!("Invalid agent package identifier: {value}"));
    }
    Ok(package_id.to_string())
}

/// Normalizes an optional user package id and rejects system package collisions.
/// 规范化可选用户包标识并拒绝与系统包冲突。
fn normalized_optional_user_package_id(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let package_id = normalized_package_id(value)?;
    if system_subagent_package_exists(&package_id) {
        return Err(format!(
            "A user-defined package cannot share the name of system package `{package_id}`."
        ));
    }
    Ok(Some(package_id))
}

/// Returns whether the application ships a system package/category with the id.
/// 返回应用是否内置了同名系统包或分类。
fn system_subagent_package_exists(package_id: &str) -> bool {
    let system_root = vulcan_runtime_assets::resolve_system_subagents_resource_directory();
    // packageId has already passed the path-separator guard, so the exact child path is authoritative.
    // packageId 已通过路径分隔符校验，因此精确子路径就是权威定位结果。
    system_root.join(package_id).is_dir()
}

/// Builds the target agent id for a package assignment or transfer.
/// 为包分配或转移构建目标智能体标识。
fn target_agent_id_for_save(
    requested_agent_id: &str,
    existing: Option<&vulcan_task_session::SubagentDefinition>,
    target_package_id: Option<&str>,
) -> Result<String, String> {
    let mut base_id = normalize_agent_file_name_input(requested_agent_id);
    if let Some(existing_package_id) = existing
        .and_then(|definition| definition.source.package_id.as_deref())
        .filter(|package_id| !package_id.is_empty())
    {
        let prefix = format!("{existing_package_id}/");
        if let Some(stripped) = base_id.strip_prefix(&prefix) {
            base_id = stripped.to_string();
        } else if let Some(stripped) = existing
            .map(|definition| definition.id.as_str())
            .and_then(|id| id.strip_prefix(&prefix))
        {
            base_id = stripped.to_string();
        }
    }
    if let Some(target_package_id) = target_package_id {
        let prefix = format!("{target_package_id}/");
        if let Some(stripped) = base_id.strip_prefix(&prefix) {
            base_id = stripped.to_string();
        }
        base_id = format!("{target_package_id}/{base_id}");
    }
    user_subagent_definition_path(&base_id)?;
    Ok(base_id)
}

/// Normalizes an editable agent file name into a stable definition id.
/// 将可编辑智能体文件名规范化为稳定定义标识。
fn normalize_agent_file_name_input(value: &str) -> String {
    // Keep one pending segment so only the final path segment loses its extension.
    // 保留一个待定片段，确保只有最后一个路径片段会移除扩展名。
    let mut normalized = String::new();
    let mut pending_segment = None;
    for segment in value
        .trim()
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
    {
        if let Some(previous_segment) = pending_segment.replace(segment) {
            push_agent_file_name_segment(&mut normalized, previous_segment);
        }
    }
    if let Some(last_segment) = pending_segment {
        push_agent_file_name_segment(
            &mut normalized,
            strip_agent_file_segment_extension(last_segment),
        );
    }
    normalized
}

/// Appends one normalized agent id segment to an existing slash-separated id.
/// 将单个规范化智能体标识片段追加到现有斜杠分隔标识。
fn push_agent_file_name_segment(normalized: &mut String, segment: &str) {
    if !normalized.is_empty() {
        normalized.push('/');
    }
    normalized.push_str(segment);
}

/// Removes the last extension-like suffix from one agent id path segment.
/// 从单个智能体标识路径片段移除最后一个扩展名形式的后缀。
fn strip_agent_file_segment_extension(segment: &str) -> &str {
    segment
        .rsplit_once('.')
        .filter(|(stem, _extension)| !stem.is_empty())
        .map(|(stem, _extension)| stem)
        .unwrap_or(segment)
}

/// Collects all possible user-layer files that can override one system agent definition.
/// 收集可能覆盖某个系统智能体定义的所有用户层文件。
fn restore_default_user_file_candidates(
    agent_id: &str,
    definition: &vulcan_task_session::SubagentDefinition,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    push_user_subagent_generated_files(&mut candidates, agent_id, true)?;
    push_user_subagent_source_files(
        &mut candidates,
        &definition.source,
        definition.source.scope == vulcan_task_session::SubagentDefinitionScope::User,
    );
    Ok(candidates)
}

/// Collects user-owned files that define one deletable user agent.
/// 收集定义单个可删除用户智能体的用户文件。
fn user_definition_delete_candidates(
    agent_id: &str,
    definition: &vulcan_task_session::SubagentDefinition,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    push_user_subagent_source_files(&mut candidates, &definition.source, true);
    push_user_subagent_generated_files(&mut candidates, agent_id, true)?;
    Ok(candidates)
}

/// Removes stale user-owned source files after the definition has moved to a new id.
/// 在定义移动到新标识后删除旧的用户来源文件。
fn remove_stale_user_subagent_files_for_source(
    previous_agent_id: &str,
    previous_source: &vulcan_task_session::SubagentDefinitionSource,
    retained_file: &Path,
) -> Result<(), String> {
    let candidates = stale_user_subagent_file_candidates(previous_agent_id, previous_source)?;
    for path in candidates {
        remove_user_subagent_file_if_safe(&path, retained_file)?;
    }
    Ok(())
}

/// Collects stale user-owned source files for one previous agent id.
/// 收集单个旧智能体标识对应的旧用户来源文件。
fn stale_user_subagent_file_candidates(
    previous_agent_id: &str,
    previous_source: &vulcan_task_session::SubagentDefinitionSource,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    push_user_subagent_source_files(&mut candidates, previous_source, true);
    push_user_subagent_generated_files(&mut candidates, previous_agent_id, false)?;
    Ok(candidates)
}

/// Pushes source-owned user agent files into one candidate list.
/// 将来源记录拥有的用户智能体文件推入候选列表。
fn push_user_subagent_source_files(
    candidates: &mut Vec<PathBuf>,
    source: &vulcan_task_session::SubagentDefinitionSource,
    include_primary_file: bool,
) {
    if include_primary_file {
        push_unique_path(candidates, source.file.clone());
    }
    if let Some(sidecar_file) = source.sidecar_file.as_ref() {
        push_unique_path(candidates, sidecar_file.clone());
    }
}

/// Pushes generated user agent files for one agent id into one candidate list.
/// 将单个智能体标识对应的派生用户文件推入候选列表。
fn push_user_subagent_generated_files(
    candidates: &mut Vec<PathBuf>,
    agent_id: &str,
    include_definition_file: bool,
) -> Result<(), String> {
    if include_definition_file {
        push_unique_path(candidates, user_subagent_definition_path(agent_id)?);
    }
    push_unique_path(candidates, user_subagent_sidecar_path(agent_id)?);
    push_unique_path(candidates, user_subagent_override_path(agent_id)?);
    Ok(())
}

/// Returns whether one existing file is inside the user agent root.
/// 返回一个已存在文件是否位于用户智能体根目录内。
fn user_subagent_file_exists_inside_root(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let user_root = user_subagent_root();
    let canonical_user_root = std::fs::canonicalize(&user_root).map_err(|error| {
        format!(
            "Failed to resolve the user agent root directory `{}`: {error}",
            user_root.display()
        )
    })?;
    let canonical_path = std::fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to resolve the agent user-layer file `{}`: {error}",
            path.display()
        )
    })?;
    Ok(canonical_path.starts_with(canonical_user_root))
}

/// Removes one file when it is an existing user-layer agent file.
/// 当文件是已存在的用户层智能体文件时删除它。
fn remove_user_subagent_file_if_inside_root(path: &Path) -> Result<(), String> {
    if !user_subagent_file_exists_inside_root(path)? {
        return Ok(());
    }
    std::fs::remove_file(path).map_err(|error| {
        format!(
            "Failed to delete the user agent file `{}`: {error}",
            path.display()
        )
    })
}

/// Pushes one filesystem path while preserving first occurrence order.
/// 按首次出现顺序推入单个文件系统路径。
fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

/// Removes one user catalog file only when it is still inside the user agent root.
/// 仅在文件仍位于用户智能体根目录内时删除单个用户目录文件。
fn remove_user_subagent_file_if_safe(path: &Path, retained_file: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let user_root = user_subagent_root();
    let canonical_user_root = std::fs::canonicalize(&user_root).map_err(|error| {
        format!(
            "Failed to resolve the user agent root directory `{}`: {error}",
            user_root.display()
        )
    })?;
    let canonical_path = std::fs::canonicalize(path).map_err(|error| {
        format!(
            "Failed to resolve the old agent file `{}`: {error}",
            path.display()
        )
    })?;
    if std::fs::canonicalize(retained_file)
        .ok()
        .as_ref()
        .is_some_and(|canonical_retained| canonical_retained == &canonical_path)
    {
        return Ok(());
    }
    if !canonical_path.starts_with(&canonical_user_root) {
        return Err(format!(
            "Refusing to delete an old file outside the user agent directory: {}",
            path.display()
        ));
    }
    std::fs::remove_file(path).map_err(|error| {
        format!(
            "Failed to delete the old agent file `{}`: {error}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies finalization commits terminal state and returns the same newer revision.
    /// 验证终结会提交终态，并返回相同的较新版本。
    #[test]
    #[ignore = "mutates the process-wide LuaSkills progress callback slot"]
    fn operation_finalization_cannot_return_or_retain_in_flight_state() {
        crate::luaskills_install_progress::shutdown_global();
        crate::luaskills_install_progress::install_global();
        // state is the authoritative reconnect snapshot modified by finalization.
        // state 是终结过程改写的权威重连快照。
        let state = std::sync::Arc::new(std::sync::Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(
                crate::SessionId::new_v4(),
            ),
        ));
        let app = ServiceEventEmitter::new(Vec::new());

        let succeeded_id = crate::luaskills_install_progress::begin_global_operation(
            luaskills::SkillLifecycleAction::Install,
        )
        .expect("installed progress owner should allocate an id");
        let in_flight =
            crate::luaskills_install_progress::global_operation_progress_view(&succeeded_id)
                .expect("in-flight progress");
        let original_revision = {
            let mut guard = state.lock().expect("service state should remain lockable");
            guard.workbench_generation = 7;
            guard.workbench.revision = 7;
            guard.workbench.lua_skills.operation_progress = Some(in_flight.clone());
            7
        };
        let returned = finalize_luaskills_operation_progress(
            &app,
            &state,
            Some(&succeeded_id),
            Ok(LuaSkillsCatalogUpdate {
                revision: original_revision,
                lua_skills: vulcan_service_api::LuaSkillsCatalogSummary {
                    operation_progress: Some(in_flight),
                    ..vulcan_service_api::LuaSkillsCatalogSummary::default()
                },
                plugins: Vec::new(),
            }),
        )
        .expect("successful operation should remain successful");
        let retained = state
            .lock()
            .expect("service state should remain lockable")
            .workbench
            .lua_skills
            .operation_progress
            .clone()
            .expect("terminal progress should be retained");
        assert!(!retained.in_flight);
        assert_eq!(retained.outcome.as_deref(), Some("succeeded"));
        assert_eq!(returned.lua_skills.operation_progress, Some(retained));
        assert!(returned.revision > original_revision);

        let failed_id = crate::luaskills_install_progress::begin_global_operation(
            luaskills::SkillLifecycleAction::Update,
        )
        .expect("second operation should allocate an id");
        let primary_error = String::from("fixture update failure");
        let returned_error = finalize_luaskills_operation_progress(
            &app,
            &state,
            Some(&failed_id),
            Err(primary_error.clone()),
        )
        .expect_err("failed operation should preserve its primary error");
        assert_eq!(returned_error, primary_error);
        let failed = state
            .lock()
            .expect("service state should remain lockable")
            .workbench
            .lua_skills
            .operation_progress
            .clone()
            .expect("failed terminal progress should be retained");
        assert_eq!(failed.correlation_id, failed_id);
        assert!(!failed.in_flight);
        assert_eq!(failed.outcome.as_deref(), Some("failed"));
        crate::luaskills_install_progress::shutdown_global();
    }

    /// Builds one permission preview containing two exact local MCP Secret requirements.
    /// 构建一份包含两个精确本地 MCP Secret 要求的权限预览。
    ///
    /// # Returns
    /// 返回值
    /// Returns a deterministic preview suitable for installation-input validation tests.
    /// 返回适用于安装输入校验测试的确定性预览。
    fn local_mcp_secret_preview() -> vulcan_plugin_runtime::VulcanPluginPermissionPreview {
        vulcan_plugin_runtime::VulcanPluginPermissionPreview {
            plugin_id: String::from("fixture-plugin"),
            version: String::from("1.0.0"),
            content_fingerprint: String::from("fixture-fingerprint"),
            origin: String::from("user_installed"),
            permissions: Vec::new(),
            hook_points: Vec::new(),
            ui_mount_points: Vec::new(),
            binary_ids: Vec::new(),
            node_entry_ids: Vec::new(),
            python_entry_ids: Vec::new(),
            skill_ids: Vec::new(),
            agent_ids: Vec::new(),
            has_lua_runtime: false,
            mcp_servers: Vec::new(),
            install_secret_requirements: vec![
                vulcan_plugin_runtime::VulcanPluginInstallSecretRequirement {
                    server_id: String::from("alpha"),
                    secret_key: String::from("TOKEN"),
                },
                vulcan_plugin_runtime::VulcanPluginInstallSecretRequirement {
                    server_id: String::from("beta"),
                    secret_key: String::from("API_KEY"),
                },
            ],
        }
    }

    /// Verifies local MCP installation Secrets require an exact, complete, duplicate-free allowlist match.
    /// 验证本地 MCP 安装 Secret 必须与白名单精确、完整且无重复地匹配。
    #[test]
    fn initial_plugin_mcp_secrets_require_exact_complete_inputs() {
        // preview is the sole host-derived authorization source used by the validator.
        // preview 是校验器使用的唯一宿主派生授权来源。
        let preview = local_mcp_secret_preview();
        let values = validate_initial_plugin_mcp_secrets(
            &preview,
            vec![
                VulcanPluginInitialSecretInput {
                    server_id: String::from("beta"),
                    secret_key: String::from("API_KEY"),
                    value: String::from("beta-value"),
                },
                VulcanPluginInitialSecretInput {
                    server_id: String::from("alpha"),
                    secret_key: String::from("TOKEN"),
                    value: String::from("alpha-value"),
                },
            ],
        )
        .expect("accept complete exact secret inputs");
        assert_eq!(values.len(), 2);

        let missing_error = validate_initial_plugin_mcp_secrets(
            &preview,
            vec![VulcanPluginInitialSecretInput {
                server_id: String::from("alpha"),
                secret_key: String::from("TOKEN"),
                value: String::from("alpha-value"),
            }],
        )
        .expect_err("reject missing local MCP secret");
        assert!(missing_error.contains("beta:API_KEY"));

        let undeclared_error = validate_initial_plugin_mcp_secrets(
            &preview,
            vec![VulcanPluginInitialSecretInput {
                server_id: String::from("alpha"),
                secret_key: String::from("UNDECLARED"),
                value: String::from("value"),
            }],
        )
        .expect_err("reject undeclared local MCP secret");
        assert!(undeclared_error.contains("undeclared"));

        let duplicate_error = validate_initial_plugin_mcp_secrets(
            &preview,
            vec![
                VulcanPluginInitialSecretInput {
                    server_id: String::from("alpha"),
                    secret_key: String::from("TOKEN"),
                    value: String::from("first"),
                },
                VulcanPluginInitialSecretInput {
                    server_id: String::from("alpha"),
                    secret_key: String::from("TOKEN"),
                    value: String::from("second"),
                },
            ],
        )
        .expect_err("reject duplicate local MCP secret");
        assert!(duplicate_error.contains("more than once"));
    }

    /// Loads one isolated content-plugin catalog for service-side ownership rejection tests.
    /// 为服务端所有权拒绝测试加载一个隔离的内容插件目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the temporary root and its fully validated immutable plugin catalog.
    /// 返回临时根目录及其完整校验后的不可变插件目录。
    fn test_content_plugin_catalog() -> (PathBuf, vulcan_plugin_runtime::VulcanPluginCatalogSnapshot)
    {
        // root is unique per test process and is removed explicitly by each caller.
        // root 在每个测试进程内唯一，并由各调用方显式删除。
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-readonly-test-{}",
            uuid::Uuid::new_v4()
        ));
        // package is the only plugin package in the isolated catalog.
        // package 是隔离目录中的唯一插件包。
        let package = root.join("content-plugin");
        std::fs::create_dir_all(package.join("skills/review-helper"))
            .expect("create plugin skill directory");
        std::fs::create_dir_all(package.join("agents")).expect("create plugin agent directory");
        std::fs::write(
            package.join("skills/review-helper/SKILL.md"),
            "---\nname: review-helper\ndescription: Review one change.\n---\n\nReview the exact diff.",
        )
        .expect("write plugin skill");
        std::fs::write(
            package.join("agents/reviewer.md"),
            "---\nname: Reviewer\nskills:\n  - review-helper\n---\nReview the exact diff.",
        )
        .expect("write plugin agent");
        std::fs::write(
            package.join("plugin.yaml"),
            "schema_version: 1\nid: content-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Content Plugin\n  capability: Read-only fixture\n",
        )
        .expect("write plugin manifest");
        // catalog is the same validated snapshot consumed by production rejection helpers.
        // catalog 是生产拒绝辅助函数使用的同类校验快照。
        let catalog = vulcan_plugin_runtime::load_vulcan_plugin_catalog(
            &root,
            None,
            &BTreeSet::new(),
            &BTreeSet::new(),
            1,
        );
        (root, catalog)
    }

    /// Verifies forged skill mutation identities are rejected by the service ownership boundary.
    /// 验证伪造的 Skill 变更标识会被服务所有权边界拒绝。
    #[test]
    fn forged_plugin_skill_mutations_are_rejected_server_side() {
        let (root, catalog) = test_content_plugin_catalog();

        assert!(
            reject_plugin_skill_mutation_in_catalog(&catalog, "content-plugin:review-helper")
                .expect_err("canonical plugin skill mutation must fail")
                .contains("read-only")
        );
        assert!(
            reject_plugin_skill_mutation_in_catalog(
                &catalog,
                "plugin:content-plugin:review-helper"
            )
            .expect_err("source-aware plugin skill mutation must fail")
            .contains("content-plugin")
        );
        assert!(reject_plugin_skill_mutation_in_catalog(&catalog, "ordinary-skill").is_ok());
        assert!(
            reject_plugin_skill_mutation_in_catalog(&catalog, "unknown-plugin:review-helper")
                .expect_err("unknown plugin skill namespace must fail closed")
                .contains("not present in the committed plugin catalog")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies forged agent mutation identities are rejected by the service ownership boundary.
    /// 验证伪造的 Agent 变更标识会被服务所有权边界拒绝。
    #[test]
    fn forged_plugin_agent_mutations_are_rejected_server_side() {
        let (root, catalog) = test_content_plugin_catalog();

        assert!(
            reject_plugin_agent_mutation_in_catalog(&catalog, "content-plugin:reviewer")
                .expect_err("plugin agent mutation must fail")
                .contains("read-only")
        );
        assert!(reject_plugin_agent_mutation_in_catalog(&catalog, "ordinary-agent").is_ok());
        assert!(
            reject_plugin_agent_mutation_in_catalog(&catalog, "unknown-plugin:reviewer")
                .expect_err("unknown plugin agent namespace must fail closed")
                .contains("not present in the committed plugin catalog")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies prompts are served by agent id and unknown ids fail instead of returning empty prompts.
    /// 验证提示词按智能体标识提供，未知标识报错而不是返回空提示词。
    #[test]
    fn subagent_definition_prompts_resolve_by_agent_id() {
        let mut definition = test_user_subagent_definition();
        definition.system_prompt = Some(String::from("base"));
        definition.system_prompt_append = Some(String::from("append"));
        let catalog = vulcan_task_session::SubagentCatalog {
            definitions: vec![definition],
            ..Default::default()
        };

        let prompts = subagent_definition_prompts_from_catalog(&catalog, "engineering/test-agent")
            .expect("known agent prompts");
        assert_eq!(prompts.agent_id, "engineering/test-agent");
        assert_eq!(prompts.system_prompt.as_deref(), Some("base"));
        assert_eq!(prompts.system_prompt_append.as_deref(), Some("append"));
        assert!(subagent_definition_prompts_from_catalog(&catalog, "missing").is_err());
    }

    /// Builds one minimal user subagent definition for file-candidate tests.
    /// 为文件候选测试构建一个最小用户智能体定义。
    fn test_user_subagent_definition() -> vulcan_task_session::SubagentDefinition {
        vulcan_task_session::SubagentDefinition {
            id: String::from("engineering/test-agent"),
            name: String::from("Test Agent"),
            description: None,
            locales: std::collections::BTreeMap::new(),
            origin: None,
            emoji: String::from("AI"),
            color: String::from("gray"),
            vibe: None,
            mode: vulcan_task_session::SubagentDefinitionMode::Expert,
            system_prompt: None,
            system_prompt_append: None,
            provider_model: Default::default(),
            tool_exposure: Default::default(),
            prompt_skills: Default::default(),
            context_compaction: Default::default(),
            runtime_policy: Default::default(),
            max_depth: None,
            enabled: true,
            source: vulcan_task_session::SubagentDefinitionSource {
                scope: vulcan_task_session::SubagentDefinitionScope::User,
                file: std::path::PathBuf::from("custom/test-agent.md"),
                sidecar_file: Some(std::path::PathBuf::from("custom/test-agent.sidecar.json")),
                source_key: String::from("user:engineering/test-agent"),
                category_id: Some(String::from("engineering")),
                package_id: None,
                remote_url: None,
                override_of: None,
                source_hash: None,
                version: None,
                etag: None,
                last_modified: None,
                updated_at: None,
            },
            extra: serde_json::Value::Null,
        }
    }

    /// Verifies delete candidates include source and generated definition files.
    /// 验证删除候选会包含来源文件与派生定义文件。
    #[test]
    fn user_definition_delete_candidates_include_source_and_generated_files() {
        let definition = test_user_subagent_definition();
        let candidates = user_definition_delete_candidates("engineering/test-agent", &definition)
            .expect("delete candidates should resolve");

        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.md")));
        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.sidecar.json")));
        assert!(
            candidates.contains(
                &user_subagent_definition_path("engineering/test-agent")
                    .expect("definition path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_sidecar_path("engineering/test-agent")
                    .expect("sidecar path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_override_path("engineering/test-agent")
                    .expect("override path should resolve")
            )
        );
    }

    /// Verifies editable agent file names normalize separators and strip the final extension.
    /// 验证可编辑智能体文件名会归一化分隔符并移除最终扩展名。
    #[test]
    fn normalize_agent_file_name_input_normalizes_separators_and_final_extension() {
        // editable_name mirrors a Windows-style file path submitted from the WebUI.
        // editable_name 模拟 WebUI 提交的 Windows 风格文件路径。
        let editable_name = r"  engineering\reviewer.md  ";

        assert_eq!(
            normalize_agent_file_name_input(editable_name),
            "engineering/reviewer"
        );
        assert_eq!(
            normalize_agent_file_name_input("//engineering//reviewer.sidecar.json"),
            "engineering/reviewer.sidecar"
        );
    }

    /// Verifies agent file-name normalization preserves non-final segment text.
    /// 验证智能体文件名归一化会保留非最终片段文本。
    #[test]
    fn normalize_agent_file_name_input_preserves_non_final_segments() {
        // dotted_category proves only the last path segment is treated as a file name.
        // dotted_category 证明只有最后一个路径片段会按文件名处理。
        let dotted_category = "team.v1/reviewer.md";

        assert_eq!(
            normalize_agent_file_name_input(dotted_category),
            "team.v1/reviewer"
        );
        assert_eq!(normalize_agent_file_name_input(".md"), ".md");
    }

    /// Verifies saved tool policies drop tools that child agents may never use.
    /// 验证保存的工具策略会移除智能体绝不能使用的工具。
    #[test]
    fn apply_request_tool_policy_sanitizes_child_forbidden_tools() {
        let request = SubagentDefinitionSaveRequest {
            tool_policy_mode: Some(String::from("allow_only")),
            tool_policy_tools: vec![
                String::from("shell_command"),
                String::from("todolist"),
                String::from("ask"),
            ],
            ..SubagentDefinitionSaveRequest::default()
        };
        let mut snapshot = vulcan_session_core::ToolExposureSnapshot::default();

        apply_request_tool_policy(&mut snapshot, &request);

        assert_eq!(
            snapshot.policy.mode,
            vulcan_session_core::CapabilityPolicyMode::AllowOnly
        );
        assert_eq!(
            snapshot.policy.selectors,
            vec![String::from("shell_command")]
        );
        assert_eq!(snapshot.allowed_tools, vec![String::from("shell_command")]);
        assert!(snapshot.disabled_tools.is_empty());
    }

    /// Verifies stale cleanup candidates keep source files but skip generated definitions.
    /// 验证旧文件清理候选保留来源文件但跳过派生定义文件。
    #[test]
    fn stale_user_subagent_file_candidates_skip_generated_definition_file() {
        let definition = test_user_subagent_definition();
        let candidates =
            stale_user_subagent_file_candidates("engineering/test-agent", &definition.source)
                .expect("stale candidates should resolve");

        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.md")));
        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.sidecar.json")));
        assert!(
            !candidates.contains(
                &user_subagent_definition_path("engineering/test-agent")
                    .expect("definition path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_sidecar_path("engineering/test-agent")
                    .expect("sidecar path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_override_path("engineering/test-agent")
                    .expect("override path should resolve")
            )
        );
    }

    /// Verifies package configuration input projection accepts only the four official scalar forms.
    /// 验证技能包配置输入投影仅接受四种正式标量形式。
    #[test]
    fn luaskill_config_input_projection_accepts_only_official_scalars() {
        assert!(matches!(
            project_luaskill_config_input(json!("value")).expect("string input"),
            luaskills::SkillPackageConfigInputValue::String(value) if value == "value"
        ));
        assert!(matches!(
            project_luaskill_config_input(json!(42)).expect("integer input"),
            luaskills::SkillPackageConfigInputValue::Integer(42)
        ));
        assert!(matches!(
            project_luaskill_config_input(json!(1.5)).expect("float input"),
            luaskills::SkillPackageConfigInputValue::Float(value) if value == 1.5
        ));
        assert!(matches!(
            project_luaskill_config_input(json!(true)).expect("boolean input"),
            luaskills::SkillPackageConfigInputValue::Boolean(true)
        ));
        assert!(project_luaskill_config_input(json!({"value": "secret"})).is_err());
        assert!(project_luaskill_config_input(json!(["secret"])).is_err());
        assert!(project_luaskill_config_input(serde_json::Value::Null).is_err());
    }

    /// Verifies the WebUI event projection contains metadata but no configuration values.
    /// 验证 WebUI 事件投影包含元数据但不包含配置值。
    #[test]
    fn luaskill_config_event_projection_is_value_free() {
        let projected =
            project_luaskill_config_event_batch(luaskills::RuntimeSkillConfigEventBatch {
                events: vec![luaskills::RuntimeSkillConfigEvent {
                    sequence: String::from("7"),
                    event_type: String::from("config_changed"),
                    store_scope: String::from("skills"),
                    skill_id: Some(String::from("fixture")),
                    revision: String::from("8"),
                    changed_keys: vec![String::from("api_token")],
                    source: String::from("local_write"),
                    restart_required_keys: Vec::new(),
                    complete: Some(true),
                    error: None,
                }],
                next_sequence: String::from("7"),
            });
        let json = serde_json::to_value(projected).expect("serialize event projection");

        assert_eq!(json["events"][0]["changedKeys"], json!(["api_token"]));
        assert!(json["events"][0].get("value").is_none());
        assert!(json["events"][0].get("values").is_none());
    }

    /// Verifies expired event cursors recover only from the exact LuaSkills diagnostic contract.
    /// 验证过期事件游标仅从精确的 LuaSkills 诊断契约恢复。
    #[test]
    fn luaskill_config_event_cursor_recovery_is_strict() {
        assert_eq!(
            expired_luaskill_config_event_recovery_cursor(
                "CONFIG_EVENT_CURSOR_EXPIRED: event cursor 2 precedes the oldest retained sequence 9"
            )
            .as_deref(),
            Some("8")
        );
        assert!(
            expired_luaskill_config_event_recovery_cursor(
                "CONFIG_EVENT_CURSOR_INVALID: event cursor 9 is newer than the current sequence 2"
            )
            .is_none()
        );
        assert!(
            expired_luaskill_config_event_recovery_cursor(
                "CONFIG_EVENT_CURSOR_EXPIRED: event cursor 2 precedes the oldest retained sequence invalid"
            )
            .is_none()
        );
    }

    /// Verifies write-result projection removes sensitive canonical values and reports restart keys.
    /// 验证写入结果投影会移除敏感规范值并报告重启键。
    #[test]
    fn luaskill_config_write_projection_is_host_safe() {
        // result is decoded through the upstream public wire contract because its durability field is intentionally private.
        // result 通过上游公开线协议解码，因为其耐久化字段刻意保持私有。
        let result: luaskills::SkillConfigWriteResult = serde_json::from_value(json!({
            "revision": "4",
            "changed": true,
            "values": {
                "api_token": "secret-value",
                "endpoint": "https://example.com"
            },
            "changed_keys": ["api_token", "endpoint"]
        }))
        .expect("decode upstream write result");
        // descriptor is the authoritative post-write declaration snapshot used for disclosure.
        // descriptor 是用于披露判断的权威写入后声明快照。
        let descriptor: luaskills::RuntimeSkillPackageConfigDescriptor =
            serde_json::from_value(json!({
                "skill_id": "fixture",
                "skill_version": "1.0.0",
                "complete": true,
                "revision": "4",
                "store_scope": "skills",
                "missing_count": 0,
                "invalid_count": 0,
                "business_issue_count": 0,
                "orphaned_count": 0,
                "orphaned": [],
                "items": [
                    {
                        "key": "api_token",
                        "type": "string",
                        "required": true,
                        "sensitive": true,
                        "description": "secret",
                        "constraints": {},
                        "options": [],
                        "advanced": false,
                        "restart_required": true,
                        "deprecated": false,
                        "state": "configured",
                        "satisfied": true
                    },
                    {
                        "key": "endpoint",
                        "type": "string",
                        "required": true,
                        "sensitive": false,
                        "description": "endpoint",
                        "constraints": {},
                        "options": [],
                        "advanced": false,
                        "restart_required": false,
                        "deprecated": false,
                        "state": "configured",
                        "satisfied": true,
                        "value": "https://example.com"
                    }
                ]
            }))
            .expect("decode upstream descriptor");

        let projected =
            project_luaskill_config_set_result(result, descriptor).expect("project write result");
        assert_eq!(
            projected.values.get("endpoint").map(String::as_str),
            Some("https://example.com")
        );
        assert!(!projected.values.contains_key("api_token"));
        assert_eq!(projected.restart_required_keys, vec!["api_token"]);
    }

    /// Preserve the exact test-only destination inside a detached service task and persist through the real bounded writer.
    /// 在已分离服务任务内保留精确测试专用目标，并经真实受限写入器持久化。
    /// No parameters or return value; verifies a cancelled outer scoped waiter cannot redirect its still-owned save.
    /// 无参数或返回值；验证取消外层作用域等待者不能重定向仍被拥有的保存。
    #[tokio::test]
    async fn scoped_management_persistence_survives_waiter_cancellation() {
        // One bounded observation deadline is shared by both synchronization points in this test.
        // 此测试的两个同步点共用单个有界观测期限。
        const CONFIRMATION_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
        // Private storage outlives the actual detached operation and all file assertions.
        // 私有存储比实际分离操作及全部文件断言存活更久。
        let storage = tempfile::tempdir().expect("private detached persistence root");
        // Only this explicitly installed destination may receive the real configuration save.
        // 仅此明确安装目标可以接收真实配置保存。
        let destination = storage.path().join("owned-policy.json");
        // A separate owned copy survives destruction of the outer task-local scope.
        // 独立拥有副本跨外层任务局部作用域销毁存续。
        let write_destination = destination.clone();
        // Entry notification proves the service task acquired its actual management guard before cancellation.
        // 入场通知证明取消之前服务任务取得了实际管理守卫。
        let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
        // The real write is delayed until the caller and its outer task-local scope are gone.
        // 真实写入延迟至调用方及其外层任务局部作用域消失之后。
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        // Completion carries the actual retained destination after successful real persistence.
        // 完成通知在真实持久化成功之后携带实际保留目标。
        let (finished_sender, finished_receiver) = tokio::sync::oneshot::channel();
        // Scope the future inside its own spawned caller; Tokio peer tasks do not inherit a parent's task-local value.
        // 在其自身派生调用方内设置协程作用域；Tokio 其他任务不继承父任务局部值。
        let waiter = tokio::spawn(
            crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH
                .scope(destination.clone(), async move {
                    run_luaskills_management_owned("private detached persistence", move || async move {
                        entered_sender.send(()).map_err(|_| String::from("entry receiver closed"))?;
                        release_receiver.await.map_err(|_| String::from("release sender closed"))?;
                        // Prove the destination before invoking the writer so an old defective implementation cannot touch user files.
                        // 调用写入器之前证明目标，避免旧缺陷实现触及用户文件。
                        let actual = crate::tests::provider_configuration_persistence_scope::current_path();
                        if actual.as_ref() != Some(&write_destination) {
                            return Err(String::from("Owned management task lost its exact private save destination."));
                        }
                        // This unique harmless membership makes the actual saved document independently identifiable.
                        // 此唯一无害成员关系使实际保存文档可以独立识别。
                        let mut configuration = vulcan_provider_config::config::ProviderConfigurationFile::default();
                        configuration.luaskills.disabled_vulcan_plugins.insert(String::from("owned-scope-proof"));
                        // Use the same transaction and bounded atomic writer as the production persistence path.
                        // 使用与生产持久化路径相同的事务及受限原子写入器。
                        let transaction = crate::service_state_access::begin_provider_configuration_transaction().await;
                        transaction.persist(&configuration, "private detached persistence")?;
                        finished_sender.send(actual).map_err(|_| String::from("completion receiver closed"))?;
                        Ok(())
                    }).await
                }),
        );
        tokio::time::timeout(CONFIRMATION_WAIT, entered_receiver)
            .await
            .expect("owned task must enter")
            .expect("actual entry notification");
        waiter.abort();
        assert!(
            waiter
                .await
                .expect_err("outer observer is cancelled")
                .is_cancelled()
        );
        release_sender
            .send(())
            .expect("actual owner remains alive after observer cancellation");
        assert_eq!(
            tokio::time::timeout(CONFIRMATION_WAIT, finished_receiver)
                .await
                .expect("actual private persistence must finish")
                .expect("actual completion notification"),
            Some(destination.clone())
        );
        // Parse the actual file produced by the real writer rather than counting a callback or trusting a path projection.
        // 解析真实写入器产出的实际文件，不计数回调或信任路径投影。
        let saved: vulcan_provider_config::config::ProviderConfigurationFile =
            serde_json::from_slice(
                &std::fs::read(&destination).expect("actual private persisted file"),
            )
            .expect("actual private configuration document");
        assert!(
            saved
                .luaskills
                .disabled_vulcan_plugins
                .contains("owned-scope-proof")
        );
        assert!(crate::tests::provider_configuration_persistence_scope::current_path().is_none());
    }

    /// Verifies an aborted RPC waiter cannot release a live management transaction.
    /// 验证被中止的 RPC 等待者无法释放仍在运行的管理事务。
    ///
    /// The first caller is aborted only after its service-owned child has acquired the
    /// global lock. A second transaction must remain queued until the first child exits,
    /// proving that `spawn_blocking` work and post-mutation projection cannot overlap a
    /// successor merely because the transport stopped waiting.
    /// 第一个调用方只在其服务自有子任务取得全局锁后被中止。第二个事务必须持续排队，直到第一
    /// 个子任务退出，从而证明传输层停止等待不会让 `spawn_blocking` 工作及后置投影与后继事务重叠。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn aborted_waiter_does_not_release_management_ownership() {
        let (first_started_sender, first_started_receiver) = tokio::sync::oneshot::channel();
        let first_release = Arc::new(tokio::sync::Notify::new());
        let first_release_for_task = Arc::clone(&first_release);
        let (first_done_sender, first_done_receiver) = tokio::sync::oneshot::channel();

        let first_waiter = tokio::spawn(run_luaskills_management_owned(
            "first cancellation fixture",
            move || async move {
                first_started_sender
                    .send(())
                    .map_err(|_| String::from("first start receiver closed"))?;
                first_release_for_task.notified().await;
                first_done_sender
                    .send(())
                    .map_err(|_| String::from("first completion receiver closed"))?;
                Ok(())
            },
        ));
        first_started_receiver
            .await
            .expect("first transaction must acquire the lock");
        first_waiter.abort();
        assert!(
            first_waiter
                .await
                .expect_err("waiter must be aborted")
                .is_cancelled(),
            "the outer waiter cancellation is the condition under test"
        );

        let (second_started_sender, mut second_started_receiver) = tokio::sync::oneshot::channel();
        let second_waiter = tokio::spawn(run_luaskills_management_owned(
            "second cancellation fixture",
            move || async move {
                second_started_sender
                    .send(())
                    .map_err(|_| String::from("second start receiver closed"))?;
                Ok(())
            },
        ));
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                &mut second_started_receiver,
            )
            .await
            .is_err(),
            "the second transaction must remain queued behind the detached first owner"
        );

        first_release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(5), first_done_receiver)
            .await
            .expect("first transaction must finish after release")
            .expect("first transaction completion channel");
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            &mut second_started_receiver,
        )
        .await
        .expect("second transaction must acquire after first exits")
        .expect("second transaction start channel");
        second_waiter
            .await
            .expect("join second waiter")
            .expect("second transaction result");
    }
}
