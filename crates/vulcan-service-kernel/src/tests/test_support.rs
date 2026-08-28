//! Shared service-kernel test support.
//! service-kernel 共享测试支撑。

use super::*;

use crate::session_runtime_registry_v1::{SessionHandle, SessionPersistenceBackend};
use vulcan_agent_runtime::agent_runtime::task_runtime::TestSubagentRegistryEntrySeed;
use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

/// In-memory durable backend used by service tests that only exercise V1 projections.
/// 供仅验证 V1 投影的服务测试使用的内存可靠后端。
struct TestSessionV1ProjectionBackend;

impl SessionPersistenceBackend for TestSessionV1ProjectionBackend {
    /// Accepts one complete candidate and returns its authoritative metadata.
    /// 接纳一份完整候选并返回其权威 metadata。
    ///
    /// # Parameters
    /// 参数
    /// `candidate` is the exact V1 flush candidate produced by the session owner.
    /// `candidate` 是会话 owner 产生的精确 V1 flush 候选。
    ///
    /// # Returns
    /// 返回值
    /// Returns a successful publication receipt without touching the filesystem.
    /// 返回成功发布回执且不访问文件系统。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Registers one exact V1 document in a service test state.
/// 在服务测试状态中注册一份精确 V1 文档。
///
/// # Parameters
/// 参数
/// `state` owns the process registry, `session_id` is the exact identity, and `document` is the authoritative snapshot.
/// `state` 持有进程注册表，`session_id` 是精确身份，`document` 是权威快照。
///
/// # Returns
/// 返回值
/// Returns the unique registered session handle.
/// 返回唯一已注册的会话句柄。
pub(super) fn register_test_session_v1_document(
    state: &mut ServiceBridgeState,
    session_id: SessionId,
    document: SessionDocument,
) -> Arc<SessionHandle> {
    let handle = SessionHandle::with_backend(
        session_id,
        document,
        Arc::new(TestSessionV1ProjectionBackend),
    );
    state
        .session_runtime_registry
        .register(handle.clone())
        .expect("test V1 session should register");
    handle
}

/// Seeds the authoritative process-local task-runtime registry from test panel states.
/// 从测试面板状态播种权威的进程内任务运行时注册表。
///
/// # Parameters
/// 参数
/// `guard` is the service state whose agent runtime receives resident test records.
/// `guard` 是其代理运行时接收常驻测试记录的服务状态。
/// `root_session_id` is the root parent session for the seeded child-agent tree.
/// `root_session_id` 是待播种子智能体树的根父会话。
/// `states` are the display states projected into resident TaskRuntime entries.
/// `states` 是投影为常驻 TaskRuntime 条目的展示状态。
pub(super) fn seed_test_subagent_registry_from_panel_states(
    guard: &mut ServiceBridgeState,
    root_session_id: SessionId,
    states: &[SubagentTaskPanelState],
) {
    seed_test_live_subagent_registry_from_panel_states(guard, root_session_id, states);
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
            .seed_test_subagent_registry_entry(TestSubagentRegistryEntrySeed {
                root_session_id,
                parent_session_id: state.parent_session_id,
                task_id: state.task_id,
                public_task_id: state.public_task_id.clone(),
                display_title: state.subagent_label.clone(),
                description: state.description.clone(),
                subagent: Some(state.subagent_label.clone()),
                status: test_task_session_status_from_panel_state(state.run_state),
                result_summary: (!state.result_text.trim().is_empty())
                    .then_some(state.result_text.clone()),
            })
            .expect("seed debug subagent registry entry");
    }
}

/// Caches process-local subagent panel state without creating membership.
/// 缓存进程内子智能体面板状态，但不创建成员关系。
///
/// # Parameters
/// 参数
/// `guard` is the service state receiving process-local panel entries.
/// `guard` 是接收进程内面板条目的服务状态。
/// `states` are panel states retained for navigation and task controls.
/// `states` 是为导航与任务控制保留的面板状态。
pub(super) fn cache_test_subagent_surface_detail_states(
    guard: &mut ServiceBridgeState,
    states: &[SubagentTaskPanelState],
) {
    for state in states {
        guard.subagent_surface_detail_cache.insert(
            state.task_id,
            WebuiSubagentSurfaceDetailCacheEntry {
                task_state: state.clone(),
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
pub(crate) fn test_service_state_with_parent_session(
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
            sessions: vec![
                SessionSummary::new(
                    parent_session_id,
                    Some(String::from("project-a")),
                    "父会话",
                    "",
                    SessionState::Completed,
                    0,
                    0,
                    "刚刚",
                )
                .with_created_at("1"),
            ],
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::new(),
        }),
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
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeMap::new(),
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
    }
}

/// Owns one isolated controller data root for Core VMM service tests.
/// 为 Core VMM 服务测试持有一个隔离的控制器数据根。
pub(crate) struct TestControllerRuntimeRoot {
    /// Exact temporary data root removed when the fixture leaves scope.
    /// 夹具离开作用域时移除的精确临时数据根。
    data_root: std::path::PathBuf,
    /// Exact production V1 locator used by VMM controller tests.
    /// VMM 控制器测试使用的精确生产 V1 定位器。
    v1_storage_locator: vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    /// Exact V1 session directory retained only for corruption fault injection.
    /// 仅为损坏故障注入保留的精确 V1 会话目录。
    v1_session_directory: std::path::PathBuf,
}

impl TestControllerRuntimeRoot {
    /// Creates an isolated segmented session directory and its offline controller source.
    /// 创建隔离的分段会话目录及其离线控制器来源。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact session discoverable by the Core VMM storage adapter.
    /// `session_id` 是 Core VMM 存储适配器可发现的精确会话。
    ///
    /// # Returns
    /// 返回值
    /// A scoped fixture that owns all test files.
    /// 返回持有全部测试文件的作用域夹具。
    pub(crate) fn new(session_id: SessionId) -> Self {
        // data_root is unique per test and remains outside every real application data root.
        // data_root 对每个测试均唯一，并且位于所有真实应用数据根之外。
        let data_root =
            std::env::temp_dir().join(format!("vulcan-service-kernel-vmm-test-{}", Uuid::new_v4()));
        let v1_storage_locator =
            vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture(
                &data_root,
                &session_id.to_string(),
                1,
            )
            .expect("create exact V1 VMM session fixture");
        let v1_session_directory = data_root
            .join("code")
            .join("sessions")
            .join("v1-test-project")
            .join(format!("1970-01-01-{session_id}"));
        Self {
            data_root,
            v1_storage_locator,
            v1_session_directory,
        }
    }

    /// Builds an offline controller handle bound to this isolated data root.
    /// 构建绑定到当前隔离数据根的离线控制器句柄。
    ///
    /// # Returns
    /// 返回值
    /// A production-shaped handle that never connects to an external controller.
    /// 返回不会连接外部控制器的生产形态句柄。
    pub(crate) fn controller_runtime_handle(
        &self,
    ) -> vulcan_workspace_store::data_plane::ControllerRuntimeHandle {
        // registration supplies a complete local identity for the unused offline client proxy.
        // registration 为不会联网的离线客户端代理提供完整本地身份。
        let registration = vldb_controller_client::ClientRegistration {
            client_name: format!("vulcan-service-kernel-test-{}", Uuid::new_v4()),
            host_kind: String::from("vulcan-service-kernel-test"),
            process_id: std::process::id(),
            process_name: String::from("vulcan-service-kernel-test"),
            lease_ttl_secs: None,
        };
        // client is retained only because ControllerRuntimeHandle owns the production data-plane shape.
        // client 仅因 ControllerRuntimeHandle 持有生产数据平面结构而被保留。
        let client = vldb_controller_client::ControllerClient::new(
            vldb_controller_client::ControllerClientConfig::default(),
            registration,
        );
        vulcan_workspace_store::data_plane::ControllerRuntimeHandle::new(
            client,
            vulcan_workspace_store::data_plane::ControllerRuntimeMetadata {
                endpoint: String::from("http://127.0.0.1:0"),
                executable: String::from("offline-test-controller"),
                data_root: self.data_root.clone(),
                sqlite_path: self.data_root.join("sqlite").join("offline-test.sqlite3"),
                sqlite_binding_id: format!("offline-test-{}", Uuid::new_v4()),
                system_statistics_sqlite_path: self
                    .data_root
                    .join("statistics")
                    .join("offline-test-statistics.sqlite3"),
                system_statistics_sqlite_binding_id: format!(
                    "offline-test-statistics-{}",
                    Uuid::new_v4()
                ),
            },
        )
    }

    /// Returns the exact isolated data root owned by this fixture.
    /// 返回当前夹具持有的精确隔离数据根。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed controller data-root path.
    /// 返回借用的控制器数据根路径。
    pub(crate) fn data_root(&self) -> &std::path::Path {
        &self.data_root
    }

    /// Returns the exact production V1 locator owned by this fixture.
    /// 返回当前夹具持有的精确生产 V1 定位器。
    ///
    /// # Returns
    /// 返回值
    /// A cloned opaque locator suitable for controller ownership transfer.
    /// 返回适合转移给控制器所有权的不透明定位器副本。
    pub(crate) fn v1_storage_locator(
        &self,
    ) -> vulcan_workspace_store::workspace_store::SessionV1StorageLocator {
        self.v1_storage_locator.clone()
    }

    /// Returns the exact V1 session directory for test-only fault injection.
    /// 返回用于测试专用故障注入的精确 V1 会话目录。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed exact V1 session-directory path.
    /// 返回借用的精确 V1 会话目录路径。
    pub(crate) fn v1_session_directory(&self) -> &std::path::Path {
        &self.v1_session_directory
    }
}

impl Drop for TestControllerRuntimeRoot {
    /// Removes only the exact test-owned temporary data root.
    /// 仅移除测试持有的精确临时数据根。
    fn drop(&mut self) {
        if self
            .data_root
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("vulcan-service-kernel-vmm-test-"))
        {
            let _ = std::fs::remove_dir_all(&self.data_root);
        }
    }
}

/// Builds a minimal agent-runtime bridge for lifecycle unit tests.
/// 为生命周期单元测试构建最小代理运行时桥。
pub(super) fn test_agent_runtime_bridge() -> AgentRuntimeBridge {
    // plugin_root provides isolated path identities without requiring a Lua engine.
    // plugin_root 提供隔离路径身份，且不要求存在 Lua 引擎。
    let plugin_root = std::env::temp_dir().join(format!("vulcan-plugin-test-{}", Uuid::new_v4()));
    // provider_configuration is shared by the bridge and its empty MCP access projection.
    // provider_configuration 由桥接与其空 MCP 访问投影共同使用。
    let provider_configuration = ProviderConfigurationFile::default();
    // vulcan_plugin_runtime owns the isolated catalog later frozen into the extension generation.
    // vulcan_plugin_runtime 持有随后冻结到扩展代次中的隔离目录。
    let vulcan_plugin_runtime = vulcan_plugin_runtime::VulcanPluginRuntimeManager::without_runtime(
        plugin_root.join("system"),
        plugin_root.join("user"),
        plugin_root.join("bridge"),
        plugin_root.join("distributions"),
        plugin_root.join("state"),
        plugin_root.join("runtime"),
    );
    // extension_generation pins the exact test catalog and MCP access surface as one immutable snapshot.
    // extension_generation 把精确测试目录与 MCP 访问面固定为单个不可变快照。
    let extension_generation = Arc::new(
        vulcan_agent_runtime::agent_runtime::ExtensionGeneration::new(
            0,
            Some(vulcan_plugin_runtime.snapshot()),
            vulcan_agent_runtime::mcp::configured_mcp_runtime_access(&provider_configuration),
            None,
        ),
    );
    // publicAiRuntime is shared by ordinary provider tests and configured-inference isolation.
    // publicAiRuntime 由普通供应商测试与已配置推理隔离层共享。
    let public_ai_runtime = Arc::new(
        vulcan_provider_adapters::PublicAiRuntime::unavailable()
            .expect("test public AI runtime should initialize"),
    );
    // configuredInferenceService owns no network activity until an explicitly configured request.
    // configuredInferenceService 在显式配置请求前不拥有网络活动。
    let configured_inference_service = Arc::new(
        crate::configured_inference::ConfiguredInferenceService::new(Arc::clone(
            &public_ai_runtime,
        )),
    );
    AgentRuntimeBridge {
        provider_configuration,
        public_ai_runtime,
        configured_inference_service,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        agent_profile_catalog: AgentProfileCatalog::default(),
        subagent_catalog: Arc::new(vulcan_task_session::SubagentCatalog::default()),
        luaskills_runtime: None,
        vulcan_plugin_runtime,
        extension_generation,
        task_runtime: Arc::new(TaskRuntime::new()),
        question_runtime: Arc::new(QuestionRuntime::new()),
        command_session_runtime: Arc::new(CommandSessionRuntime::new()),
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
            "execution_id": format!("test-execution-{public_task_id}"),
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": result,
        }
    })))
}
