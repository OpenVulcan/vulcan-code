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
pub(crate) fn seed_test_live_subagent_registry_from_panel_states(
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
        SubagentTaskRunState::Closed => vulcan_task_session::TaskSessionStatus::Archived,
    }
}

/// Seeds one authoritative running child for wait-agent coordination tests.
/// 为 wait-agent 协调测试播种一个权威的运行中子代理。
///
/// # Parameters
/// 参数
/// `guard` owns the process-local task runtime registry.
/// `guard` 持有进程内任务运行时注册表。
/// `parent_session_id` is the direct parent whose wait scope observes the child.
/// `parent_session_id` 是等待范围观察该子代理的直接父会话。
///
/// # Returns
/// 返回值
/// The stable child session identifier inserted into the registry.
/// 返回插入注册表的稳定子会话标识。
pub(super) fn seed_test_running_subagent_for_parent(
    guard: &mut ServiceBridgeState,
    parent_session_id: SessionId,
) -> SessionId {
    // task_id identifies the active child observed by wait_agent.
    // task_id 标识 wait_agent 观察的活动子代理。
    let task_id = SessionId::new_v4();
    // state carries the exact running lifecycle projection consumed by the registry seeder.
    // state 携带注册表播种器消费的精确运行中生命周期投影。
    let state = SubagentTaskPanelState {
        task_id,
        public_task_id: format!("wait-child-{task_id}"),
        parent_session_id,
        subagent_label: String::from("wait-child"),
        description: Some(String::from("wait coordination fixture")),
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    };
    seed_test_live_subagent_registry_from_panel_states(
        guard,
        parent_session_id,
        std::slice::from_ref(&state),
    );
    task_id
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
pub(crate) fn test_service_shell_host() -> Arc<dyn ServiceShellHost> {
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
        native_vmm_maintenance: None,
        vce_runtime: None,
        vce_watcher: None,
        workspace_watcher: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        knowledge_runtime: None,
        luaskills_runtime: None,
        host_database_providers: None,
        cached_luaskills_catalog: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        provider_manifest_sync: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeMap::new(),
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

/// Owns one isolated host database root for Core VMM service tests.
/// 为 Core VMM 服务测试持有一个隔离的宿主数据库根。
pub(crate) struct TestHostRuntimeRoot {
    /// Exact session identity owned by this isolated fixture.
    /// 当前隔离夹具拥有的精确会话身份。
    session_id: SessionId,
    /// Exact temporary data root removed when the fixture leaves scope.
    /// 夹具离开作用域时移除的精确临时数据根。
    data_root: std::path::PathBuf,
    /// Exact production V1 locator used by VMM storage tests.
    /// VMM 存储测试使用的精确生产 V1 定位器。
    v1_storage_locator: vulcan_workspace_store::workspace_store::SessionV1StorageLocator,
    /// Exact V1 session directory retained only for corruption fault injection.
    /// 仅为损坏故障注入保留的精确 V1 会话目录。
    v1_session_directory: std::path::PathBuf,
}

impl TestHostRuntimeRoot {
    /// Creates an isolated segmented session directory and its offline host-storage source.
    /// 创建隔离的分段会话目录及其离线宿主存储来源。
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
        // The short prefix leaves room for Lance table, transaction and UUID suffixes below Win32's legacy
        // path budget while the UUID still makes the root unique.
        // 短前缀为 Lance 表名、事务目录与 UUID 后缀预留 Win32 传统路径预算，同时 UUID 仍保证根目录唯一。
        let data_root = std::env::temp_dir().join(format!("vsk-{}", Uuid::new_v4()));
        let v1_storage_locator =
            vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture(
                &data_root,
                &session_id.to_string(),
                1,
            )
            .expect("create exact V1 VMM session fixture");
        let v1_session_directory = data_root
            .join("sessions")
            .join("v1-test-project")
            .join(format!("1970-01-01-{session_id}"));
        Self {
            session_id,
            data_root,
            v1_storage_locator,
            v1_session_directory,
        }
    }

    /// Builds an offline host runtime handle bound to this isolated data root.
    /// 构建绑定到当前隔离数据根的离线宿主运行时句柄。
    ///
    /// # Returns
    /// 返回值
    /// A production-shaped handle that performs all storage work in process.
    /// 返回在进程内完成全部存储工作的生产形态句柄。
    pub(crate) fn host_runtime_handle(
        &self,
    ) -> vulcan_workspace_store::data_plane::HostRuntimeHandle {
        vulcan_workspace_store::data_plane::HostRuntimeHandle::new_for_test(
            vulcan_workspace_store::data_plane::HostRuntimeMetadata {
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

    /// Builds an offline runtime handle bound to this fixture's isolated APP database.
    /// 构建绑定到本夹具隔离 APP 数据库的离线运行时句柄。
    ///
    /// # Returns
    /// 返回值
    /// The production-shaped runtime handle the service fixtures consume.
    /// 返回服务夹具所消费的生产形态运行时句柄。
    pub(crate) async fn attached_host_runtime_handle(
        &self,
    ) -> vulcan_workspace_store::data_plane::HostRuntimeHandle {
        // sqlite_path is the exact APP database already populated by the V1 fixture.
        // sqlite_path 是已由 V1 夹具填充的精确 APP 数据库。
        let sqlite_path = self.data_root.join("sqlite").join("app-workspace.sqlite3");
        // sqlite_binding_id uniquely owns this fixture's runtime binding.
        // sqlite_binding_id 唯一标识该夹具的运行时绑定。
        let sqlite_binding_id = format!("attached-test-{}", Uuid::new_v4());
        // host_runtime carries the exact metadata consumed by the service selection preflight.
        // host_runtime 携带服务选择预检所消费的精确元数据。
        let host_runtime = vulcan_workspace_store::data_plane::HostRuntimeHandle::new_for_test(
            vulcan_workspace_store::data_plane::HostRuntimeMetadata {
                data_root: self.data_root.clone(),
                sqlite_path: sqlite_path.clone(),
                sqlite_binding_id: sqlite_binding_id.clone(),
                system_statistics_sqlite_path: sqlite_path,
                system_statistics_sqlite_binding_id: sqlite_binding_id,
            },
        );
        // The offline fixture publishes files first; production creation below republishes the same exact session through the strict SQLite index boundary.
        // 离线夹具先发布文件；下面的生产创建入口通过严格 SQLite 索引边界重新发布同一精确会话。
        std::fs::remove_dir_all(&self.v1_session_directory)
            .expect("remove offline V1 fixture before indexed publication");
        // session_id_text keeps the metadata identity stable for every borrowed creation field.
        // session_id_text 为所有借用的创建字段保持稳定的 metadata 身份。
        let session_id_text = self.session_id.to_string();
        // metadata is the authoritative V1 document published by the indexed creation path.
        // metadata 是由索引创建路径发布的权威 V1 文档。
        let metadata = vulcan_session_core::v1::config::MetadataDocument::new(
            session_id_text.as_str(),
            "v1-test-project",
            "V1 persistence test session",
            1,
        );
        // runtime_lock starts from the same fresh state as the original storage fixture.
        // runtime_lock 从与原存储夹具相同的全新状态开始。
        let runtime_lock = vulcan_session_core::v1::context::RuntimeLock {
            initialization_state:
                vulcan_session_core::v1::context::RuntimeInitializationState::FreshUninitialized,
            current_compaction_id: None,
            locked_sources: Vec::new(),
        };
        vulcan_workspace_store::workspace_store::create_indexed_session_v1(
            &host_runtime,
            &vulcan_workspace_store::workspace_store::CreateSessionV1Request {
                project_id: "v1-test-project",
                storage_partition:
                    vulcan_workspace_store::workspace_store::SESSION_V1_STORAGE_PARTITION,
                date_bucket: "1970-01-01",
                metadata: &metadata,
                runtime_lock: &runtime_lock,
                compactions: &[],
                events: &[],
            },
        )
        .await
        .expect("publish indexed V1 service fixture");
        host_runtime
    }

    /// Returns the exact isolated data root owned by this fixture.
    /// 返回当前夹具持有的精确隔离数据根。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed host data-root path.
    /// 返回借用的宿主数据根路径。
    pub(crate) fn data_root(&self) -> &std::path::Path {
        &self.data_root
    }

    /// Returns the exact production V1 locator owned by this fixture.
    /// 返回当前夹具持有的精确生产 V1 定位器。
    ///
    /// # Returns
    /// 返回值
    /// A cloned opaque locator suitable for host ownership transfer.
    /// 返回适合转移给宿主所有权的不透明定位器副本。
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

impl Drop for TestHostRuntimeRoot {
    /// Removes only the exact test-owned temporary data root.
    /// 仅移除测试持有的精确临时数据根。
    fn drop(&mut self) {
        if self
            .data_root
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("vsk-"))
        {
            let _ = std::fs::remove_dir_all(&self.data_root);
        }
    }
}

/// Builds a minimal agent-runtime bridge for lifecycle unit tests.
/// 为生命周期单元测试构建最小代理运行时桥。
pub(crate) fn test_agent_runtime_bridge() -> AgentRuntimeBridge {
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
    let internal_ai_port = Arc::new(crate::internal_ai::ConfiguredInferenceHostAiPort::new(
        Arc::clone(&configured_inference_service),
    ));
    AgentRuntimeBridge {
        provider_configuration,
        public_ai_runtime,
        configured_inference_service,
        internal_ai_port,
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
