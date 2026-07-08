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

/// Refreshes the agent catalog and updates the shared task runtime snapshot.
/// 刷新智能体目录并更新共享任务运行时快照。
pub(crate) async fn service_refresh_subagent_catalog(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    refresh_subagent_catalog_local_state(&state)
}

/// Opens the user-owned subagent definition directory in the host file manager.
/// 在宿主文件管理器中打开用户拥有的智能体定义目录。
pub(crate) async fn service_open_subagent_user_directory(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let user_definition_root = user_subagent_definition_root();
    std::fs::create_dir_all(&user_definition_root).map_err(|error| {
        format!(
            "创建用户智能体定义目录 `{}` 失败：{error}",
            user_definition_root.display()
        )
    })?;
    let snapshot = refresh_subagent_catalog_local_state(&state)?;
    shell_host_from_state(&state)?.open_path(&user_definition_root.to_string_lossy())?;
    Ok(snapshot)
}

/// Persists one agent definition enabled state through the sidecar configuration layer.
/// 通过 sidecar 配置层持久化单个智能体定义启停状态。
pub(crate) async fn service_set_subagent_definition_enabled(
    state: SharedServiceBridgeState,
    agent_id: String,
    enabled: bool,
) -> Result<SubagentDefinitionEnabledUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from("智能体定义标识不能为空。"));
    }
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
            return Err(format!("未找到智能体定义：{agent_id}"));
        }
    }
    vulcan_task_session::write_user_subagent_enabled_override(
        user_subagent_root().as_path(),
        agent_id,
        enabled,
    )
    .map_err(|error| {
        format!(
            "写入智能体启停覆盖 `{}` 失败：{}",
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
        return Err(format!("未找到智能体定义：{agent_id}"));
    };
    definition.enabled = enabled;
    if let Some(runtime) = guard.agent_runtime.as_ref() {
        runtime
            .task_runtime
            .set_subagent_definition_enabled(agent_id, enabled)
            .map_err(|error| format!("更新运行时智能体启停状态失败：{error}"))?;
    }
    let notice = subagent_catalog_notice_from_summary(&guard.workbench.subagent_catalog);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.subagent_catalog.notice = notice.clone();
    Ok(SubagentDefinitionEnabledUpdate {
        agent_id: agent_id.to_string(),
        enabled,
        notice,
    })
}

/// Saves one user-authored agent definition into the user catalog.
/// 将单个用户编写的智能体定义保存到用户目录。
pub(crate) async fn service_save_subagent_definition(
    state: SharedServiceBridgeState,
    request: SubagentDefinitionSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let requested_agent_id = normalize_agent_file_name_input(&request.agent_id);
    if requested_agent_id.is_empty() {
        return Err(String::from("智能体标识不能为空。"));
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
    let provider_configuration = provider_configuration_from_state(&state)?;
    let requested_provider_model = validate_subagent_provider_model_request_with_existing(
        &request,
        existing.as_ref(),
        &provider_configuration,
    )?;
    if let Some(existing_definition) = existing
        .as_ref()
        .filter(|definition| subagent_definition_is_system_backed(definition))
    {
        if requested_agent_id != original_agent_id {
            return Err(String::from("系统智能体不允许修改 agent_id。"));
        }
        if target_package_id.is_some() {
            return Err(String::from("系统智能体不允许转移到用户包。"));
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
                "保存系统智能体 `{}` 的用户元数据覆盖失败：{}",
                existing_definition.id, error.message
            )
        })?;
        return refresh_subagent_catalog_local_state(&state);
    }
    let target_agent_id = target_agent_id_for_save(
        &requested_agent_id,
        existing.as_ref(),
        target_package_id.as_deref(),
    )?;
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
            "目标智能体 `{target_agent_id}` 已存在，请更换文件名或先编辑已有智能体。"
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
    let request_locales = normalized_request_definition_locales(request.locales);
    definition.name = preferred_definition_locale_name(&request_locales)
        .or_else(|| non_empty_request_text(&request.name))
        .unwrap_or_else(|| target_agent_id.clone());
    definition.description =
        preferred_definition_locale_description(&request_locales).or_else(|| {
            request
                .description
                .as_deref()
                .and_then(non_empty_request_text)
        });
    definition.locales = request_locales;
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
    definition.source.package_id = target_package_id.clone();
    definition.source.updated_at = Some(current_unix_millis_text());
    definition.source.override_of = None;
    let written_user_file = vulcan_task_session::write_user_subagent_definition(
        user_subagent_root().as_path(),
        &definition,
    )
    .map_err(|error| {
        format!(
            "保存智能体定义 `{}` 失败：{}",
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
    refresh_subagent_catalog_local_state(&state)
}

/// Deletes one user-owned agent definition from the user catalog.
/// 从用户目录删除单个用户拥有的智能体定义。
pub(crate) async fn service_delete_subagent_definition(
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from("智能体定义标识不能为空。"));
    }
    let catalog = load_current_subagent_catalog();
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("未找到智能体定义：{agent_id}"))?;
    if !matches!(
        definition.source.scope,
        vulcan_task_session::SubagentDefinitionScope::User
            | vulcan_task_session::SubagentDefinitionScope::Package
    ) || definition.source.override_of.is_some()
    {
        return Err(String::from("只有用户自定义智能体允许删除。"));
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
        return Err(format!("未找到可删除的用户智能体文件：{agent_id}"));
    }
    refresh_subagent_catalog_local_state(&state)
}

/// Restores one system-backed agent definition by deleting its user override.
/// 通过删除用户覆盖来恢复单个系统集成智能体定义。
pub(crate) async fn service_restore_subagent_definition_default(
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from("智能体定义标识不能为空。"));
    }
    let catalog = load_current_subagent_catalog();
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("未找到智能体定义：{agent_id}"))?;
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
            "当前智能体没有可恢复的系统或包默认定义：{agent_id}"
        ));
    }
    for path in restore_candidates {
        remove_user_subagent_file_if_inside_root(&path)?;
    }
    refresh_subagent_catalog_local_state(&state)
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

/// Reprojects agent catalog after a focused catalog write or refresh.
/// 在聚焦的目录写入或刷新后重新投影智能体目录。
fn refresh_subagent_catalog_local_state(
    state: &SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    refresh_subagent_catalog_local_state_with_errors(state, Vec::new())
}

/// Reprojects agent catalog and appends transient refresh errors.
/// 重新投影智能体目录并追加临时刷新错误。
fn refresh_subagent_catalog_local_state_with_errors(
    state: &SharedServiceBridgeState,
    transient_errors: Vec<vulcan_task_session::SubagentCatalogLoadError>,
) -> Result<WorkbenchSnapshot, String> {
    let mut catalog = load_current_subagent_catalog();
    catalog.errors.extend(transient_errors);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_ref() {
        runtime
            .task_runtime
            .replace_subagent_catalog(catalog.clone())
            .map_err(|error| format!("刷新运行时智能体目录失败：{error}"))?;
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    let provider_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .unwrap_or_default();
    guard.workbench.subagent_catalog =
        subagent_catalog_summary_from_catalog(&catalog, &provider_configuration);
    Ok(guard.workbench.clone())
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
        "智能体定义 {} 个，已启用 {} 个，加载警告 {}。",
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

/// Validates a fixed provider/model request against existing metadata and effective model choices.
/// 根据既有元数据与有效模型列表校验固定供应商/模型请求。
pub(crate) fn validate_subagent_provider_model_request_with_existing(
    request: &SubagentDefinitionSaveRequest,
    existing: Option<&vulcan_task_session::SubagentDefinition>,
    provider_configuration: &ProviderConfigurationFile,
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
                "固定模型需要同时提供 Provider 和模型；如需不固定模型，请清空两者以跟随会话模型。",
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
    let available = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .any(|choice| choice.id == model_id);
    if available {
        Ok(snapshot)
    } else {
        Err(format!(
            "固定模型 `{provider_id}/{model_id}` 不在当前已启用模型列表中，请选择可用模型或恢复跟随会话模型。"
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

/// Normalizes localized definition metadata from a save request.
/// 规范化保存请求中的智能体本地化元数据。
fn normalized_request_definition_locales(
    locales: std::collections::BTreeMap<String, SubagentDefinitionLocaleSummary>,
) -> std::collections::BTreeMap<
    String,
    vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
> {
    locales
        .into_iter()
        .filter_map(|(locale, metadata)| {
            let locale = locale.trim().to_string();
            if locale.is_empty() {
                return None;
            }
            let name = metadata.name.as_deref().and_then(non_empty_request_text);
            let description = metadata
                .description
                .as_deref()
                .and_then(non_empty_request_text);
            if name.is_none() && description.is_none() {
                return None;
            }
            Some((
                locale,
                vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition {
                    name,
                    description,
                },
            ))
        })
        .collect()
}

/// Selects the default name from localized metadata with English taking priority.
/// 从本地化元数据中选择默认名称，并优先使用英文。
fn preferred_definition_locale_name(
    locales: &std::collections::BTreeMap<
        String,
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
    >,
) -> Option<String> {
    preferred_definition_locale_field(locales, |metadata| metadata.name.as_ref())
}

/// Selects the default description from localized metadata with English taking priority.
/// 从本地化元数据中选择默认说明，并优先使用英文。
fn preferred_definition_locale_description(
    locales: &std::collections::BTreeMap<
        String,
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
    >,
) -> Option<String> {
    preferred_definition_locale_field(locales, |metadata| metadata.description.as_ref())
}

/// Selects a localized text field using English first and Chinese as fallback.
/// 按英文优先、中文兜底的顺序选择本地化文本字段。
fn preferred_definition_locale_field(
    locales: &std::collections::BTreeMap<
        String,
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
    >,
    field: impl Fn(&vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition) -> Option<&String>,
) -> Option<String> {
    ["en-US", "en", "zh-CN", "zh"].iter().find_map(|preferred| {
        let preferred_lower = preferred.to_ascii_lowercase();
        locales
            .iter()
            .find(|(locale, metadata)| {
                let locale_lower = locale.to_ascii_lowercase();
                (locale_lower == preferred_lower
                    || locale_lower.starts_with(&format!("{preferred_lower}-")))
                    && field(metadata).is_some()
            })
            .and_then(|(_, metadata)| field(metadata).cloned())
    })
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
        return Err(format!("无效的智能体包标识：{value}"));
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
        return Err(format!("用户自定义包不能与系统包 `{package_id}` 重名。"));
    }
    Ok(Some(package_id))
}

/// Returns whether the application ships a system package/category with the id.
/// 返回应用是否内置了同名系统包或分类。
fn system_subagent_package_exists(package_id: &str) -> bool {
    let system_root = vulcan_runtime_assets::resolve_system_subagents_resource_directory();
    std::fs::read_dir(system_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .any(|entry| {
            entry.path().is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name == package_id)
        })
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
            "解析用户智能体根目录 `{}` 失败：{error}",
            user_root.display()
        )
    })?;
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|error| format!("解析智能体用户层文件 `{}` 失败：{error}", path.display()))?;
    Ok(canonical_path.starts_with(canonical_user_root))
}

/// Removes one file when it is an existing user-layer agent file.
/// 当文件是已存在的用户层智能体文件时删除它。
fn remove_user_subagent_file_if_inside_root(path: &Path) -> Result<(), String> {
    if !user_subagent_file_exists_inside_root(path)? {
        return Ok(());
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("删除用户智能体文件 `{}` 失败：{error}", path.display()))
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
            "解析用户智能体根目录 `{}` 失败：{error}",
            user_root.display()
        )
    })?;
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|error| format!("解析旧智能体文件 `{}` 失败：{error}", path.display()))?;
    if std::fs::canonicalize(retained_file)
        .ok()
        .as_ref()
        .is_some_and(|canonical_retained| canonical_retained == &canonical_path)
    {
        return Ok(());
    }
    if !canonical_path.starts_with(&canonical_user_root) {
        return Err(format!(
            "拒绝删除用户智能体目录外的旧文件：{}",
            path.display()
        ));
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("删除旧智能体文件 `{}` 失败：{error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
