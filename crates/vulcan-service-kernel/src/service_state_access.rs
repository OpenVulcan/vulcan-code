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
    current_workspace_snapshot_from_state(state)
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
    mutate_retained_project_local_state(
        state,
        project_id,
        "切换置顶状态",
        |project| {
            project.pinned = !project.pinned;
        },
        |project| {
            project.pinned = !project.pinned;
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;

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
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
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

    /// Verifies pinned-state updates stay synchronized across retained projections.
    /// 验证置顶状态更新会在两份保留投影中保持同步。
    #[test]
    fn update_project_pinned_local_state_updates_both_projections() {
        let state = service_state_with_retained_project();

        update_project_pinned_local_state(&state, "project-a")
            .expect("pinned-state update should mutate retained projections");

        let guard = state.lock().expect("test service state lock must be valid");
        let workspace_snapshot = guard
            .workspace_snapshot
            .as_ref()
            .expect("test workspace snapshot must exist");
        assert!(workspace_snapshot.task_projects[0].pinned);
        assert!(guard.workbench.projects[0].pinned);
        assert_eq!(8, guard.workbench_generation);
        assert_eq!(8, guard.workbench.revision);
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

        let error = update_project_pinned_local_state(&state, "missing-project")
            .expect_err("missing project should return a retained-project diagnostic");

        let guard = state.lock().expect("test service state lock must be valid");
        assert_eq!("没有找到要切换置顶状态的项目：missing-project", error);
        assert_eq!(7, guard.workbench_generation);
        assert_eq!(7, guard.workbench.revision);
    }
}
