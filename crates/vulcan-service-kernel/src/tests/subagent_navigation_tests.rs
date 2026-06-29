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

/// Verifies historical cached subagents do not slow or hijack a normal parent-session surface.
/// 验证历史缓存子代理不会拖慢或劫持普通父会话界面。
#[test]
fn historical_subagent_cache_without_visible_round_does_not_build_navigation() {
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
    assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
    assert!(
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot).is_none()
    );
}

/// Verifies a terminal child session phase wins over stale detached-listener state.
/// 验证子会话终态阶段优先于陈旧的分离监听器状态。
#[test]
fn terminal_child_phase_removes_stale_running_subagent_from_navigation() {
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
    assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
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
    assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
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
