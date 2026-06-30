//! service state access helpers.
//! service state access 辅助逻辑。

use super::*;

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
pub(crate) fn current_workspace_snapshot_from_shared_state(
    state: &SharedServiceBridgeState,
) -> Result<Option<WorkspaceSnapshot>, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workspace_snapshot.clone())
}

/// Clones the latest in-memory workbench snapshot retained by the service bridge.
/// 克隆服务桥接层保留的最近内存工作台快照。
///
/// # Parameters
/// 参数
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
///
/// # Returns
/// 返回值
/// The latest workbench snapshot already emitted by an in-memory mutation.
/// 返回内存变更已经发出的最近工作台快照。
pub(crate) fn current_workbench_snapshot_from_state(
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench.clone())
}

/// Applies one pinned-state mutation to retained project snapshots.
/// 将单个置顶状态变更应用到保留的项目快照。
pub(crate) fn update_project_pinned_local_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut found = false;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        for project in &mut workspace_snapshot.task_projects {
            if project.id == project_id {
                project.pinned = !project.pinned;
                found = true;
            }
        }
    }
    for project in &mut guard.workbench.projects {
        if project.id == project_id {
            project.pinned = !project.pinned;
            found = true;
        }
    }
    if found {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        Ok(())
    } else {
        Err(format!("没有找到要切换置顶状态的项目：{project_id}"))
    }
}

/// Applies one icon mutation to retained project snapshots.
/// 将单个图标变更应用到保留的项目快照。
pub(crate) fn update_project_icon_local_state(
    state: &SharedServiceBridgeState,
    project_id: &str,
    icon_pack: Option<String>,
    icon_key: Option<String>,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mut found = false;
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        for project in &mut workspace_snapshot.task_projects {
            if project.id == project_id {
                project.icon_pack = icon_pack.clone();
                project.icon_key = icon_key.clone();
                found = true;
            }
        }
    }
    for project in &mut guard.workbench.projects {
        if project.id == project_id {
            project.icon_pack = icon_pack.clone();
            project.icon_key = icon_key.clone();
            found = true;
        }
    }
    if found {
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        Ok(())
    } else {
        Err(format!("没有找到要设置图标的项目：{project_id}"))
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
        engine.list_entries_for_authority(
            vulcan_service_runtime::luaskills::SkillManagementAuthority::System,
        )
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

/// Mutates, persists, and stores provider configuration for WebUI skill commands.
/// 为 WebUI 技能命令变更、持久化并存储 Provider 配置。
pub(crate) fn mutate_provider_configuration<F>(
    state: &SharedServiceBridgeState,
    mutate: F,
) -> Result<(), String>
where
    F: FnOnce(&mut ProviderConfigurationFile) -> Result<(), String>,
{
    let mut configuration = provider_configuration_from_state(state)?;
    mutate(&mut configuration)?;
    vulcan_provider_config::config::save_configuration_file(&configuration)?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_mut() {
        runtime.provider_configuration = configuration;
    }
    Ok(())
}
