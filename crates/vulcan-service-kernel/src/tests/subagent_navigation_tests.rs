//! subagent navigation tests.
//! 子智能体导航测试。
use super::*;

/// Returns rebuilt subagent navigation and fails the test on registry projection errors.
/// 返回重建后的智能体导航，并在注册表投影错误时让测试失败。
fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    super::subagent_navigation_snapshot_from_state(guard, workspace_snapshot)
        .expect("subagent navigation projection should succeed")
}

/// Returns cached subagent navigation and fails the test on registry projection errors.
/// 返回缓存智能体导航，并在注册表投影错误时让测试失败。
fn subagent_navigation_snapshot_from_cached_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    super::subagent_navigation_snapshot_from_cached_state(guard, workspace_snapshot)
        .expect("cached subagent navigation projection should succeed")
}

/// Verifies active subagent surfaces fall back to cached conversation detail when no display rows exist.
/// 验证活动子代理界面在没有显示层行时会回退到缓存会话详情。
/// Verifies active child surfaces scope cloned runtime state without showing parent history.
/// 验证活动子界面会裁剪克隆运行态且不会显示父会话历史。
/// Verifies active child surfaces scope already persisted display rows before rendering.
/// 验证活动子界面在渲染前会裁剪已经持久化的显示行。
/// Verifies unclosed cached subagents remain listed without hijacking the parent-session surface.
/// 楠岃瘉鏈叧闂紦瀛樺瓙浠ｇ悊鍦ㄤ笉鍔寔鐖朵細璇濈晫闈㈢殑鎯呭喌涓嬩粛浼氫繚鐣欏湪鍒楄〃涓€?
/// Verifies root navigation lists every unclosed direct and nested subagent from cache.
/// 楠岃瘉鏍逛細璇濆鑸細浠庣紦瀛樺垪鍑烘墍鏈夋湭鍏抽棴鐨勭洿鎺ヤ笌宓屽鏅鸿兘浣撱€?
#[test]
fn cached_navigation_lists_unclosed_subagent_tree() {
    let root_session_id = SessionId::new_v4();
    let direct_task_id = SessionId::new_v4();
    let nested_parent_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(root_session_id);
    let task_states = vec![
        cached_navigation_test_state(root_session_id, direct_task_id, "clone_tool_probe"),
        cached_navigation_test_state(root_session_id, nested_parent_task_id, "lifecycle_test"),
        cached_navigation_test_state(nested_parent_task_id, nested_task_id, "mirror_tool_probe"),
    ];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    let _registry_fixture =
        seed_test_subagent_registry_from_panel_states(&mut guard, root_session_id, &task_states);
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
/// Verifies workspace bootstrap wording no longer promises subagent index preload.
/// 验证工作区启动文案不再承诺预加载智能体索引。
#[test]
fn workspace_bootstrap_ready_message_excludes_subagent_index_preload() {
    let message = crate::service_bootstrap::startup_ready_message("workspace");

    assert!(!message.trim().is_empty());
    assert!(!message.to_ascii_lowercase().contains("subagent"));
}

/// Verifies registry terminal status remains visible after a stale listener is suppressed.
/// 验证陈旧监听器被抑制后，注册表中的终态状态仍然会显示在导航中。
#[test]
fn registry_terminal_status_remains_visible_after_stale_listener_is_suppressed() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        pending_prompt_process_mirrors: BTreeMap::new(),
        agent_runtime: None,
        codebase_memory_controller: Default::default(),
        managed_memory_mesh_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        knowledge_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        provider_manifest_sync: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeMap::from([(
            task_session_id,
            String::from("registry-terminal-test-execution"),
        )]),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        active_stream_tasks: BTreeMap::new(),
        session_terminal_persistence_owners: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_runtime_registry: Default::default(),
        project_resource_catalog_registry: Default::default(),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
        vmm_controller_registry: Default::default(),
    };
    let task_states = vec![cached_navigation_test_state(
        parent_session_id,
        task_session_id,
        "agent-alpha",
    )];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    let _registry_fixture =
        seed_test_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "parent session",
            "",
            SessionState::Completed,
            0,
            0,
            "鍒氬垰",
        )],
        archived_sessions: Vec::new(),
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

/// Verifies a same-parent inactive subagent panel falls back to the parent surface.
/// 验证同父级的非活动子智能体面板会回退到父会话界面。
/// Verifies a stale WebUI subagent panel cannot hijack another selected conversation surface.
/// 验证 WebUI 残留的子智能体面板不能劫持另一个已选会话的显示界面。
/// Builds a compact completed subagent state for process-local navigation projection tests.
/// 为进程内导航投影测试构建紧凑的已完成子智能体状态。
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
        conversation_state: Some(
            conversation_state_from_session_messages(&[SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "杈撳嚭",
                public_task_id,
            )
            .with_runtime_metadata(format!("subagent-output-{task_id}"), "1")])
            .expect("valid replayed conversation identities"),
        ),
    }
}
