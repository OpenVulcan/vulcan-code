//! LuaSkills and AgentSkills service handlers.
//! LuaSkills 与 AgentSkills 服务处理器。

use super::*;

pub(crate) async fn run_luaskills_management_blocking<T, F>(
    label: &'static str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    spawn_service_blocking_task(operation)
        .await
        .map_err(|error| format!("LuaSkills 管理操作 `{label}` 任务异常结束：{error}"))?
}

/// Refreshes the LuaSkills catalog used by the WebUI skill page.
/// 刷新 WebUI 技能页面使用的 LuaSkills 目录。
pub(crate) async fn service_refresh_luaskills_catalog(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let catalog = run_luaskills_management_blocking("refresh catalog", move || {
        vulcan_service_runtime::luaskills_bridge::refresh_luaskills_catalog(&handle)
    })
    .await?;
    store_luaskills_catalog(&state, catalog)?;
    refresh_workbench_snapshot(state).await
}

/// Installs one LuaSkill package into the USER layer and refreshes the WebUI snapshot.
/// 将单个 LuaSkill 包安装到 USER 层并刷新 WebUI 快照。
pub(crate) async fn service_install_luaskill(
    state: SharedServiceBridgeState,
    skill_id: Option<String>,
    source: String,
    source_type: String,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("install skill", move || {
        vulcan_service_runtime::luaskills_bridge::install_luaskill(
            &handle,
            skill_id,
            source,
            source_type,
        )
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Updates one managed LuaSkill package and refreshes the WebUI snapshot.
/// 更新单个受管 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_update_luaskill(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("update skill", move || {
        vulcan_service_runtime::luaskills_bridge::update_luaskill(&handle, skill_id)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Enables or disables one LuaSkill package and refreshes the WebUI snapshot.
/// 启用或停用单个 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_set_luaskill_enabled(
    state: SharedServiceBridgeState,
    skill_id: String,
    enabled: bool,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("set skill enabled", move || {
        vulcan_service_runtime::luaskills_bridge::set_luaskill_enabled(&handle, skill_id, enabled)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Uninstalls one ordinary LuaSkill package and refreshes the WebUI snapshot.
/// 卸载单个普通 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_uninstall_luaskill(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("uninstall skill", move || {
        vulcan_service_runtime::luaskills_bridge::uninstall_luaskill(&handle, skill_id)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_workbench_snapshot(state).await
}

/// Persists one LuaSkill tool registration state and refreshes the WebUI snapshot.
/// 持久化单个 LuaSkill 工具注册状态并刷新 WebUI 快照。
pub(crate) async fn service_set_luaskill_tool_registration_state(
    state: SharedServiceBridgeState,
    skill_id: String,
    local_name: String,
    registration_state: String,
) -> Result<WorkbenchSnapshot, String> {
    let next_state = parse_luaskill_tool_registration_state(&registration_state)?;
    mutate_provider_configuration(&state, |configuration| {
        configuration
            .luaskills
            .set_tool_registration_state(&skill_id, &local_name, next_state);
        Ok(())
    })?;
    refresh_luaskills_local_state(&state)
}

/// Refreshes Agent Skills by rebuilding the current WebUI workbench snapshot.
/// 通过重建当前 WebUI 工作台快照刷新 Agent Skills。
pub(crate) async fn service_refresh_agent_skills_catalog(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    refresh_workbench_snapshot(state).await
}

/// Persists the Agent Skills global switch and refreshes only the Agent Skills catalog.
/// 持久化 Agent Skills 全局开关并仅刷新 Agent Skills 目录。
pub(crate) async fn service_set_agent_skills_enabled(
    state: SharedServiceBridgeState,
    enabled: bool,
) -> Result<AgentSkillsCatalogSummary, String> {
    mutate_provider_configuration(&state, |configuration| {
        configuration.prompt_skills.enabled = enabled;
        Ok(())
    })?;
    refresh_agent_skills_local_state(&state)
}

/// Persists one global Agent Skill state and refreshes only the Agent Skills catalog.
/// 持久化单个全局 Agent Skill 状态并仅刷新 Agent Skills 目录。
pub(crate) async fn service_set_agent_skill_enabled(
    state: SharedServiceBridgeState,
    name: String,
    enabled: bool,
) -> Result<AgentSkillsCatalogSummary, String> {
    let state_override = if enabled {
        PersistedPromptSkillState::Enabled
    } else {
        PersistedPromptSkillState::Disabled
    };
    mutate_provider_configuration(&state, |configuration| {
        configuration
            .prompt_skills
            .set_skill_state(&name, Some(state_override));
        Ok(())
    })?;
    refresh_agent_skills_local_state(&state)
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

/// Persists one project-level Agent Skill tri-state override without rebuilding the full snapshot.
/// 持久化单个项目级 Agent Skill 三态覆盖，但不重建完整快照。
pub(crate) fn service_set_agent_skill_project_state(
    state: SharedServiceBridgeState,
    project_id: String,
    name: String,
    state_key: String,
) -> Result<(), String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let mut project_configuration = load_project_configuration(&project_path);
    let stable_name = name.trim();
    if stable_name.is_empty() {
        return Err(String::from("项目级 Agent Skill 名称不能为空。"));
    }
    match parse_agent_skill_project_state(&state_key)? {
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
                .remove(stable_name);
        }
    }
    vulcan_provider_config::config::save_project_configuration_file(
        &PathBuf::from(&project_path),
        &project_configuration,
    )?;
    Ok(())
}
