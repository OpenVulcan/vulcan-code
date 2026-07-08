//! Conversation selection fast-path tests.
//! 会话选择快速路径测试。

use super::*;
use std::sync::{Arc, Mutex};

/// Returns active surface messages and fails the test on registry projection errors.
/// 返回活动界面消息，并在注册表投影错误时让测试失败。
fn active_surface_messages_from_workspace_and_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Vec<SessionMessageEntry> {
    super::active_surface_messages_from_workspace_and_state(guard, workspace_snapshot)
        .expect("subagent active surface messages should project")
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

/// Builds a sidebar conversation summary for one test session.
/// 为一个测试会话构建侧栏会话摘要。
///
/// # Parameters
/// 参数
/// `session_id` identifies the conversation row.
/// `session_id` 标识会话行。
/// `title` is the human-readable row title.
/// `title` 是人类可读的行标题。
///
/// # Returns
/// 返回值
/// A minimal conversation summary matching the retained workspace session.
/// 返回与保留工作区会话匹配的最小会话摘要。
fn test_conversation_summary(session_id: SessionId, title: &str) -> ConversationSummary {
    ConversationSummary {
        id: session_id.to_string(),
        title: title.to_string(),
        status: SessionState::Completed.label().to_string(),
        run_phase: SessionRunPhase::Ready.to_string(),
        last_updated_label: String::from("刚刚"),
        last_updated_at: String::from("100"),
        unread: false,
        attention: None,
        project_id: Some(String::from("project-a")),
        fork_source: None,
    }
}

/// Builds a visible test transcript message.
/// 构建一条可见测试转录消息。
///
/// # Parameters
/// 参数
/// `message_id` is the stable runtime message id.
/// `message_id` 是稳定运行时消息标识。
/// `body` is the text body shown in the WebUI timeline.
/// `body` 是 WebUI 时间线展示的正文。
///
/// # Returns
/// 返回值
/// A session message entry that survives display projection.
/// 返回可通过显示投影的会话消息条目。
fn test_visible_message(message_id: &str, body: &str) -> SessionMessageEntry {
    SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", body)
        .with_runtime_metadata(message_id, "100")
}

/// Builds a compact test compaction entry anchored to one message.
/// 构建一条锚定到单条消息的紧凑测试压缩记录。
///
/// # Parameters
/// 参数
/// `entry_id` is the stable compaction entry id.
/// `entry_id` 是稳定的压缩记录标识。
/// `message_id` is the transcript message id that anchors the compaction boundary.
/// `message_id` 是锚定压缩边界的转录消息标识。
/// `turn_index` is the turn boundary stored with the compaction.
/// `turn_index` 是随压缩记录保存的轮次边界。
/// `summary_message_id` is the generated summary message id.
/// `summary_message_id` 是生成的摘要消息标识。
/// `created_at` is the deterministic creation timestamp used by tests.
/// `created_at` 是测试使用的确定性创建时间戳。
///
/// # Returns
/// 返回值
/// A minimal compaction entry for hydration-retention tests.
/// 返回用于水合保留测试的最小压缩记录。
fn test_compaction_entry(
    entry_id: &str,
    message_id: &str,
    turn_index: i64,
    summary_message_id: &str,
    created_at: &str,
) -> vulcan_context::SessionCompactionEntry {
    vulcan_context::SessionCompactionEntry {
        id: entry_id.to_string(),
        source_until_message_id: Some(message_id.to_string()),
        source_until_turn_index: Some(turn_index),
        context_start_message_id: Some(message_id.to_string()),
        context_budget_fingerprint: None,
        context_assembly_fingerprint: None,
        retained_tail_start_turn_index: None,
        summary_message_id: Some(summary_message_id.to_string()),
        created_at: created_at.to_string(),
    }
}

/// Creates minimal segmented root metadata for retained-selection preload tests.
/// 为保留选择预载测试创建最小分段根元数据。
///
/// # Parameters
/// 参数
/// `session_dir` is the persisted segmented root session directory.
/// `session_dir` 是持久化分段根会话目录。
/// `root_session_id` identifies the root WebUI conversation.
/// `root_session_id` 标识根 WebUI 会话。
/// `project_id` identifies the owning test project.
/// `project_id` 标识所属测试项目。
fn create_selection_test_root_metadata(
    session_dir: &std::path::Path,
    root_session_id: SessionId,
    project_id: &str,
) {
    let metadata = serde_json::json!({
        "schema_version": 1,
        "session_id": root_session_id.to_string(),
        "project_tag": project_id,
        "created_at": "100",
        "updated_at": "100",
        "project_id": project_id,
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

/// Builds one persisted subagent metadata document for retained-selection preload tests.
/// 为保留选择预载测试构建单个持久化子智能体元数据文档。
///
/// # Parameters
/// 参数
/// `root_session_id` identifies the root WebUI conversation.
/// `root_session_id` 标识根 WebUI 会话。
/// `task_session_id` identifies the child task session.
/// `task_session_id` 标识子任务会话。
/// `public_task_id` is the user-visible task label.
/// `public_task_id` 是用户可见任务标签。
/// `display_title` is the child panel display title.
/// `display_title` 是子面板显示标题。
///
/// # Returns
/// 返回值
/// A TaskSession metadata object parented by the root conversation.
/// 返回挂在根会话下的 TaskSession 元数据对象。
fn selection_test_task_metadata(
    root_session_id: SessionNodeId,
    task_session_id: SessionNodeId,
    public_task_id: &str,
    display_title: &str,
) -> vulcan_task_session::TaskSessionMetadata {
    vulcan_task_session::TaskSessionMetadata::new(vulcan_task_session::TaskSessionMetadataInit {
        root_session_id,
        task_session_id,
        parent: vulcan_session_core::ParentSessionRef::MainSession { root_session_id },
        public_task_id: public_task_id.to_string(),
        display_title: display_title.to_string(),
        requested_config: vulcan_session_core::SessionRequestedConfig::default(),
        effective_config: vulcan_session_core::SessionEffectiveConfig::default(),
        depth: 1,
        timestamp_ms: 100,
    })
}

/// Adds a second retained conversation to one minimal service state.
/// 向一个最小服务状态添加第二个保留会话。
///
/// # Parameters
/// 参数
/// `state` is the mutable service state under construction.
/// `state` 是正在构建的可变服务状态。
/// `second_session_id` identifies the additional conversation.
/// `second_session_id` 标识额外会话。
fn add_second_conversation(state: &mut ServiceBridgeState, second_session_id: SessionId) {
    let workspace_snapshot = state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot.sessions.push(SessionSummary::new(
        second_session_id,
        Some(String::from("project-a")),
        "第二会话",
        "",
        SessionState::Completed,
        0,
        100,
        "刚刚",
    ));
    workspace_snapshot
        .session_run_phases
        .insert(second_session_id, SessionRunPhase::Ready);
    state.workbench.conversations = workspace_snapshot
        .sessions
        .iter()
        .map(|session| test_conversation_summary(session.id, &session.title))
        .collect();
    state.workbench.selected_conversation_id =
        Some(workspace_snapshot.selected_session.to_string());
}

/// Verifies sidebar selection does not hydrate an unloaded conversation body.
/// 验证侧栏选择不会水合尚未加载的会话正文。
#[tokio::test]
async fn select_conversation_does_not_hydrate_unloaded_body() {
    let first_session_id = SessionId::new_v4();
    let second_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(first_session_id);
    add_second_conversation(&mut service_state, second_session_id);
    let first_message = test_visible_message("first-message", "first body");
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot
        .session_messages
        .insert(first_session_id, vec![first_message]);
    workspace_snapshot
        .hydrated_session_ids
        .insert(first_session_id);
    service_state.workbench.hydrated_conversation_windows =
        vec![HydratedConversationWindowSummary {
            session_id: first_session_id.to_string(),
            subagent_metadata_loaded: false,
            earliest_ordinal: None,
            has_more_history: false,
        }];
    service_state
        .webui_preloaded_unclosed_subagent_roots
        .clear();
    let state = Arc::new(Mutex::new(service_state));

    let workbench = service_select_conversation(state.clone(), second_session_id.to_string())
        .await
        .expect("select conversation");

    assert_eq!(
        workbench.selected_conversation_id.as_deref(),
        Some(second_session_id.to_string().as_str())
    );
    assert!(
        workbench
            .messages
            .iter()
            .all(|message| message.conversation_id != second_session_id.to_string())
    );
    assert!(
        workbench
            .messages
            .iter()
            .all(|message| message.conversation_id != first_session_id.to_string())
    );
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| window.session_id == first_session_id.to_string())
    );
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .all(|window| window.session_id != second_session_id.to_string())
    );
    let guard = state.lock().expect("state lock");
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot");
    assert_eq!(workspace_snapshot.selected_session, second_session_id);
    assert!(
        !workspace_snapshot
            .session_messages
            .contains_key(&second_session_id)
    );
    assert!(guard.webui_preloaded_unclosed_subagent_roots.is_empty());
}

/// Verifies retained selection does not synthesize missing subagent preload completion.
/// 验证保留会话选择不会合成缺失的子智能体预载完成标记。
#[tokio::test]
async fn select_retained_conversation_does_not_mark_missing_subagent_metadata_loaded() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot.session_messages.insert(
        session_id,
        vec![test_visible_message("message", "retained body")],
    );
    workspace_snapshot.hydrated_session_ids.insert(session_id);
    service_state.workbench.conversations = vec![test_conversation_summary(session_id, "父会话")];
    service_state.workbench.selected_conversation_id = Some(session_id.to_string());
    retain_hydrated_conversation_windows_for_workspace(
        &mut service_state.hydrated_conversation_windows,
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot"),
    );
    service_state
        .webui_preloaded_unclosed_subagent_roots
        .clear();
    let state = Arc::new(Mutex::new(service_state));

    let workbench = service_select_conversation(state.clone(), session_id.to_string())
        .await
        .expect("select retained conversation");

    assert!(workbench.subagent_navigation.is_none());
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| {
                window.session_id == session_id.to_string() && !window.subagent_metadata_loaded
            })
    );
    let guard = state.lock().expect("state lock");
    assert!(
        !guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&session_id)
    );
}

/// Verifies cached subagent navigation is projected after retained selection preload completion.
/// 验证保留选择完成预载后会立即投影缓存子智能体导航。
#[tokio::test]
async fn select_conversation_projects_cached_subagent_navigation_only() {
    let first_session_id = SessionId::new_v4();
    let second_session_id = SessionId::new_v4();
    let child_task_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(first_session_id);
    add_second_conversation(&mut service_state, second_session_id);
    let first_message = test_visible_message("first-message", "first body");
    let second_message = test_visible_message("second-message", "second body");
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot
        .session_messages
        .insert(first_session_id, vec![first_message]);
    workspace_snapshot
        .session_messages
        .insert(second_session_id, vec![second_message]);
    workspace_snapshot
        .hydrated_session_ids
        .extend([first_session_id, second_session_id]);
    let task_states = vec![SubagentTaskPanelState {
        task_id: child_task_id,
        public_task_id: String::from("cached_child"),
        parent_session_id: second_session_id,
        subagent_label: String::from("cached child"),
        description: Some(String::from("cached child task")),
        run_state: SubagentTaskRunState::Completed,
        result_text: String::from("done"),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut service_state, &task_states);
    seed_test_live_subagent_registry_from_panel_states(
        &mut service_state,
        second_session_id,
        &task_states,
    );
    service_state
        .webui_preloaded_unclosed_subagent_roots
        .insert(second_session_id);
    let state = Arc::new(Mutex::new(service_state));

    let workbench = service_select_conversation(state.clone(), second_session_id.to_string())
        .await
        .expect("select conversation");

    let navigation = workbench
        .subagent_navigation
        .expect("cached subagent navigation");
    assert_eq!(
        navigation.parent_conversation_id,
        second_session_id.to_string()
    );
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, child_task_id.to_string());
    assert!(
        workbench
            .messages
            .iter()
            .any(|message| message.conversation_id == second_session_id.to_string())
    );
    assert!(
        workbench
            .messages
            .iter()
            .all(|message| message.conversation_id != first_session_id.to_string())
    );
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| window.session_id == second_session_id.to_string())
    );
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| {
                window.session_id == second_session_id.to_string()
                    && window.subagent_metadata_loaded
            })
    );
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| window.session_id == first_session_id.to_string())
    );
    let guard = state.lock().expect("state lock");
    assert!(
        guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&second_session_id)
    );
}

/// Verifies retained selection does not read persisted unclosed subagents before returning the snapshot.
/// 验证保留选择不会在返回快照前读取持久化未关闭子智能体。
#[tokio::test]
async fn select_retained_conversation_does_not_restore_persisted_subagents_before_returning() {
    use vulcan_task_session::TaskSessionStore as _;

    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let project_id = format!("selection-preload-{parent_session_id}");
    let project_root = TestSegmentedProjectRoot::new(project_id.clone());
    let session_dir = project_root
        .path()
        .join(format!("1970-01-01-{parent_session_id}"));
    std::fs::create_dir_all(&session_dir).expect("create segmented parent session dir");
    create_selection_test_root_metadata(&session_dir, parent_session_id, &project_id);
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(parent_session_id);
    let task_session_node = SessionNodeId::from(task_session_id);
    let mut metadata = selection_test_task_metadata(
        root_session_node,
        task_session_node,
        "selection_child",
        "选择恢复智能体",
    );
    metadata.status = vulcan_task_session::TaskSessionStatus::Completed;
    task_store
        .create_task_session(metadata)
        .expect("create persisted subagent task session");

    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    let session = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == parent_session_id)
        .expect("parent session");
    session.project_id = Some(project_root.project_id().to_string());
    session.created_at = String::from("100");
    workspace_snapshot.session_messages.insert(
        parent_session_id,
        vec![test_visible_message("message", "retained body")],
    );
    workspace_snapshot
        .hydrated_session_ids
        .insert(parent_session_id);
    service_state.workbench.conversations =
        vec![test_conversation_summary(parent_session_id, "父会话")];
    service_state.workbench.selected_conversation_id = Some(parent_session_id.to_string());
    retain_hydrated_conversation_windows_for_workspace(
        &mut service_state.hydrated_conversation_windows,
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot"),
    );
    let state = Arc::new(Mutex::new(service_state));

    let workbench = service_select_conversation(state.clone(), parent_session_id.to_string())
        .await
        .expect("select retained conversation");

    assert!(workbench.subagent_navigation.is_none());
    assert!(
        workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| {
                window.session_id == parent_session_id.to_string()
                    && !window.subagent_metadata_loaded
            })
    );
    let guard = state.lock().expect("state lock");
    assert!(
        !guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&parent_session_id)
    );
}

/// Verifies retained body loads return only the requested conversation message window.
/// 验证已保留正文加载只返回请求会话的消息窗口。
#[tokio::test]
async fn retained_conversation_messages_response_is_focused_on_requested_session() {
    let first_session_id = SessionId::new_v4();
    let second_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(first_session_id);
    add_second_conversation(&mut service_state, second_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot.session_messages.insert(
        first_session_id,
        vec![test_visible_message("first-message", "first body")],
    );
    workspace_snapshot.session_messages.insert(
        second_session_id,
        vec![test_visible_message("second-message", "second body")],
    );
    workspace_snapshot
        .hydrated_session_ids
        .extend([first_session_id, second_session_id]);
    retain_hydrated_conversation_windows_for_workspace(
        &mut service_state.hydrated_conversation_windows,
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot"),
    );
    service_state
        .webui_preloaded_unclosed_subagent_roots
        .insert(first_session_id);
    let state = Arc::new(Mutex::new(service_state));

    let response = service_session_messages(
        ServiceEventEmitter::new(Vec::new()),
        state.clone(),
        first_session_id.to_string(),
    )
    .await
    .expect("load retained messages");

    assert!(!response.messages.is_empty());
    assert!(
        response
            .messages
            .iter()
            .all(|message| message.conversation_id == first_session_id.to_string())
    );
    assert!(
        response
            .workbench
            .messages
            .iter()
            .all(|message| message.conversation_id == first_session_id.to_string())
    );
    assert!(
        response
            .workbench
            .hydrated_conversation_windows
            .iter()
            .any(|window| {
                window.session_id == first_session_id.to_string() && window.subagent_metadata_loaded
            })
    );
    let guard = state.lock().expect("state lock");
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot");
    assert!(
        workspace_snapshot
            .session_messages
            .contains_key(&second_session_id)
    );
}

/// Verifies retained message fast path refuses sessions missing subagent preload completion.
/// 验证已保留消息快速路径会拒绝缺失子智能体预载完成标记的会话。
#[tokio::test]
async fn retained_conversation_messages_fast_path_requires_subagent_preload_marker() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot.session_messages.insert(
        session_id,
        vec![test_visible_message("message", "retained body")],
    );
    workspace_snapshot.hydrated_session_ids.insert(session_id);
    retain_hydrated_conversation_windows_for_workspace(
        &mut service_state.hydrated_conversation_windows,
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot"),
    );
    let state = Arc::new(Mutex::new(service_state));

    let error = service_session_messages(
        ServiceEventEmitter::new(Vec::new()),
        state.clone(),
        session_id.to_string(),
    )
    .await
    .expect_err("missing subagent preload marker should force full hydration");

    assert!(
        error.contains("controller") || error.contains("workspace"),
        "unexpected full-hydration error: {error}"
    );
    let guard = state.lock().expect("state lock");
    assert!(
        !guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&session_id)
    );
}

/// Verifies retained message fast path still requires the session to be a regular conversation.
/// 验证已保留消息快速路径仍要求目标会话是常规会话。
#[tokio::test]
async fn retained_conversation_messages_fast_path_rejects_non_regular_session() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    workspace_snapshot.session_messages.insert(
        task_session_id,
        vec![test_visible_message("message", "task retained body")],
    );
    workspace_snapshot.session_runtime_state.insert(
        task_session_id,
        SessionRuntimeStateEntry {
            active_model_id: Some(String::from("task-model")),
            ..SessionRuntimeStateEntry::default()
        },
    );
    workspace_snapshot
        .hydrated_session_ids
        .insert(task_session_id);
    retain_hydrated_conversation_windows_for_workspace(
        &mut service_state.hydrated_conversation_windows,
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot"),
    );
    let state = Arc::new(Mutex::new(service_state));
    let task_session_id_text = task_session_id.to_string();

    let error = service_session_messages(
        ServiceEventEmitter::new(Vec::new()),
        state.clone(),
        task_session_id_text.clone(),
    )
    .await
    .expect_err("task-session retained body must not use fast path");

    assert!(
        error.contains("controller runtime is not ready"),
        "unexpected slow-path error: {error}"
    );
    let guard = state.lock().expect("state lock");
    assert_ne!(
        guard.workbench.selected_conversation_id.as_deref(),
        Some(task_session_id_text.as_str())
    );
    assert!(
        guard
            .workbench
            .messages
            .iter()
            .all(|message| message.body != "task retained body")
    );
}

/// Verifies startup index projection leaves conversation hydration to the normal session loader.
/// 验证启动索引投影不加载会话正文和智能体详情，而是交给正常会话加载器处理。
#[test]
fn startup_index_projection_excludes_messages_windows_and_subagents() {
    let parent_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state.agent_runtime = Some(test_agent_runtime_bridge());
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();
    let surface_messages =
        active_surface_messages_from_workspace_and_state(&mut service_state, &workspace_snapshot);
    let subagent_navigation =
        subagent_navigation_snapshot_from_cached_state(&mut service_state, &workspace_snapshot);
    let hydrated_windows = hydrated_conversation_window_summaries(
        &service_state.hydrated_conversation_windows,
        &service_state.webui_preloaded_unclosed_subagent_roots,
    );
    assert!(surface_messages.is_empty());
    assert!(subagent_navigation.is_none());
    assert!(hydrated_windows.is_empty());
}

/// Verifies history-page RPC responses carry the requested session window even when selection differs.
/// 验证历史分页 RPC 响应在当前选中会话不同时仍携带请求会话窗口。
#[test]
fn history_page_response_workbench_is_focused_on_requested_session() {
    let current_session_id = SessionId::new_v4();
    let requested_session_id = SessionId::new_v4();
    let current_entries = vec![test_visible_message("current-message", "current body")];
    let requested_entries = vec![test_visible_message("requested-message", "requested body")];
    let current_messages = conversation_messages_from_session(
        &current_session_id.to_string(),
        &current_entries,
        false,
    );
    let requested_messages = conversation_messages_from_session(
        &requested_session_id.to_string(),
        &requested_entries,
        false,
    );
    let mut event_workbench = fallback_workbench_snapshot(&[]);
    event_workbench.selected_conversation_id = Some(current_session_id.to_string());
    event_workbench.messages = current_messages.clone();

    let response_workbench =
        focused_history_page_response_workbench(&event_workbench, requested_messages);

    assert_eq!(
        response_workbench.selected_conversation_id,
        event_workbench.selected_conversation_id
    );
    assert!(
        event_workbench
            .messages
            .iter()
            .all(|message| message.conversation_id == current_session_id.to_string())
    );
    assert!(!response_workbench.messages.is_empty());
    assert!(
        response_workbench
            .messages
            .iter()
            .all(|message| message.conversation_id == requested_session_id.to_string())
    );
    assert!(
        response_workbench
            .messages
            .iter()
            .all(|message| message.body == "requested body")
    );
}

/// Verifies active-surface checks include both selected and subagent-active conversations.
/// 验证活动界面判断同时包含选中会话与子代理活动会话。
#[test]
fn workbench_surface_matches_selected_or_subagent_active_conversation() {
    // selected_session_id identifies the regular selected conversation.
    // selected_session_id 标识常规选中会话。
    let selected_session_id = SessionId::new_v4();
    // subagent_session_id identifies the child conversation currently shown by subagent navigation.
    // subagent_session_id 标识子代理导航当前展示的子会话。
    let subagent_session_id = SessionId::new_v4();
    // unrelated_session_id proves unrelated conversations are not treated as active.
    // unrelated_session_id 证明无关会话不会被视为活动界面。
    let unrelated_session_id = SessionId::new_v4();
    // workbench stores both the regular selection and active subagent navigation surface.
    // workbench 同时保存常规选中会话和活动子代理导航界面。
    let mut workbench = fallback_workbench_snapshot(&[]);
    workbench.selected_conversation_id = Some(selected_session_id.to_string());
    workbench.subagent_navigation = Some(SubagentNavigationSnapshot {
        parent_conversation_id: selected_session_id.to_string(),
        active_conversation_id: subagent_session_id.to_string(),
        active_task_id: Some(subagent_session_id.to_string()),
        tasks: Vec::new(),
    });

    assert!(workbench_surface_matches_conversation(
        &workbench,
        selected_session_id.to_string().as_str()
    ));
    assert!(workbench_surface_matches_conversation(
        &workbench,
        subagent_session_id.to_string().as_str()
    ));
    assert!(!workbench_surface_matches_conversation(
        &workbench,
        unrelated_session_id.to_string().as_str()
    ));
}

/// Verifies lightweight hydration refresh preserves retained payloads for other sessions.
/// 验证轻量水合刷新会保留其它会话的已水合载荷。
#[test]
fn hydration_index_refresh_preserves_other_retained_payloads() {
    let hydrating_session_id = SessionId::new_v4();
    let retained_session_id = SessionId::new_v4();
    let preloaded_session_id = SessionId::new_v4();
    let removed_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(hydrating_session_id);
    add_second_conversation(&mut service_state, retained_session_id);
    add_second_conversation(&mut service_state, preloaded_session_id);
    let retained_workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot");
    retained_workspace.session_messages.insert(
        hydrating_session_id,
        vec![test_visible_message("hydrating-old", "old hydrating")],
    );
    retained_workspace.session_messages.insert(
        retained_session_id,
        vec![test_visible_message("retained-message", "retained body")],
    );
    retained_workspace.session_messages.insert(
        preloaded_session_id,
        vec![test_visible_message(
            "preloaded-retained-message",
            "retained preloaded body",
        )],
    );
    retained_workspace.session_messages.insert(
        removed_session_id,
        vec![test_visible_message("removed-message", "removed body")],
    );
    retained_workspace.session_runtime_state.insert(
        retained_session_id,
        SessionRuntimeStateEntry {
            active_model_id: Some(String::from("retained-model")),
            ..SessionRuntimeStateEntry::default()
        },
    );
    retained_workspace.session_runtime_state.insert(
        preloaded_session_id,
        SessionRuntimeStateEntry {
            active_model_id: Some(String::from("retained-preloaded-model")),
            ..SessionRuntimeStateEntry::default()
        },
    );
    retained_workspace.session_compactions.insert(
        retained_session_id,
        vec![test_compaction_entry(
            "retained-compaction",
            "retained-message",
            1,
            "summary-message",
            "100",
        )],
    );
    retained_workspace.session_compactions.insert(
        preloaded_session_id,
        vec![test_compaction_entry(
            "retained-preloaded-compaction",
            "preloaded-retained-message",
            1,
            "preloaded-summary-message",
            "100",
        )],
    );
    retained_workspace.hydrated_session_ids.extend([
        hydrating_session_id,
        retained_session_id,
        preloaded_session_id,
        removed_session_id,
    ]);
    let retained_workspace = retained_workspace.clone();
    let mut refreshed_workspace = retained_workspace.clone();
    refreshed_workspace.session_messages = BTreeMap::from([
        (
            hydrating_session_id,
            vec![test_visible_message("hydrating-new", "new hydrating")],
        ),
        (
            preloaded_session_id,
            vec![test_visible_message(
                "preloaded-current-message",
                "current preloaded body",
            )],
        ),
    ]);
    refreshed_workspace.session_runtime_state = BTreeMap::from([(
        preloaded_session_id,
        SessionRuntimeStateEntry {
            active_model_id: Some(String::from("current-preloaded-model")),
            ..SessionRuntimeStateEntry::default()
        },
    )]);
    refreshed_workspace.session_compactions = BTreeMap::from([(
        preloaded_session_id,
        vec![test_compaction_entry(
            "current-preloaded-compaction",
            "preloaded-current-message",
            2,
            "current-summary-message",
            "200",
        )],
    )]);
    refreshed_workspace.hydrated_session_ids = BTreeSet::from([hydrating_session_id]);

    preserve_retained_hydrated_payloads_after_index_refresh(
        &mut refreshed_workspace,
        &retained_workspace,
        hydrating_session_id,
    );

    assert_eq!(
        refreshed_workspace.session_messages[&hydrating_session_id][0].body,
        "new hydrating"
    );
    assert_eq!(
        refreshed_workspace.session_messages[&retained_session_id][0].body,
        "retained body"
    );
    assert!(
        !refreshed_workspace
            .session_messages
            .contains_key(&removed_session_id)
    );
    assert_eq!(
        refreshed_workspace.session_runtime_state[&retained_session_id]
            .active_model_id
            .as_deref(),
        Some("retained-model")
    );
    assert_eq!(
        refreshed_workspace.session_messages[&preloaded_session_id][0].body,
        "current preloaded body"
    );
    assert_eq!(
        refreshed_workspace.session_runtime_state[&preloaded_session_id]
            .active_model_id
            .as_deref(),
        Some("current-preloaded-model")
    );
    assert_eq!(
        refreshed_workspace.session_compactions[&retained_session_id][0].id,
        "retained-compaction"
    );
    assert_eq!(
        refreshed_workspace.session_compactions[&preloaded_session_id][0].id,
        "current-preloaded-compaction"
    );
    assert!(
        refreshed_workspace
            .hydrated_session_ids
            .contains(&hydrating_session_id)
    );
    assert!(
        refreshed_workspace
            .hydrated_session_ids
            .contains(&retained_session_id)
    );
    assert!(
        refreshed_workspace
            .hydrated_session_ids
            .contains(&preloaded_session_id)
    );
    assert!(
        !refreshed_workspace
            .hydrated_session_ids
            .contains(&removed_session_id)
    );
}
