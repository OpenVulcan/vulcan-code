//! subagent navigation tests.
//! subagent navigation tests。

use super::*;

/// Verifies token-refresh projection can reuse existing subagent navigation without rebuilding the task list.
/// 验证 token 刷新投影可以复用现有子代理导航而不重建任务列表。
#[test]
fn preserved_subagent_navigation_refresh_reads_child_surface_without_task_list_rebuild() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        session_subagent_tasks: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let child_message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "子代理增量")
            .with_runtime_metadata("child-output", "1000");
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "父会话",
            "",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(task_session_id, vec![child_message.clone()])]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::from([task_session_id]),
    };
    let navigation = SubagentNavigationSnapshot {
        parent_conversation_id: parent_session_id.to_string(),
        active_conversation_id: task_session_id.to_string(),
        active_task_id: Some(task_session_id.to_string()),
        tasks: Vec::new(),
    };

    let active_session_id =
        preserved_active_surface_session_id(&workspace_snapshot, Some(&navigation));
    let surface_messages = surface_messages_for_session_without_subagent_navigation_refresh(
        &guard,
        &workspace_snapshot,
        active_session_id,
    );

    assert_eq!(active_session_id, task_session_id);
    assert_eq!(surface_messages, vec![child_message]);
    assert!(guard.session_subagent_tasks.is_empty());
}

/// Verifies unclosed cached subagents remain listed without hijacking the parent-session surface.
/// 验证未关闭缓存子代理在不劫持父会话界面的情况下仍会保留在列表中。
#[test]
fn unclosed_subagent_cache_without_visible_round_builds_navigation() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::from([(
            parent_session_id,
            BTreeMap::from([(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("historical-task"),
                    parent_session_id,
                    subagent_label: String::from("历史子代理"),
                    description: None,
                    run_state: SubagentTaskRunState::Running,
                    result_text: String::new(),
                    conversation_state: None,
                },
            )]),
        )]),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        active_surface_session_id_from_state(&mut guard, &workspace_snapshot),
        parent_session_id
    );
    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("unclosed task cache should build navigation");
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    let cached_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("cached unclosed task cache should build navigation");
    assert_eq!(cached_navigation.tasks.len(), 1);
}

/// Verifies root navigation lists every unclosed direct and nested subagent from cache.
/// 验证根会话导航会从缓存列出所有未关闭的直接与嵌套智能体。
#[test]
fn cached_navigation_lists_unclosed_subagent_tree() {
    let root_session_id = SessionId::new_v4();
    let direct_task_id = SessionId::new_v4();
    let nested_parent_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(root_session_id);
    guard.session_subagent_tasks = BTreeMap::from([
        (
            root_session_id,
            BTreeMap::from([
                (
                    direct_task_id,
                    cached_navigation_test_state(
                        root_session_id,
                        direct_task_id,
                        "clone_tool_probe",
                    ),
                ),
                (
                    nested_parent_task_id,
                    cached_navigation_test_state(
                        root_session_id,
                        nested_parent_task_id,
                        "lifecycle_test",
                    ),
                ),
            ]),
        ),
        (
            nested_parent_task_id,
            BTreeMap::from([(
                nested_task_id,
                cached_navigation_test_state(
                    nested_parent_task_id,
                    nested_task_id,
                    "mirror_tool_probe",
                ),
            )]),
        ),
    ]);
    let workspace_snapshot = guard
        .workspace_snapshot
        .clone()
        .expect("workspace snapshot");

    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("tree navigation");

    assert_eq!(
        navigation.parent_conversation_id,
        root_session_id.to_string()
    );
    assert_eq!(navigation.tasks.len(), 3);
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == direct_task_id.to_string())
    );
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == nested_parent_task_id.to_string())
    );
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == nested_task_id.to_string())
    );
}

/// Verifies a nested active subagent panel remains valid under root navigation.
/// 验证嵌套活动智能体面板在根会话导航下仍然有效。
#[test]
fn nested_active_subagent_panel_remains_active_under_root_navigation() {
    let root_session_id = SessionId::new_v4();
    let nested_parent_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(root_session_id);
    guard.session_subagent_tasks = BTreeMap::from([
        (
            root_session_id,
            BTreeMap::from([(
                nested_parent_task_id,
                cached_navigation_test_state(
                    root_session_id,
                    nested_parent_task_id,
                    "lifecycle_test",
                ),
            )]),
        ),
        (
            nested_parent_task_id,
            BTreeMap::from([(
                nested_task_id,
                cached_navigation_test_state(
                    nested_parent_task_id,
                    nested_task_id,
                    "mirror_tool_probe",
                ),
            )]),
        ),
    ]);
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id: nested_parent_task_id,
        task_id: nested_task_id,
    });
    let workspace_snapshot = guard
        .workspace_snapshot
        .clone()
        .expect("workspace snapshot");

    let active_session_id = active_surface_session_id_from_state(&mut guard, &workspace_snapshot);
    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("tree navigation");

    assert_eq!(active_session_id, nested_task_id);
    assert_eq!(navigation.active_task_id, Some(nested_task_id.to_string()));
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == nested_task_id.to_string() && task.is_active)
    );
    assert_eq!(
        guard.active_subagent_panel,
        Some(SelectedSubagentTaskPanel {
            parent_session_id: nested_parent_task_id,
            task_id: nested_task_id,
        })
    );
}

/// Verifies cached lookup can recover the true direct parent for nested subagent tasks.
/// 验证缓存查找可以恢复嵌套智能体任务的真实直接父级。
#[test]
fn cached_nested_subagent_parent_lookup_uses_direct_parent() {
    let root_session_id = SessionId::new_v4();
    let nested_parent_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(root_session_id);
    guard.session_subagent_tasks = BTreeMap::from([(
        nested_parent_task_id,
        BTreeMap::from([(
            nested_task_id,
            cached_navigation_test_state(
                nested_parent_task_id,
                nested_task_id,
                "mirror_tool_probe",
            ),
        )]),
    )]);

    let resolved_parent =
        webui_subagent_task_parent_session_id_by_task_id(&guard, root_session_id, nested_task_id);

    assert_eq!(resolved_parent, Some(nested_parent_task_id));
}

/// Verifies a restart-style navigation rebuild restores unclosed subagents from segmented TaskSession metadata.
/// 验证重启式导航重建会从分段 TaskSession 元数据恢复未关闭智能体。
#[test]
fn rebuild_subagent_navigation_restores_unclosed_tasks_from_segmented_store() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("subagent-restore-{parent_session_id}");
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    let project_root = data_root.join("sessions").join(&project_id);
    let session_dir = project_root.join(format!("100-{parent_session_id}"));
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let task_session_node = SessionNodeId::from(task_session_id);
    let mut metadata = vulcan_task_session::TaskSessionMetadata::new(
        root_session_node,
        task_session_node,
        vulcan_session_core::ParentSessionRef::MainSession {
            root_session_id: root_session_node,
        },
        "restored-agent",
        "恢复的智能体",
        vulcan_session_core::SessionRequestedConfig::default(),
        vulcan_session_core::SessionEffectiveConfig::default(),
        1,
        100,
    );
    metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
    metadata.last_result_summary = Some(String::from("已完成但未关闭"));
    task_store
        .create_task_session(metadata)
        .expect("create persisted subagent task session");

    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(project_id.clone()),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    guard.workspace_snapshot = Some(workspace_snapshot.clone());
    assert!(
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .is_none_or(BTreeMap::is_empty)
    );

    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("persisted unclosed subagent should rebuild navigation");

    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    assert_eq!(navigation.tasks[0].label, "恢复的智能体");
    assert_eq!(navigation.tasks[0].run_state, "completed");
    assert!(
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .is_some_and(|tasks| tasks.contains_key(&task_session_id))
    );
    let _ = std::fs::remove_dir_all(project_root);
}

/// Verifies registry hydration fills missing WebUI navigation entries before cached projection.
/// 验证注册表水合会在缓存投影前补齐缺失的 WebUI 导航条目。
#[test]
fn runtime_registry_sync_restores_all_unclosed_tasks_before_cached_navigation() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let clone_task_id = SessionId::new_v4();
    let lifecycle_task_id = SessionId::new_v4();
    let mirror_task_id = SessionId::new_v4();
    let project_id = format!("subagent-registry-sync-{parent_session_id}");
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    let project_root = data_root.join("sessions").join(&project_id);
    let session_dir = project_root.join(format!("100-{parent_session_id}"));
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let clone_session_node = SessionNodeId::from(clone_task_id);
    let lifecycle_session_node = SessionNodeId::from(lifecycle_task_id);
    let mirror_session_node = SessionNodeId::from(mirror_task_id);
    let direct_parent = vulcan_session_core::ParentSessionRef::MainSession {
        root_session_id: root_session_node,
    };
    let nested_parent = vulcan_session_core::ParentSessionRef::TaskSession {
        root_session_id: root_session_node,
        parent_task_session_id: lifecycle_session_node,
    };
    for (task_node, parent, public_task_id, display_title, result) in [
        (
            clone_session_node,
            direct_parent.clone(),
            "clone_tool_probe",
            "clone_tool_probe",
            "clone completed",
        ),
        (
            lifecycle_session_node,
            direct_parent,
            "lifecycle_test",
            "lifecycle_test",
            "lifecycle completed",
        ),
        (
            mirror_session_node,
            nested_parent,
            "mirror_tool_probe",
            "mirror_tool_probe",
            "mirror completed",
        ),
    ] {
        let mut metadata = vulcan_task_session::TaskSessionMetadata::new(
            root_session_node,
            task_node,
            parent,
            public_task_id,
            display_title,
            vulcan_session_core::SessionRequestedConfig::default(),
            vulcan_session_core::SessionEffectiveConfig::default(),
            1,
            100,
        );
        metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
        metadata.last_result_summary = Some(String::from(result));
        task_store
            .create_task_session(metadata)
            .expect("create persisted subagent task session");
    }
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    guard.session_subagent_tasks = BTreeMap::from([(
        parent_session_id,
        BTreeMap::from([(
            clone_task_id,
            cached_navigation_test_state(parent_session_id, clone_task_id, "clone_tool_probe"),
        )]),
    )]);
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(project_id.clone()),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    guard.workspace_snapshot = Some(workspace_snapshot.clone());
    let task_runtime = vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime::new();

    let restored_count = refresh_webui_subagent_task_cache_from_task_runtime_registry(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
        &task_runtime,
    );
    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("registry-hydrated subagents should build navigation");

    assert_eq!(restored_count, 3);
    assert_eq!(navigation.tasks.len(), 3);
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.label == "clone_tool_probe")
    );
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.label == "lifecycle_test")
    );
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.label == "mirror_tool_probe")
    );
    let _ = std::fs::remove_dir_all(project_root);
}

/// Verifies session hydration preloads unclosed subagent metadata without transcript details.
/// 验证会话水合只预载未关闭子智能体元数据，不预载转录详情。
#[test]
fn hydration_preloads_unclosed_subagent_metadata_from_segmented_store() {
    use vulcan_session_core::SessionNodeStore as _;
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("subagent-detail-restore-{parent_session_id}");
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    let project_root = data_root.join("sessions").join(&project_id);
    let session_dir = project_root.join(format!("100-{parent_session_id}"));
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let task_session_node = SessionNodeId::from(task_session_id);
    let mut metadata = vulcan_task_session::TaskSessionMetadata::new(
        root_session_node,
        task_session_node,
        vulcan_session_core::ParentSessionRef::MainSession {
            root_session_id: root_session_node,
        },
        "restored-detail-agent",
        "恢复详情智能体",
        vulcan_session_core::SessionRequestedConfig::default(),
        vulcan_session_core::SessionEffectiveConfig::default(),
        1,
        100,
    );
    metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
    task_store
        .create_task_session(metadata)
        .expect("create persisted subagent task session");
    let child_message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "预载的子智能体详情")
            .with_runtime_metadata("child-output", "1000");
    let conversation_state = conversation_state_from_session_messages(&[child_message]);
    task_store
        .save_context(vulcan_session_core::SessionContextDocument {
            schema_version: 1,
            node_id: task_session_node,
            provider_id: None,
            model_id: None,
            reasoning_effort: None,
            system_prompt_layers: Vec::new(),
            conversation_state: json!(conversation_state),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("save subagent context");

    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(project_id.clone()),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let preload =
        load_webui_unclosed_subagent_metadata_preload(&workspace_snapshot, parent_session_id);
    let restored_count = apply_webui_unclosed_subagent_metadata_preload(&mut guard, preload);

    assert_eq!(restored_count, 1);
    assert!(
        workspace_snapshot
            .session_messages
            .get(&task_session_id)
            .is_none()
    );
    assert!(
        !workspace_snapshot
            .hydrated_session_ids
            .contains(&task_session_id)
    );
    assert!(
        !guard
            .subagent_surface_detail_cache
            .contains_key(&task_session_id)
    );
    assert!(
        guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .and_then(|tasks| tasks.get(&task_session_id))
            .is_some_and(|state| state.conversation_state.is_none())
    );
    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("preloaded subagent should be visible without disk reload");
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    let _ = std::fs::remove_dir_all(project_root);
}

/// Verifies archived subagent metadata is excluded from the unclosed navigation preload.
/// 验证已归档智能体元数据不会进入未关闭导航预载。
#[test]
fn hydration_metadata_preload_excludes_archived_subagents() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let open_task_id = SessionId::new_v4();
    let archived_task_id = SessionId::new_v4();
    let project_id = format!("subagent-archived-filter-{parent_session_id}");
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    let project_root = data_root.join("sessions").join(&project_id);
    let session_dir = project_root.join(format!("100-{parent_session_id}"));
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    for (task_id, public_task_id, status) in [
        (
            open_task_id,
            "open-agent",
            vulcan_task_session::TaskSessionStatus::Completed,
        ),
        (
            archived_task_id,
            "archived-agent",
            vulcan_task_session::TaskSessionStatus::Archived,
        ),
    ] {
        let mut metadata = vulcan_task_session::TaskSessionMetadata::new(
            root_session_node,
            SessionNodeId::from(task_id),
            vulcan_session_core::ParentSessionRef::MainSession {
                root_session_id: root_session_node,
            },
            public_task_id,
            public_task_id,
            vulcan_session_core::SessionRequestedConfig::default(),
            vulcan_session_core::SessionEffectiveConfig::default(),
            1,
            100,
        );
        metadata.status = status;
        task_store
            .create_task_session(metadata)
            .expect("create persisted subagent task session");
    }
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(project_id.clone()),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let preload =
        load_webui_unclosed_subagent_metadata_preload(&workspace_snapshot, parent_session_id);
    let restored_count = apply_webui_unclosed_subagent_metadata_preload(&mut guard, preload);
    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("open subagent should remain visible");

    assert_eq!(restored_count, 1);
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, open_task_id.to_string());
    assert!(
        !navigation
            .tasks
            .iter()
            .any(|task| task.task_id == archived_task_id.to_string())
    );
    let _ = std::fs::remove_dir_all(project_root);
}

/// Verifies a terminal child session phase marks stale listener state as completed in navigation.
/// 验证子会话终态阶段会在导航中把陈旧监听器状态标记为已完成。
#[test]
fn terminal_child_phase_marks_stale_running_subagent_completed_in_navigation() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::from([(
            parent_session_id,
            BTreeMap::from([(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("agent-alpha"),
                    parent_session_id,
                    subagent_label: String::from("Alpha"),
                    description: Some(String::from("sequential counter 1-5")),
                    run_state: SubagentTaskRunState::Running,
                    result_text: String::new(),
                    conversation_state: None,
                },
            )]),
        )]),
        session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::from([task_session_id]),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([
            (parent_session_id, SessionRunPhase::Ready),
            (task_session_id, SessionRunPhase::Ready),
        ]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::from([task_session_id]),
    };

    assert!(
        running_webui_subagent_task_states_for_tree_from_cache(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
        )
        .is_empty()
    );
    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("terminal but unclosed subagent should remain listed");
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    assert_eq!(navigation.tasks[0].run_state, "completed");
    assert!(!navigation.tasks[0].is_running);
}

/// Verifies workspace phases only suppress listener-derived running state.
/// 验证工作区阶段只压制由监听器残留推导出的运行态。
#[test]
fn workspace_phase_live_run_helper_filters_only_stale_listener_state() {
    let task_session_id = SessionId::new_v4();

    assert!(webui_subagent_task_has_live_run_for_phase(
        Some(SessionRunPhase::Requesting),
        None,
        true,
        task_session_id,
    ));
    assert!(!webui_subagent_task_has_live_run_for_phase(
        Some(SessionRunPhase::Ready),
        None,
        true,
        task_session_id,
    ));
    assert!(webui_subagent_task_has_live_run_for_phase(
        None,
        None,
        true,
        task_session_id,
    ));
    assert!(!webui_subagent_task_has_live_run_for_phase(
        None,
        None,
        false,
        task_session_id,
    ));
}

/// Verifies a same-parent inactive subagent panel falls back to the parent surface.
/// 验证同父级的非活动子代理面板会回退到父会话界面。
#[test]
fn inactive_same_parent_subagent_panel_falls_back_to_parent_surface() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::from([(
            parent_session_id,
            BTreeMap::from([(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("inactive-task"),
                    parent_session_id,
                    subagent_label: String::from("已结束子代理"),
                    description: None,
                    run_state: SubagentTaskRunState::Completed,
                    result_text: String::new(),
                    conversation_state: None,
                },
            )]),
        )]),
        session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
        active_subagent_panel: Some(SelectedSubagentTaskPanel {
            parent_session_id,
            task_id: task_session_id,
        }),
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        active_surface_session_id_from_state(&mut guard, &workspace_snapshot),
        parent_session_id
    );
    assert!(guard.active_subagent_panel.is_none());
    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("inactive but unclosed subagent should remain listed");
    assert_eq!(navigation.active_task_id, None);
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
}

/// Verifies a stale WebUI subagent panel cannot hijack another selected conversation surface.
/// 验证 WebUI 残留的子代理面板不能劫持另一个已选会话的显示界面。
#[test]
fn webui_active_subagent_panel_cannot_cross_selected_session() {
    let selected_session_id = SessionId::new_v4();
    let previous_session_id = SessionId::new_v4();
    let previous_task_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::from([(
            previous_session_id,
            BTreeMap::from([(
                previous_task_id,
                SubagentTaskPanelState {
                    task_id: previous_task_id,
                    public_task_id: String::from("previous-task"),
                    parent_session_id: previous_session_id,
                    subagent_label: String::from("旧子代理"),
                    description: None,
                    run_state: SubagentTaskRunState::Running,
                    result_text: String::new(),
                    conversation_state: None,
                },
            )]),
        )]),
        session_subagent_navigation_rounds: BTreeSet::from([previous_session_id]),
        active_subagent_panel: Some(SelectedSubagentTaskPanel {
            parent_session_id: previous_session_id,
            task_id: previous_task_id,
        }),
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                selected_session_id,
                Some(String::from("project-a")),
                "当前会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
            SessionSummary::new(
                previous_session_id,
                Some(String::from("project-a")),
                "旧会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: selected_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        active_surface_session_id_from_state(&mut guard, &workspace_snapshot),
        selected_session_id
    );
    assert!(guard.active_subagent_panel.is_none());
    assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
}

/// Builds a compact open subagent state for navigation projection tests.
/// 为导航投影测试构建紧凑的未关闭智能体状态。
fn cached_navigation_test_state(
    parent_session_id: SessionId,
    task_id: SessionId,
    public_task_id: &str,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id,
        public_task_id: public_task_id.to_string(),
        parent_session_id,
        subagent_label: public_task_id.to_string(),
        description: None,
        run_state: SubagentTaskRunState::Completed,
        result_text: String::new(),
        conversation_state: Some(conversation_state_from_session_messages(&[
            SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", public_task_id),
        ])),
    }
}
