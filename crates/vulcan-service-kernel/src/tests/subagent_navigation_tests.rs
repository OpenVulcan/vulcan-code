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

/// Verifies an exact embedded task file remains readable after every live agent entry is gone.
/// 验证全部实时智能体条目消失后，精确嵌入式任务文件仍可读取。
#[tokio::test]
async fn closed_subagent_panel_reopens_exact_embedded_task_file() {
    // parent_session_id identifies the indexed root that owns the deterministic TASK directory.
    // parent_session_id 标识拥有确定性 TASK 目录的已索引根会话。
    let parent_session_id = SessionId::new_v4();
    // task_session_id identifies the closed child transcript recovered only from its exact file.
    // task_session_id 标识仅从精确文件恢复的已关闭子任务转录。
    let task_session_id = SessionId::new_v4();
    // controller_root owns production-shaped root storage and removes it after the test.
    // controller_root 持有生产形态的根存储，并在测试结束后将其移除。
    let controller_root = TestHostRuntimeRoot::new(parent_session_id);
    // service_state intentionally has no AgentRuntime, proving open-task registry membership is not required.
    // service_state 故意不包含 AgentRuntime，以证明无需属于开放任务注册表。
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    // controller_client keeps the isolated attached APP space alive through exact locator lookup.
    // controller_client 在精确定位查询期间保持隔离的已挂接 APP 空间存活。
    let controller_runtime = controller_root.attached_host_runtime_handle().await;
    service_state.controller_runtime = Some(controller_runtime);
    // root_handle publishes the exact parent locator required to create the deterministic child file.
    // root_handle 发布创建确定性子文件所需的精确父定位器。
    let root_handle = crate::session_runtime_registry_v1::SessionHandle::new(
        parent_session_id,
        controller_root.v1_storage_locator(),
        vulcan_session_core::v1::document::SessionDocument::new(
            parent_session_id.to_string(),
            "project-a",
            "Closed child transcript owner",
            1,
        ),
    );
    service_state
        .session_runtime_registry
        .register(root_handle)
        .expect("root V1 session should register");
    // child_handle creates and flushes the embedded task before its process-local handle is removed.
    // child_handle 在移除进程内句柄前创建并刷新嵌入式任务。
    let child_handle = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        service_state.session_runtime_registry.as_ref(),
        vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts {
            root_session_id: parent_session_id,
            parent_session_id,
            task_id: task_session_id,
            execution_id: String::from("closed-subagent-transcript-execution"),
            prompt: String::from("retain this transcript after close"),
            display_title: String::from("Closed child transcript"),
            is_resumed: false,
            admitted_at_ms: 1_787_788_800_020,
        },
    )
    .expect("embedded task file should be created");
    assert_eq!(child_handle.session_id(), task_session_id);
    service_state
        .session_runtime_registry
        .unregister_lifecycle_session(task_session_id)
        .expect("closed child handle should unregister")
        .expect("closed child handle should exist");
    let state = Arc::new(Mutex::new(service_state));

    let response = service_open_subagent_panel(
        Arc::clone(&state),
        parent_session_id.to_string(),
        task_session_id.to_string(),
    )
    .await
    .expect("closed child transcript should reopen from exact storage");

    assert_eq!(
        response.parent_conversation_id,
        Some(parent_session_id.to_string())
    );
    assert_eq!(
        response.active_conversation_id,
        Some(task_session_id.to_string())
    );
    assert!(response.subagent_navigation.is_none());
    assert!(matches!(
        state
            .lock()
            .expect("lock route-only child state")
            .session_runtime_registry
            .get(task_session_id),
        Err(crate::session_runtime_registry_v1::SessionRuntimeError::SessionNotRegistered(_))
    ));

    // hydration is the one explicit persisted-window transfer requested by the visible closed-child tab.
    // hydration 是可见已关闭子会话页签显式请求的一次持久化窗口传递。
    let hydration = crate::conversation_events_v1::service_session_event_hydration_v1(
        Arc::clone(&state),
        vulcan_service_api::ConversationEventHydrationV1Request {
            session_id: task_session_id.to_string(),
            parent_session_id: Some(parent_session_id.to_string()),
        },
    )
    .await
    .expect("closed child recent window should load once from exact storage");
    assert_eq!(hydration.batch.session_id, task_session_id.to_string());
    assert!(!hydration.batch.events.is_empty());
    assert_eq!(hydration.older_before_seq, None);
    assert!(!hydration.has_more);
    assert!(matches!(
        state
            .lock()
            .expect("lock one-shot child state")
            .session_runtime_registry
            .get(task_session_id),
        Err(crate::session_runtime_registry_v1::SessionRuntimeError::SessionNotRegistered(_))
    ));
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
        native_vmm_maintenance: None,
        vce_runtime: None,
        vce_watcher: None,
        workspace_watcher: None,
        service_runtime: None,
        webui_service_discovery: None,
        html_answer_runtime: None,
        controller_runtime: None,
        knowledge_runtime: None,
        luaskills_runtime: None,
        host_database_providers: None,
        cached_luaskills_catalog: None,
        provider_manifest_sync: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeMap::from([(
            parent_session_id,
            BTreeSet::from([task_session_id]),
        )]),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeMap::from([(
            task_session_id,
            String::from("registry-terminal-test-execution"),
        )]),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        active_stream_tasks: BTreeMap::new(),
        question_preparation_tasks: BTreeMap::new(),
        collaboration_dispatch_owners: BTreeMap::new(),
        session_terminal_persistence_owners: BTreeMap::new(),
        child_question_execution_owners: BTreeMap::new(),
        child_maintenance_owners: BTreeMap::new(),
        child_maintenance_tasks: BTreeMap::new(),
        child_maintenance_stops: BTreeMap::new(),
        terminal_pre_intent_repairs: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        automatic_plugin_prewarm: BTreeMap::new(),
        session_runtime_registry: Default::default(),
        service_lifecycle_state: crate::state::ServiceLifecycleState::default(),
        pending_local_plugin_updates: Arc::new(Default::default()),
        // Actual original removal resources survive executor failures.
        // 实际原移除资源跨执行器失败存续。
        pending_local_plugin_uninstalls: Arc::new(Default::default()),
        project_resource_catalog_registry: Default::default(),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
        vmm_controller_registry: Default::default(),
        browser_manager: crate::browser::BrowserManager::new(),
    };
    let task_states = vec![cached_navigation_test_state(
        parent_session_id,
        task_session_id,
        "agent-alpha",
    )];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
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

/// Verifies closed child rows remain visible for the current parent turn and disappear at the next turn boundary.
/// 验证已关闭子代理行在当前父回合内保持可见，并在下一回合边界消失。
#[test]
fn closed_subagent_navigation_is_retained_only_for_the_owning_round() {
    // parent_session_id owns both the archived child and the current navigation round.
    // parent_session_id 同时拥有已归档子代理与当前导航轮次。
    let parent_session_id = SessionId::new_v4();
    // task_session_id identifies the archived child row retained for inspection.
    // task_session_id 标识为查看而保留的已归档子代理行。
    let task_session_id = SessionId::new_v4();
    // closed_state seeds the authoritative registry with an archived lifecycle status.
    // closed_state 使用已归档生命周期状态填充权威注册表。
    let mut closed_state =
        cached_navigation_test_state(parent_session_id, task_session_id, "closed-worker");
    closed_state.run_state = SubagentTaskRunState::Closed;
    // guard owns the registry and the explicit current-turn visibility marker.
    // guard 持有注册表与显式的当前回合可见标记。
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    seed_test_live_subagent_registry_from_panel_states(
        &mut guard,
        parent_session_id,
        std::slice::from_ref(&closed_state),
    );
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id, BTreeSet::from([task_session_id]));

    let current_round_navigation = subagent_navigation_snapshot_for_parent_from_cached_state(
        &mut guard,
        parent_session_id,
        true,
    )
    .expect("current round navigation should project")
    .expect("closed row should remain visible in its owning round");

    assert_eq!(current_round_navigation.tasks.len(), 1);
    assert_eq!(
        current_round_navigation.tasks[0].task_id,
        task_session_id.to_string()
    );
    assert_eq!(current_round_navigation.tasks[0].run_state, "closed");
    assert!(!current_round_navigation.tasks[0].is_running);

    // workspace_snapshot supplies the selected parent used by the read-only click route.
    // workspace_snapshot 提供只读点击路由使用的已选父会话。
    let workspace_snapshot = guard
        .workspace_snapshot
        .clone()
        .expect("closed row route requires a workspace snapshot");
    // route_response proves clicking a closed row retains the same authoritative navigation membership.
    // route_response 证明点击已关闭行会保留同一份权威导航成员关系。
    let route_response = build_readonly_subagent_panel_route_response_for_task_locked(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
        task_session_id,
    )
    .expect("closed row should open without mutating navigation membership");
    let routed_navigation = route_response
        .subagent_navigation
        .expect("closed row click should return the retained navigation");
    assert_eq!(routed_navigation.tasks.len(), 1);
    assert_eq!(routed_navigation.tasks[0].run_state, "closed");

    guard
        .session_subagent_navigation_rounds
        .remove(&parent_session_id);
    let next_round_navigation = subagent_navigation_snapshot_for_parent_from_cached_state(
        &mut guard,
        parent_session_id,
        true,
    )
    .expect("next round navigation should project");

    assert!(next_round_navigation.is_none());

    // new_task_id belongs to a later round and must not re-admit the previously dismissed closed child.
    // new_task_id 属于后续回合，不得重新准入此前已移除的关闭子任务。
    let new_task_id = SessionId::new_v4();
    // new_state models a newly created child alongside the archived registry history.
    // new_state 模拟与已归档注册表历史并存的新建子任务。
    let new_state = cached_navigation_test_state(parent_session_id, new_task_id, "new-worker");
    seed_test_live_subagent_registry_from_panel_states(
        &mut guard,
        parent_session_id,
        std::slice::from_ref(&new_state),
    );
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id, BTreeSet::from([new_task_id]));
    // reopened_navigation is rebuilt by the same path used after the next spawn event.
    // reopened_navigation 通过下一次创建事件后使用的同一路径重建。
    let reopened_navigation = subagent_navigation_snapshot_for_parent_from_cached_state(
        &mut guard,
        parent_session_id,
        true,
    )
    .expect("later round should project")
    .expect("new child should be visible");
    assert_eq!(reopened_navigation.tasks.len(), 1);
    assert_eq!(
        reopened_navigation.tasks[0].task_id,
        new_task_id.to_string()
    );

    // historical_route loads the old child's persisted view without changing the new round membership.
    // historical_route 载入旧子任务的持久化视图，但不改变新回合成员关系。
    let historical_route = build_readonly_subagent_panel_route_response_for_task_locked(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
        task_session_id,
    )
    .expect("historical route should remain readable");
    let historical_navigation = historical_route
        .subagent_navigation
        .expect("new child remains visible");
    assert_eq!(historical_navigation.tasks.len(), 1);
    assert_eq!(
        historical_navigation.tasks[0].task_id,
        new_task_id.to_string()
    );
}

/// Verifies stopping another child does not dismiss closed rows before the next user turn.
/// 验证停止另一个子任务不会在下一用户回合前移除已关闭行。
/// Parameters: none. Returns: none; asserts current-round membership survives stop cleanup.
/// 参数：无。返回值：无；断言停止清理不会破坏当前回合成员关系。
#[test]
fn subagent_navigation_stop_preserves_closed_siblings() {
    // parent_id owns the two child lifecycle records in the same turn.
    // parent_id 拥有同一回合中的两个子任务生命周期记录。
    let parent_id = SessionId::new_v4();
    // closed_id is retained for inspection; stopped_id is independently stopped.
    // closed_id 为查看而保留；stopped_id 被独立停止。
    let closed_id = SessionId::new_v4();
    let stopped_id = SessionId::new_v4();
    // closed and stopped seed the authoritative registry after both lifecycle transitions.
    // closed 和 stopped 填充两次生命周期变化后的权威注册表。
    let mut closed = cached_navigation_test_state(parent_id, closed_id, "closed");
    closed.run_state = SubagentTaskRunState::Closed;
    let mut stopped = cached_navigation_test_state(parent_id, stopped_id, "stopped");
    stopped.run_state = SubagentTaskRunState::Stopped;
    // guard retains the closed sibling admitted earlier in this user turn.
    // guard 保留本用户回合此前准入的已关闭兄弟任务。
    let mut guard = test_service_state_with_parent_session(parent_id);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_id, &[closed, stopped]);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_id, BTreeSet::from([closed_id, stopped_id]));
    mark_webui_subagent_tasks_stopped_in_state(&mut guard, &[(parent_id, stopped_id)]);
    // navigation must retain both rows until a genuine new-turn boundary clears the closed one.
    // navigation 必须保留两行，直到真正的新回合边界清除已关闭行。
    let navigation =
        subagent_navigation_snapshot_for_parent_from_cached_state(&mut guard, parent_id, true)
            .expect("stop projection succeeds")
            .expect("current round is retained");
    assert_eq!(navigation.tasks.len(), 2);
    assert!(
        navigation
            .tasks
            .iter()
            .any(|task| task.task_id == closed_id.to_string() && task.run_state == "closed")
    );
}

/// Verifies late completed feedback cannot re-admit archived history after a new turn starts.
/// 验证新回合开始后迟到的完成反馈不能重新准入归档历史。
/// Parameters: none. Returns: none; asserts the event leaves navigation empty.
/// 参数：无。返回值：无；断言事件处理后导航仍为空。
#[test]
fn subagent_navigation_late_completion_cannot_readmit_dismissed_child() {
    // parent_id and child_id identify an archived child from a previous turn.
    // parent_id 和 child_id 标识此前回合中已归档的子任务。
    let parent_id = SessionId::new_v4();
    let child_id = SessionId::new_v4();
    // closed seeds history without any current-turn navigation admission.
    // closed 填充历史记录，不授予任何当前回合导航准入。
    let mut closed = cached_navigation_test_state(parent_id, child_id, "closed");
    closed.run_state = SubagentTaskRunState::Closed;
    // guard owns the real registry queried by the event projection.
    // guard 持有事件投影查询的真实注册表。
    let mut guard = test_service_state_with_parent_session(parent_id);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_id, &[closed]);
    // runtime supplies the production ingest interface without starting provider work.
    // runtime 提供生产摄取接口，但不启动供应商执行。
    let runtime = guard.agent_runtime.clone().expect("seeded runtime");
    // state and app run the late callback through the real ingest and navigation emission path.
    // state 和 app 让迟到回调通过真实摄取与导航发射路径。
    let state = Arc::new(Mutex::new(guard));
    let app = ServiceEventEmitter::new(Vec::new());
    let event = test_delegate_finished_event(child_id, "closed", "finished before close");
    ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_id, &event)
        .expect("late event handled");
    let mut guard = state.lock().expect("service state lock");
    assert!(!guard.subagent_surface_detail_cache.contains_key(&child_id));
    assert!(
        subagent_navigation_snapshot_for_parent_from_cached_state(&mut guard, parent_id, true)
            .expect("late event navigation projection")
            .is_none()
    );
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
