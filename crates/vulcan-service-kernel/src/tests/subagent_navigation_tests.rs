//! subagent navigation tests.
//! subagent navigation tests銆?
use super::*;

/// Fixed creation timestamp used by segmented navigation preload fixtures.
/// 鍒嗘瀵艰埅棰勮浇娴嬭瘯澶瑰叿浣跨敤鐨勫浐瀹氬垱寤烘椂闂存埑銆?
const NAVIGATION_TEST_ROOT_CREATED_AT: &str = "100";
/// Directory date derived from the fixed navigation test creation timestamp.
/// 浠庡浐瀹氬鑸祴璇曞垱寤烘椂闂存埑鎺ㄥ鍑虹殑鐩綍鏃ユ湡銆?
const NAVIGATION_TEST_ROOT_CREATED_DATE: &str = "1970-01-01";

/// Builds the production-shaped segmented session directory for navigation preload fixtures.
/// 涓哄鑸杞芥祴璇曞す鍏锋瀯寤虹鍚堢敓浜у懡鍚嶈鍒欑殑鍒嗘浼氳瘽鐩綍銆?
fn navigation_test_session_dir(
    project_root: &std::path::Path,
    root_session_id: SessionId,
) -> std::path::PathBuf {
    project_root.join(format!(
        "{NAVIGATION_TEST_ROOT_CREATED_DATE}-{root_session_id}"
    ))
}

/// Builds a production-shaped parent session summary for navigation preload fixtures.
/// 涓哄鑸杞芥祴璇曞す鍏锋瀯寤虹鍚堢敓浜у畾浣嶈涔夌殑鐖朵細璇濇憳瑕併€?
fn navigation_test_parent_session_summary(
    parent_session_id: SessionId,
    project_id: &str,
) -> SessionSummary {
    SessionSummary::new(
        parent_session_id,
        Some(project_id.to_string()),
            "parent session",
        "",
        SessionState::Completed,
        0,
        0,
        "鍒氬垰",
    )
    .with_created_at(NAVIGATION_TEST_ROOT_CREATED_AT)
    .with_updated_at(NAVIGATION_TEST_ROOT_CREATED_AT)
}

/// Creates minimal segmented root metadata for subagent navigation preload tests.
/// 涓哄瓙鏅鸿兘浣撳鑸杞芥祴璇曞垱寤烘渶灏忓垎娈垫牴鍏冩暟鎹€?
fn create_navigation_test_root_metadata(session_dir: &std::path::Path, root_session_id: SessionId) {
    let metadata = json!({
        "schema_version": 1,
        "session_id": root_session_id.to_string(),
        "project_tag": "project-alpha",
        "created_at": "100",
        "updated_at": "100",
        "project_id": "project-alpha",
        "title": "Root",
        "title_source": "manual",
        "canonical_source": "segmented_test",
        "archived_at": null,
        "active_segment": "s001.jsonl",
        "task_dir": "task",
        "message_count": 0,
        "last_message_id": null,
        "last_message_fingerprint": null,
        "latest_rollback_seq": null,
        "next_global_seq": 0,
        "segment_policy": {
            "max_lines": 3000,
            "max_bytes": 8388608
        },
        "segments": [{
            "file": "s001.jsonl",
            "start_seq": 0,
            "line_count": 0,
            "byte_len": 0,
            "first_timestamp": null,
            "last_timestamp": null,
            "first_turn_index": null,
            "last_turn_index": null,
            "last_compaction_seq": null
        }],
        "tasks": []
    });
    let encoded = serde_json::to_string_pretty(&metadata).expect("encode root metadata");
    std::fs::write(session_dir.join("metadata.json"), encoded).expect("write root metadata");
}

/// Builds one persisted navigation test task metadata document.
/// 鏋勫缓鍗曚釜鎸佷箙鍖栧鑸祴璇曚换鍔″厓鏁版嵁鏂囨。銆?
fn navigation_test_task_metadata(
    root_session_id: SessionNodeId,
    task_session_id: SessionNodeId,
    parent: vulcan_session_core::ParentSessionRef,
    public_task_id: &str,
    display_title: &str,
) -> vulcan_task_session::TaskSessionMetadata {
    vulcan_task_session::TaskSessionMetadata::new(vulcan_task_session::TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent,
        public_task_id: public_task_id.to_string(),
        display_title: display_title.to_string(),
        requested_config: vulcan_session_core::SessionRequestedConfig::default(),
        effective_config: vulcan_session_core::SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 100,
    })
}

/// Verifies token-refresh projection can reuse existing subagent navigation without rebuilding the task list.
/// 楠岃瘉 token 鍒锋柊鎶曞奖鍙互澶嶇敤鐜版湁瀛愪唬鐞嗗鑸€屼笉閲嶅缓浠诲姟鍒楄〃銆?
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
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
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
        SessionMessageEntry::new(SessionMessageKind::Assistant, "output", "subagent delta")
            .with_runtime_metadata("child-output", "1000");
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "parent session",
            "",
            SessionState::Running,
            0,
            0,
            "鍒氬垰",
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
}

/// Verifies unclosed cached subagents remain listed without hijacking the parent-session surface.
/// 楠岃瘉鏈叧闂紦瀛樺瓙浠ｇ悊鍦ㄤ笉鍔寔鐖朵細璇濈晫闈㈢殑鎯呭喌涓嬩粛浼氫繚鐣欏湪鍒楄〃涓€?
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
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
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
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("historical-task"),
        parent_session_id,
        subagent_label: String::from("historical-task"),
        description: None,
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    }];
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
/// 楠岃瘉宓屽娲诲姩鏅鸿兘浣撻潰鏉垮湪鏍逛細璇濆鑸笅浠嶇劧鏈夋晥銆?
#[test]
fn nested_active_subagent_panel_remains_active_under_root_navigation() {
    let root_session_id = SessionId::new_v4();
    let nested_parent_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(root_session_id);
    let task_states = vec![
        cached_navigation_test_state(root_session_id, nested_parent_task_id, "lifecycle_test"),
        cached_navigation_test_state(nested_parent_task_id, nested_task_id, "mirror_tool_probe"),
    ];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    let _registry_fixture =
        seed_test_subagent_registry_from_panel_states(&mut guard, root_session_id, &task_states);
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

/// Verifies registry lookup can recover the true direct parent for nested subagent tasks.
/// 楠岃瘉娉ㄥ唽琛ㄦ煡鎵惧彲浠ユ仮澶嶅祵濂楁櫤鑳戒綋浠诲姟鐨勭湡瀹炵洿鎺ョ埗绾с€?
#[test]
fn registry_nested_subagent_parent_lookup_uses_direct_parent() {
    use vulcan_task_session::TaskSessionStore as _;

    let root_session_id = SessionId::new_v4();
    let nested_parent_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(root_session_id);
    guard.agent_runtime = Some(test_agent_runtime_bridge());
    let project_id = format!("subagent-nested-parent-{root_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id);
    let session_dir = navigation_test_session_dir(project_root.path(), root_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    create_navigation_test_root_metadata(&session_dir, root_session_id);
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(root_session_id);
    let nested_parent_node = SessionNodeId::from(nested_parent_task_id);
    task_store
        .create_task_session(navigation_test_task_metadata(
            root_session_node,
            nested_parent_node,
            vulcan_session_core::ParentSessionRef::MainSession {
                root_session_id: root_session_node,
            },
            "nested-parent",
            "nested-parent",
        ))
        .expect("create parent task");
    task_store
        .create_task_session(navigation_test_task_metadata(
            root_session_node,
            SessionNodeId::from(nested_task_id),
            vulcan_session_core::ParentSessionRef::TaskSession {
                root_session_id: root_session_node,
                parent_task_session_id: nested_parent_node,
            },
            "nested-child",
            "nested-child",
        ))
        .expect("create nested task");
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .configure_task_session_store_root(root_session_id, session_dir)
        .expect("configure task store root");
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .restore_persisted_subagent_registry_for_parent_tree(root_session_id)
        .expect("session hydration should restore nested task registry");

    let resolved_entry = open_subagent_registry_entry_for_task(
        &guard,
        root_session_id,
        nested_task_id,
    )
    .expect("registry lookup should succeed")
    .expect("nested child should be open");

    assert_eq!(resolved_entry.parent_session_id.as_uuid(), nested_parent_task_id);
}

/// Verifies workspace bootstrap wording no longer promises subagent index preload.
/// 楠岃瘉宸ヤ綔鍖哄惎鍔ㄦ枃妗堜笉鍐嶆壙璇洪杞芥櫤鑳戒綋绱㈠紩銆?
#[test]
fn workspace_bootstrap_ready_message_excludes_subagent_index_preload() {
    let message = crate::service_bootstrap::startup_ready_message("workspace");

    assert!(!message.trim().is_empty());
    assert!(!message.to_ascii_lowercase().contains("subagent"));
}

/// Verifies detail preload does not create navigation membership without registry metadata.
/// 楠岃瘉缂哄皯娉ㄥ唽琛ㄥ厓鏁版嵁鏃讹紝璇︽儏棰勮浇涓嶄細鍒涘缓瀵艰埅鎴愬憳鍏崇郴銆?
#[test]
fn detail_preload_does_not_create_navigation_membership_without_registry_metadata() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("subagent-restore-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let task_session_node = SessionNodeId::from(task_session_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
    let mut metadata = navigation_test_task_metadata(
        root_session_node,
        task_session_node,
        vulcan_session_core::ParentSessionRef::MainSession {
            root_session_id: root_session_node,
        },
        "restored-agent",
        "鎭㈠鐨勬櫤鑳戒綋",
    );
    metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
    metadata.last_result_summary = Some(String::from("completed stale payload"));
    task_store
        .create_task_session(metadata)
        .expect("create persisted subagent task session");
    std::fs::remove_file(
        session_dir
            .join("task")
            .join(task_session_node.to_string())
            .join("metadata.json"),
    )
    .expect("remove child metadata to prove mirror preload path");

    let mut guard = test_service_state_with_parent_session(parent_session_id);
    guard.agent_runtime = Some(test_agent_runtime_bridge());
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .configure_task_session_store_root(parent_session_id, session_dir.clone())
        .expect("configure task store root");
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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
    let shared_state = Arc::new(Mutex::new(guard));
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should not restore missing child metadata");
    {
        let mut guard = shared_state.lock().expect("shared state should lock");
        assert!(subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none());
    }

    let preload = load_webui_unclosed_subagent_surface_preload(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("subagent surface preload should load");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(&mut guard, &mut workspace_snapshot, preload);

    assert_eq!(restored_count, 0);
    assert!(
        subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot).is_none(),
        "render-detail preload must not invent navigation membership"
    );
    assert!(!guard.subagent_surface_detail_cache.contains_key(&task_session_id));
}

/// Verifies preload treats a missing root mirror as an empty unclosed-subagent index.
/// 楠岃瘉棰勮浇浼氬皢缂哄け鏍归暅鍍忚涓虹┖鏈叧闂瓙鏅鸿兘浣撶储寮曘€?
#[test]
fn hydration_preload_ignores_child_metadata_when_root_task_mirror_is_missing() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("subagent-missing-mirror-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let task_session_node = SessionNodeId::from(task_session_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
    let mut metadata = navigation_test_task_metadata(
        root_session_node,
        task_session_node,
        vulcan_session_core::ParentSessionRef::MainSession {
            root_session_id: root_session_node,
        },
        "fallback-agent",
        "missing mirror agent",
    );
    metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
    task_store
        .create_task_session(metadata)
        .expect("create persisted subagent task session");

    let root_metadata_path = session_dir.join("metadata.json");
    let mut root_metadata = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(&root_metadata_path).expect("read root metadata"),
    )
    .expect("decode root metadata");
    root_metadata
        .as_object_mut()
        .expect("root metadata object")
        .remove("tasks");
    std::fs::write(
        &root_metadata_path,
        serde_json::to_string_pretty(&root_metadata).expect("encode root metadata"),
    )
    .expect("write root metadata without task mirror");

    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    guard.agent_runtime = Some(test_agent_runtime_bridge());
    guard.workspace_snapshot = Some(workspace_snapshot.clone());
    let shared_state = Arc::new(Mutex::new(guard));
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should restore metadata without root mirror membership");
    let preload = load_webui_unclosed_subagent_surface_preload(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("subagent surface preload should load");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(&mut guard, &mut workspace_snapshot, preload);

    assert_eq!(restored_count, 1);
    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("metadata-backed registry should build navigation");
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
}

/// Verifies preload read failures do not mark the root session as metadata-preloaded.
/// 楠岃瘉棰勮浇璇诲彇澶辫触涓嶄細灏嗘牴浼氳瘽鏍囪涓哄凡瀹屾垚鍏冩暟鎹杞姐€?
#[test]
fn hydration_preload_error_does_not_mark_root_as_preloaded() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("subagent-corrupt-mirror-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    task_store
        .create_task_session(navigation_test_task_metadata(
            SessionNodeId::from(parent_session_id),
            SessionNodeId::from(task_session_id),
            vulcan_session_core::ParentSessionRef::MainSession {
                root_session_id: SessionNodeId::from(parent_session_id),
            },
            "corrupt-agent",
            "parent session",
        ))
        .expect("create task metadata before corruption");
    std::fs::write(
        session_dir
            .join("task")
            .join(task_session_id.to_string())
            .join("metadata.json"),
        "{ definitely invalid json",
    )
    .expect("write corrupt task metadata");

    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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
    let mut state = test_service_state_with_parent_session(parent_session_id);
    state.agent_runtime = Some(test_agent_runtime_bridge());
    state.workspace_snapshot = Some(workspace_snapshot.clone());
    let shared_state = std::sync::Arc::new(std::sync::Mutex::new(state));

    let error = restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect_err("corrupt task metadata should fail hydration restore");

    assert!(
        !error.is_empty(),
        "preload should expose the underlying metadata read failure"
    );
    let guard = shared_state.lock().expect("shared state should lock");
    assert!(
        !guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&parent_session_id),
        "failed preload must not create the completion marker"
    );
    assert!(
        guard.subagent_surface_detail_cache.is_empty(),
        "failed preload must not mutate render-only subagent details"
    );
    drop(guard);
}

/// Verifies shared-state preload restores every unclosed subagent before navigation projection.
/// 楠岃瘉鍏变韩鐘舵€侀杞戒細鍦ㄥ鑸姇褰卞墠鎭㈠鎵€鏈夋湭鍏抽棴鏅鸿兘浣撱€?
#[test]
fn shared_state_preload_restores_all_unclosed_subagents_before_navigation_projection() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let review_task_id = SessionId::new_v4();
    let flash_task_id = SessionId::new_v4();
    let nested_task_id = SessionId::new_v4();
    let project_id = format!("subagent-shared-preload-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let review_session_node = SessionNodeId::from(review_task_id);
    let flash_session_node = SessionNodeId::from(flash_task_id);
    let nested_session_node = SessionNodeId::from(nested_task_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
    let direct_parent = vulcan_session_core::ParentSessionRef::MainSession {
        root_session_id: root_session_node,
    };
    let nested_parent = vulcan_session_core::ParentSessionRef::TaskSession {
        root_session_id: root_session_node,
        parent_task_session_id: flash_session_node,
    };
    for (task_node, parent, public_task_id, display_title) in [
        (
            review_session_node,
            direct_parent.clone(),
            "code-review",
            "浠ｇ爜瀹℃煡",
        ),
        (
            flash_session_node,
            direct_parent,
            "code-review-flash",
            "fast code review",
        ),
        (
            nested_session_node,
            nested_parent,
            "nested-review",
            "宓屽瀹℃煡",
        ),
    ] {
        let mut metadata = navigation_test_task_metadata(
            root_session_node,
            task_node,
            parent,
            public_task_id,
            display_title,
        );
        metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
        task_store
            .create_task_session(metadata)
            .expect("create persisted subagent task session");
    }

    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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
    let mut state = test_service_state_with_parent_session(parent_session_id);
    state.agent_runtime = Some(test_agent_runtime_bridge());
    state.workspace_snapshot = Some(workspace_snapshot.clone());
    let shared_state = std::sync::Arc::new(std::sync::Mutex::new(state));

    let mut workspace_snapshot = workspace_snapshot;
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should restore registry before one-shot preload");
    let preload = load_webui_unclosed_subagent_surface_preload_once(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("preload should load once");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count = apply_webui_unclosed_subagent_surface_preload_once(
        &mut guard,
        &mut workspace_snapshot,
        parent_session_id,
        preload,
    );

    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("preloaded subagents should be visible");

    assert_eq!(restored_count, 3);
    assert_eq!(navigation.tasks.len(), 3);
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == review_task_id.to_string())
    );
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == flash_task_id.to_string())
    );
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == nested_task_id.to_string())
    );
    drop(guard);

    let repeated_preload = load_webui_unclosed_subagent_surface_preload_once(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("repeated preload should use cached state");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let repeated_restored_count = apply_webui_unclosed_subagent_surface_preload_once(
        &mut guard,
        &mut workspace_snapshot,
        parent_session_id,
        repeated_preload,
    );
    let repeated_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("preloaded subagents should remain visible after repeated preload");

    assert_eq!(repeated_restored_count, 0);
    assert_eq!(repeated_navigation.tasks.len(), 3);
    assert!(
        guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&parent_session_id)
    );
    drop(guard);
}

/// Verifies surface preload fills missing WebUI navigation entries before cached projection.
/// 楠岃瘉鐣岄潰棰勮浇浼氬湪缂撳瓨鎶曞奖鍓嶈ˉ榻愮己澶辩殑 WebUI 瀵艰埅鏉＄洰銆?
#[test]
fn surface_preload_restores_all_unclosed_tasks_before_cached_navigation() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let clone_task_id = SessionId::new_v4();
    let lifecycle_task_id = SessionId::new_v4();
    let mirror_task_id = SessionId::new_v4();
    let project_id = format!("subagent-registry-sync-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
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
        let mut metadata = navigation_test_task_metadata(
            root_session_node,
            task_node,
            parent,
            public_task_id,
            display_title,
        );
        metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
        metadata.last_result_summary = Some(String::from(result));
        task_store
            .create_task_session(metadata)
            .expect("create persisted subagent task session");
    }
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    guard.agent_runtime = Some(test_agent_runtime_bridge());
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .configure_task_session_store_root(parent_session_id, session_dir.clone())
        .expect("configure task store root");
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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

    let shared_state = Arc::new(Mutex::new(guard));
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should restore registry before preload");
    let preload = load_webui_unclosed_subagent_surface_preload(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("subagent surface preload should load");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(&mut guard, &mut workspace_snapshot, preload);
    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("surface-hydrated subagents should build navigation");

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
}

/// Verifies session hydration preloads unclosed subagent surfaces with transcript details.
/// 楠岃瘉浼氳瘽姘村悎浼氶杞藉甫杞綍璇︽儏鐨勬湭鍏抽棴瀛愭櫤鑳戒綋鐣岄潰銆?
#[test]
fn hydration_preloads_unclosed_subagent_surface_from_segmented_store() {
    use vulcan_session_core::SessionNodeStore as _;
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("subagent-detail-restore-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let task_session_node = SessionNodeId::from(task_session_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
    let mut metadata = navigation_test_task_metadata(
        root_session_node,
        task_session_node,
        vulcan_session_core::ParentSessionRef::MainSession {
            root_session_id: root_session_node,
        },
        "restored-detail-agent",
        "transcript agent",
    );
    metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
    task_store
        .create_task_session(metadata)
        .expect("create persisted subagent task session");
    let child_message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "output", "persisted child detail")
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
    guard.agent_runtime = Some(test_agent_runtime_bridge());
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .configure_task_session_store_root(parent_session_id, session_dir.clone())
        .expect("configure task store root");
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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
    let shared_state = Arc::new(Mutex::new(guard));
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should restore registry before detail preload");
    let preload = load_webui_unclosed_subagent_surface_preload(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("subagent surface preload should load");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(&mut guard, &mut workspace_snapshot, preload);

    assert_eq!(restored_count, 1);
    assert!(
        workspace_snapshot
            .session_messages
            .contains_key(&task_session_id)
    );
    assert!(
        workspace_snapshot
            .hydrated_session_ids
            .contains(&task_session_id)
    );
    assert!(
        guard
            .subagent_surface_detail_cache
            .contains_key(&task_session_id)
    );
    assert!(guard
        .subagent_surface_detail_cache
        .get(&task_session_id)
        .is_some_and(|entry| entry.task_state.conversation_state.is_some()));
    let navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("preloaded subagent should be visible without disk reload");
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
}

/// Verifies opening one child detail preserves sibling unclosed subagent navigation rows.
/// 楠岃瘉鎵撳紑鍗曚釜瀛愭櫤鑳戒綋璇︽儏鏃朵細淇濈暀鍏勫紵鏈叧闂櫤鑳戒綋瀵艰埅琛屻€?
#[test]
fn opening_one_subagent_detail_preserves_full_unclosed_navigation() {
    use vulcan_session_core::SessionNodeStore as _;
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let clone_task_id = SessionId::new_v4();
    let lifecycle_task_id = SessionId::new_v4();
    let mirror_task_id = SessionId::new_v4();
    let project_id = format!("subagent-detail-full-navigation-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
    for (task_id, public_task_id, display_title) in [
        (clone_task_id, "clone_tool_probe", "clone_tool_probe"),
        (lifecycle_task_id, "lifecycle_test", "lifecycle_test"),
        (mirror_task_id, "mirror_tool_probe", "mirror_tool_probe"),
    ] {
        let mut metadata = navigation_test_task_metadata(
            root_session_node,
            SessionNodeId::from(task_id),
            vulcan_session_core::ParentSessionRef::MainSession {
                root_session_id: root_session_node,
            },
            public_task_id,
            display_title,
        );
        metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
        metadata.last_result_summary = Some(format!("{public_task_id} completed"));
        task_store
            .create_task_session(metadata)
            .expect("create persisted subagent task session");
    }
    let lifecycle_message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "杈撳嚭", "lifecycle detail")
            .with_runtime_metadata("lifecycle-output", "1000");
    let lifecycle_conversation_state =
        conversation_state_from_session_messages(&[lifecycle_message]);
    task_store
        .save_context(vulcan_session_core::SessionContextDocument {
            schema_version: 1,
            node_id: SessionNodeId::from(lifecycle_task_id),
            provider_id: None,
            model_id: None,
            reasoning_effort: None,
            system_prompt_layers: Vec::new(),
            conversation_state: json!(lifecycle_conversation_state),
            compaction_anchor: None,
            context_budget_fingerprint: None,
            tool_catalog_snapshot: Value::Null,
            provider_request_preview: None,
        })
        .expect("save lifecycle subagent context");

    let mut guard = test_service_state_with_parent_session(parent_session_id);
    guard.agent_runtime = Some(test_agent_runtime_bridge());
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .configure_task_session_store_root(parent_session_id, session_dir.clone())
        .expect("configure task store root");
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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
    let shared_state = Arc::new(Mutex::new(guard));
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should restore registry before sibling preload");
    let preload = load_webui_unclosed_subagent_surface_preload(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("subagent surface preload should load");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(&mut guard, &mut workspace_snapshot, preload);
    let initial_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("initial navigation should include all open subagents");

    assert_eq!(restored_count, 3);
    assert_eq!(initial_navigation.tasks.len(), 3);

    assert!(
        open_subagent_registry_entry_for_task(&guard, parent_session_id, lifecycle_task_id)
            .expect("registry lookup should succeed")
            .is_some()
    );
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: lifecycle_task_id,
    });
    let navigation_after_detail =
        subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)
            .expect("navigation should remain visible after detail open");

    assert_eq!(navigation_after_detail.tasks.len(), 3);
    assert_eq!(
        navigation_after_detail.active_task_id,
        Some(lifecycle_task_id.to_string())
    );
    assert!(
        navigation_after_detail
            .tasks
            .iter()
            .any(|task| task.task_id == clone_task_id.to_string())
    );
    assert!(
        navigation_after_detail
            .tasks
            .iter()
            .any(|task| task.task_id == mirror_task_id.to_string())
    );
}

/// Verifies archived subagent surfaces are excluded from the unclosed navigation preload.
/// 楠岃瘉宸插綊妗ｆ櫤鑳戒綋鐣岄潰涓嶄細杩涘叆鏈叧闂鑸杞姐€?
#[test]
fn hydration_surface_preload_excludes_archived_subagents() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let open_task_id = SessionId::new_v4();
    let archived_task_id = SessionId::new_v4();
    let project_id = format!("subagent-archived-filter-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = navigation_test_session_dir(project_root.path(), parent_session_id);
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    create_navigation_test_root_metadata(&session_dir, parent_session_id);
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
        let mut metadata = navigation_test_task_metadata(
            root_session_node,
            SessionNodeId::from(task_id),
            vulcan_session_core::ParentSessionRef::MainSession {
                root_session_id: root_session_node,
            },
            public_task_id,
            public_task_id,
        );
        metadata.status = status;
        task_store
            .create_task_session(metadata)
            .expect("create persisted subagent task session");
    }
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![navigation_test_parent_session_summary(
            parent_session_id,
            project_root.project_id(),
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

    guard.agent_runtime = Some(test_agent_runtime_bridge());
    guard.workspace_snapshot = Some(workspace_snapshot.clone());
    let shared_state = Arc::new(Mutex::new(guard));
    restore_webui_subagent_registry_for_session_hydration(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("session hydration should restore non-archived registry entries");
    let preload = load_webui_unclosed_subagent_surface_preload(
        &shared_state,
        &workspace_snapshot,
        parent_session_id,
    )
    .expect("subagent surface preload should load");
    let mut guard = shared_state.lock().expect("shared state should lock");
    let restored_count =
        apply_webui_unclosed_subagent_surface_preload(&mut guard, &mut workspace_snapshot, preload);
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
}

/// Verifies a terminal child session phase marks stale listener state as completed in navigation.
/// 楠岃瘉瀛愪細璇濈粓鎬侀樁娈典細鍦ㄥ鑸腑鎶婇檲鏃х洃鍚櫒鐘舵€佹爣璁颁负宸插畬鎴愩€?
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
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
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
/// 楠岃瘉宸ヤ綔鍖洪樁娈靛彧鍘嬪埗鐢辩洃鍚櫒娈嬬暀鎺ㄥ鍑虹殑杩愯鎬併€?
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
/// 楠岃瘉鍚岀埗绾х殑闈炴椿鍔ㄥ瓙浠ｇ悊闈㈡澘浼氬洖閫€鍒扮埗浼氳瘽鐣岄潰銆?
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
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
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
    let task_states = vec![cached_navigation_test_state(
        parent_session_id,
        task_session_id,
        "inactive-task",
    )];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);
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
        task_session_id
    );
    assert_eq!(
        guard.active_subagent_panel,
        Some(SelectedSubagentTaskPanel {
            parent_session_id,
            task_id: task_session_id,
        })
    );
    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("inactive but unclosed subagent should remain listed");
    assert_eq!(navigation.active_task_id, Some(task_session_id.to_string()));
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
}

/// Verifies a stale WebUI subagent panel cannot hijack another selected conversation surface.
/// 楠岃瘉 WebUI 娈嬬暀鐨勫瓙浠ｇ悊闈㈡澘涓嶈兘鍔寔鍙︿竴涓凡閫変細璇濈殑鏄剧ず鐣岄潰銆?
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
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
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
    let task_states = vec![SubagentTaskPanelState {
        task_id: previous_task_id,
        public_task_id: String::from("previous-task"),
        parent_session_id: previous_session_id,
        subagent_label: String::from("previous-task"),
        description: None,
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    let _registry_fixture =
        seed_test_subagent_registry_from_panel_states(&mut guard, previous_session_id, &task_states);
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                selected_session_id,
                Some(String::from("project-a")),
                "褰撳墠浼氳瘽",
                "",
                SessionState::Completed,
                0,
                0,
                "鍒氬垰",
            ),
            SessionSummary::new(
                previous_session_id,
                Some(String::from("project-a")),
                "previous session",
                "",
                SessionState::Completed,
                0,
                0,
                "鍒氬垰",
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

/// Verifies task-session metadata merge updates the visible parent task card.
/// 楠岃瘉 task-session 鍏冩暟鎹悎骞朵細鏇存柊鍙鐖剁骇浠诲姟鍗＄墖銆?
#[test]
fn task_session_tool_metadata_merge_updates_parent_card() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let mut workspace_snapshot = test_service_state_with_parent_session(parent_session_id)
        .workspace_snapshot
        .expect("test workspace snapshot");
    let message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    let message_id = message.id.clone();
    workspace_snapshot
        .session_messages
        .insert(parent_session_id, vec![message]);

    merge_webui_task_session_tool_message_metadata(
        &mut workspace_snapshot,
        parent_session_id,
        &message_id,
        json!({
            "kind": SUBAGENT_TASK_KIND,
            "payload": {
                "task_id": "image-worker",
                (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                "execution_status": "completed"
            }
        }),
        "completed",
    )
    .expect("metadata merge should update parent card");

    let metadata = workspace_snapshot
        .session_messages
        .get(&parent_session_id)
        .and_then(|messages| messages.first())
        .and_then(|message| message.metadata.as_ref())
        .expect("parent card metadata");
    assert_eq!(
        metadata
            .get("display_state")
            .and_then(|value| value.as_str()),
        Some("completed")
    );
    assert_eq!(
        metadata
            .pointer("/payload/execution_status")
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

/// Verifies task-session metadata merge reports a missing target instead of silently failing.
/// 楠岃瘉 task-session 鍏冩暟鎹悎骞跺湪鐩爣缂哄け鏃惰繑鍥炴樉寮忛敊璇€屼笉鏄潤榛樺け璐ャ€?
#[test]
fn task_session_tool_metadata_merge_reports_missing_parent_card() {
    let parent_session_id = SessionId::new_v4();
    let task_id = SessionId::new_v4();
    let mut workspace_snapshot = test_service_state_with_parent_session(parent_session_id)
        .workspace_snapshot
        .expect("test workspace snapshot");

    let error = merge_webui_task_session_tool_message_metadata(
        &mut workspace_snapshot,
        parent_session_id,
        "missing-parent-task-card",
        json!({
            "kind": SUBAGENT_TASK_KIND,
            "payload": {
                "task_id": "image-worker",
                (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                "execution_status": "completed"
            }
        }),
        "completed",
    )
    .expect_err("missing parent card should be explicit");

    assert!(error.contains("missing-parent-task-card"));
}

/// Builds a compact open subagent state for navigation projection tests.
/// 涓哄鑸姇褰辨祴璇曟瀯寤虹揣鍑戠殑鏈叧闂櫤鑳戒綋鐘舵€併€?
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
            SessionMessageEntry::new(SessionMessageKind::Assistant, "杈撳嚭", public_task_id),
        ])),
    }
}
