//! Shared service-kernel test support.
//! service-kernel 共享测试支撑。

use super::*;

/// Owns one temporary segmented-session project directory created under the service data root.
/// 持有一个创建在服务数据根目录下的临时分段会话项目目录。
pub(super) struct TestSegmentedProjectRoot {
    /// Project id used by workspace session summaries and segmented session lookup.
    /// 工作区会话摘要与分段会话定位使用的项目标识。
    project_id: String,
    /// Absolute project root path inside the session storage tree.
    /// 会话存储树中的绝对项目根路径。
    path: std::path::PathBuf,
}

impl TestSegmentedProjectRoot {
    /// Creates one test-owned project directory handle without eagerly creating the directory.
    /// 创建一个测试拥有的项目目录句柄，但不立即创建目录。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the unique project id that production lookup code will receive.
    /// `project_id` 是生产定位代码将接收到的唯一项目标识。
    ///
    /// # Returns
    /// 返回值
    /// A project-root guard that removes the directory when the test scope exits.
    /// 返回一个会在测试作用域退出时移除目录的项目根守卫。
    pub(super) fn new(project_id: impl Into<String>) -> Self {
        let project_id = project_id.into();
        let data_root = vulcan_workspace_store::data_plane::application_data_root();
        let path = data_root.join("sessions").join(&project_id);
        Self { project_id, path }
    }

    /// Returns the project id used by retained workspace session summaries.
    /// 返回保留工作区会话摘要使用的项目标识。
    ///
    /// # Returns
    /// 返回值
    /// The test project id as a borrowed string.
    /// 以借用字符串形式返回测试项目标识。
    pub(super) fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Returns the filesystem path for this test project root.
    /// 返回该测试项目根目录的文件系统路径。
    ///
    /// # Returns
    /// 返回值
    /// The absolute project root path.
    /// 返回绝对项目根路径。
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TestSegmentedProjectRoot {
    /// Removes the test project directory on scope exit.
    /// 在作用域退出时移除测试项目目录。
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Seeds the authoritative task-runtime registry from test panel states via persisted TaskSession metadata.
/// 通过持久化 TaskSession 元数据，从测试面板状态播种权威任务运行时注册表。
///
/// # Parameters
/// 参数
/// `guard` is the service state whose agent runtime should receive the task-session store root.
/// `guard` 是需要接收子任务会话存储根的服务状态。
/// `root_session_id` is the root parent session for the seeded child-agent tree.
/// `root_session_id` 是待播种子智能体树的根父会话。
/// `states` are the display states used only as test fixtures for persisted metadata.
/// `states` 是仅作为持久化元数据测试夹具使用的展示状态。
///
/// # Returns
/// 返回值
/// A temporary project-root guard that keeps the persisted registry metadata alive for the test.
/// 返回一个临时项目根守卫，用于在测试期间保留持久化注册表元数据。
pub(super) fn seed_test_subagent_registry_from_panel_states(
    guard: &mut ServiceBridgeState,
    root_session_id: SessionId,
    states: &[SubagentTaskPanelState],
) -> TestSegmentedProjectRoot {
    use vulcan_task_session::TaskSessionStore as _;

    if guard.agent_runtime.is_none() {
        guard.agent_runtime = Some(test_agent_runtime_bridge());
    }
    let project_root =
        TestSegmentedProjectRoot::new(format!("registry-fixture-{root_session_id}"));
    let session_dir = project_root.path().join(format!(
        "{NAVIGATION_TEST_ROOT_CREATED_DATE_FOR_SUPPORT}-{root_session_id}"
    ));
    std::fs::create_dir_all(&session_dir).expect("create registry fixture session dir");
    let task_store = vulcan_task_session::SegmentedTaskSessionStore::new(&session_dir);
    let root_session_node = SessionNodeId::from(root_session_id);
    for state in states {
        let parent = if state.parent_session_id == root_session_id {
            vulcan_session_core::ParentSessionRef::MainSession {
                root_session_id: root_session_node,
            }
        } else {
            vulcan_session_core::ParentSessionRef::TaskSession {
                root_session_id: root_session_node,
                parent_task_session_id: SessionNodeId::from(state.parent_session_id),
            }
        };
        let mut metadata = vulcan_task_session::TaskSessionMetadata::new(
            vulcan_task_session::TaskSessionMetadataInit {
                root_session_id: root_session_node,
                task_session_id: SessionNodeId::from(state.task_id),
                parent,
                public_task_id: state.public_task_id.clone(),
                display_title: state.subagent_label.clone(),
                requested_config: vulcan_session_core::SessionRequestedConfig::default(),
                effective_config: vulcan_session_core::SessionEffectiveConfig::default(),
                depth: 1,
                timestamp_ms: 100,
            },
        );
        metadata.status = test_task_session_status_from_panel_state(state.run_state);
        if !state.result_text.trim().is_empty() {
            metadata.last_result_summary = Some(state.result_text.clone());
        }
        task_store
            .create_task_session(metadata)
            .expect("create registry fixture task session");
    }
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .configure_task_session_store_root(root_session_id, session_dir)
        .expect("configure registry fixture store root");
    guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime
        .restore_persisted_subagent_registry_for_parent_tree(root_session_id)
        .expect("restore registry fixture during test hydration");
    project_root
}

/// Seeds the authoritative in-memory TaskRuntime registry from test panel states.
/// 从测试面板状态播种权威的内存 TaskRuntime 注册表。
///
/// # Parameters
/// 参数
/// `guard` is the service state whose agent runtime receives resident task records.
/// `guard` 是接收常驻任务记录的服务状态。
/// `root_session_id` is the root parent session for the child-agent tree.
/// `root_session_id` 是子代理树的根父会话。
/// `states` are fixture states converted into resident TaskRuntime entries.
/// `states` 是转换为常驻 TaskRuntime 条目的夹具状态。
#[cfg(debug_assertions)]
pub(super) fn seed_test_live_subagent_registry_from_panel_states(
    guard: &mut ServiceBridgeState,
    root_session_id: SessionId,
    states: &[SubagentTaskPanelState],
) {
    if guard.agent_runtime.is_none() {
        guard.agent_runtime = Some(test_agent_runtime_bridge());
    }
    let task_runtime = &guard
        .agent_runtime
        .as_ref()
        .expect("agent runtime")
        .task_runtime;
    for state in states {
        task_runtime
            .seed_debug_subagent_registry_entry(
                root_session_id,
                state.parent_session_id,
                state.task_id,
                state.public_task_id.clone(),
                state.subagent_label.clone(),
                state.description.clone(),
                Some(state.subagent_label.clone()),
                test_task_session_status_from_panel_state(state.run_state),
                (!state.result_text.trim().is_empty()).then_some(state.result_text.clone()),
            )
            .expect("seed debug subagent registry entry");
    }
}

/// Caches render-only subagent surface details without creating membership.
/// 只缓存用于渲染的子智能体界面详情，不创建成员关系。
///
/// # Parameters
/// 参数
/// `guard` is the service state receiving render-only detail entries.
/// `guard` 是接收渲染详情条目的服务状态。
/// `states` are panel states whose transcripts may be reused by navigation rendering.
/// `states` 是可被导航渲染复用转录内容的面板状态。
pub(super) fn cache_test_subagent_surface_detail_states(
    guard: &mut ServiceBridgeState,
    states: &[SubagentTaskPanelState],
) {
    for state in states {
        let messages = state
            .conversation_state
            .as_ref()
            .map(conversation_state_to_session_messages)
            .unwrap_or_default();
        guard.subagent_surface_detail_cache.insert(
            state.task_id,
            WebuiSubagentSurfaceDetailCacheEntry {
                task_state: state.clone(),
                messages,
                last_access_ms: 0,
            },
        );
    }
}

/// Converts a WebUI test panel state into persisted TaskSession status.
/// 将 WebUI 测试面板状态转换为持久化 TaskSession 状态。
///
/// # Parameters
/// 参数
/// `run_state` is the WebUI panel state from the fixture.
/// `run_state` 是测试夹具中的 WebUI 面板状态。
///
/// # Returns
/// 返回值
/// The equivalent persisted task-session status.
/// 返回等价的持久化子任务会话状态。
fn test_task_session_status_from_panel_state(
    run_state: SubagentTaskRunState,
) -> vulcan_task_session::TaskSessionStatus {
    match run_state {
        SubagentTaskRunState::Running => vulcan_task_session::TaskSessionStatus::Running,
        SubagentTaskRunState::Stopped => vulcan_task_session::TaskSessionStatus::Stopped,
        SubagentTaskRunState::Completed => vulcan_task_session::TaskSessionStatus::Completed,
        SubagentTaskRunState::Failed => vulcan_task_session::TaskSessionStatus::Failed,
    }
}

/// Fixed directory date used by shared registry fixture roots.
/// 共享注册表测试夹具根目录使用的固定日期。
const NAVIGATION_TEST_ROOT_CREATED_DATE_FOR_SUPPORT: &str = "1970-01-01";

/// Test shell host that rejects interactive shell operations.
/// 拒绝交互式 shell 操作的测试宿主壳。
struct TestServiceShellHost;

impl ServiceShellHost for TestServiceShellHost {
    /// Rejects host file-manager use in unit tests.
    /// 在单元测试中拒绝使用宿主文件管理器。
    fn open_path(&self, _path: &str) -> Result<(), String> {
        Err(String::from(
            "test shell host does not support opening paths",
        ))
    }

    /// Rejects host browser use in unit tests.
    /// 在单元测试中拒绝使用宿主浏览器。
    fn open_url(&self, _url: &str) -> Result<(), String> {
        Err(String::from(
            "test shell host does not support opening URLs",
        ))
    }
}

/// Builds a shared test shell host.
/// 构建共享测试宿主壳。
pub(super) fn test_service_shell_host() -> Arc<dyn ServiceShellHost> {
    Arc::new(TestServiceShellHost)
}

/// Builds a minimal service state containing one regular parent session.
/// 构建只包含一个常规父会话的最小服务状态。
pub(super) fn test_service_state_with_parent_session(
    parent_session_id: SessionId,
) -> ServiceBridgeState {
    ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: Some(WorkspaceSnapshot {
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
            session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::new(),
        }),
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
    }
}

/// Builds a minimal agent-runtime bridge for lifecycle unit tests.
/// 为生命周期单元测试构建最小代理运行时桥。
pub(super) fn test_agent_runtime_bridge() -> AgentRuntimeBridge {
    AgentRuntimeBridge {
        provider_configuration: ProviderConfigurationFile::default(),
        ai_tool_configuration: PersistedAiToolConfiguration::default(),
        agent_profile_catalog: AgentProfileCatalog::default(),
        luaskills_runtime: None,
        mcp_runtime: None,
        task_runtime: Arc::new(TaskRuntime::new()),
        question_runtime: Arc::new(QuestionRuntime::new()),
        command_session_runtime: Arc::new(CommandSessionRuntime::new()),
        token_budget_runtime: Arc::new(TokenBudgetRuntime::pending()),
        conversation_hook_runtime: Arc::new(Mutex::new(ConversationHookRuntime::new())),
        vmm_hook_runtime: VmmHookRuntimeHandle::new(
            vulcan_agent_runtime::agent_runtime::vmm_hooks::VmmHookConfigSnapshot::default(),
        ),
    }
}

/// Builds one delegate-finished runtime event carrying task-session metadata.
/// 构建一条携带 task-session 元数据的委派完成运行时事件。
pub(super) fn test_delegate_finished_event(
    task_session_id: SessionId,
    public_task_id: &str,
    result: &str,
) -> RuntimeEvent {
    RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "子代理完成",
        "子代理任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": public_task_id,
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": result,
        }
    })))
}

/// Builds one parent spawn-agent tool output message with task-session metadata.
/// 构建一条带 task-session 元数据的父级 spawn_agent 工具输出消息。
pub(super) fn test_spawn_agent_tool_message(
    task_id: SessionId,
    execution_status: &str,
    record_kind: SessionMessageRecordKind,
) -> SessionMessageEntry {
    let mut message = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "子代理 image-worker",
        "spawn_agent returned",
    )
    .with_runtime_metadata(format!("tool-{task_id}"), String::from("1780000000000"));
    message.record_kind = record_kind;
    message.metadata = Some(json!({
        "tool_name": "spawn_agent",
        "display_state": execution_status,
        "tool_display_state": execution_status,
        "host_result": {
            "kind": SUBAGENT_TASK_KIND,
            "payload": {
                "task_id": "image-worker",
                (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
                "execution_status": execution_status
            }
        }
    }));
    message
}
