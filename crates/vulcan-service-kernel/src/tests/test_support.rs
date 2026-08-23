//! Shared service-kernel test support.
//! service-kernel 共享测试支撑。

use super::*;

use vulcan_agent_runtime::agent_runtime::task_runtime::TestSubagentRegistryEntrySeed;

/// Builds one deterministic Provider call identity shared by lifecycle test fixtures.
/// 构建一个由生命周期测试夹具共享的确定性 Provider 调用身份。
///
/// # Parameters
/// 参数
/// `owner_kind` identifies the logical runtime owner used by the fixture.
/// `owner_kind` 标识夹具使用的逻辑运行时所有者。
/// `execution_id` identifies the exact physical execution represented by the fixture.
/// `execution_id` 标识夹具表示的精确物理执行。
///
/// # Returns
/// 返回值
/// A complete immutable Provider identity suitable for Started and Completed observations.
/// 返回适用于 Started 与 Completed 观测的完整不可变 Provider 身份。
fn test_provider_call_identity(
    owner_kind: &str,
    execution_id: &str,
) -> vulcan_workbench_model::SessionProviderCallIdentity {
    vulcan_workbench_model::SessionProviderCallIdentity {
        logical_request_id: format!("logical-request-{execution_id}"),
        execution_id: execution_id.to_string(),
        turn_id: None,
        attempt_index: 0,
        retry_number: 0,
        owner_kind: owner_kind.to_string(),
        owner_id: owner_kind.to_string(),
        owner_name_snapshot: owner_kind.to_string(),
        operation: owner_kind.to_string(),
        provider_id: String::from("provider-test"),
        provider_name_snapshot: String::from("Provider Test"),
        model_id: String::from("model-test"),
        model_name_snapshot: String::from("Model Test"),
        connection_id: String::from("connection-test"),
        service_entry_id: String::from("public-api"),
        protocol_profile_id: String::from("openai.responses"),
        manifest_revision: 1,
        manifest_generation: 1,
        binding_source: String::from("system_manifest"),
        binding_rule: Some(String::from("test-model-rule")),
        route_revision: String::from("sha256:test-route"),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
    }
}

/// Records one durable pre-dispatch Provider Started boundary for refresh-race tests.
/// 为刷新竞态测试记录一个可靠的 Provider 分派前 Started 边界。
///
/// # Parameters
/// 参数
/// `runtime_state` is the authoritative in-memory session state receiving the call boundary.
/// `runtime_state` 是接收调用边界的权威内存会话状态。
/// `owner_kind` identifies the logical runtime owner used by the fixture.
/// `owner_kind` 标识夹具使用的逻辑运行时所有者。
/// `execution_id` identifies the exact physical execution represented by the fixture.
/// `execution_id` 标识夹具表示的精确物理执行。
///
/// # Returns
/// 返回值
/// The stable physical Provider call identifier inserted into the ledger.
/// 返回插入账本的稳定物理 Provider 调用标识。
pub(super) fn record_started_test_provider_call(
    runtime_state: &mut SessionRuntimeStateEntry,
    owner_kind: &str,
    execution_id: &str,
) -> String {
    // provider_call_id is the stable key checked after an overlapping refresh commits.
    // provider_call_id 是重叠刷新提交后检查的稳定键。
    let provider_call_id = format!("provider-call-{execution_id}");
    // identity freezes the route and owner facts attached to this physical call.
    // identity 冻结附着到本次物理调用的路由与所有者事实。
    let identity = test_provider_call_identity(owner_kind, execution_id);
    assert!(runtime_state.provider_call_ledger.observe_started(
        vulcan_workbench_model::SessionProviderCallStarted {
            provider_call_id: provider_call_id.clone(),
            identity: identity.clone(),
            prepared_at_ms: 1_780_000_000_050,
            request_assembly_fingerprint: Some(String::from("sha256:test-request")),
            context_generation_id: None,
            context_start_record_id: None,
            model_route_fingerprint: Some(String::from("sha256:test-route")),
            toolset_fingerprint: None,
            system_prompt_fingerprint: None,
            request_text_measurement: None,
            budget_accounting: None,
        }
    ));
    provider_call_id
}

/// Records one complete successful Provider lifecycle for aggregate-usage tests.
/// 为聚合用量测试记录一个完整成功的 Provider 生命周期。
///
/// # Parameters
/// 参数
/// `runtime_state` is the authoritative in-memory session state receiving the lifecycle.
/// `runtime_state` 是接收生命周期的权威内存会话状态。
/// `owner_kind` identifies the logical runtime owner used by the fixture.
/// `owner_kind` 标识夹具使用的逻辑运行时所有者。
/// `execution_id` identifies the exact physical execution represented by the fixture.
/// `execution_id` 标识夹具表示的精确物理执行。
///
/// # Returns
/// 返回值
/// Returns no value and panics when the deterministic fixture violates ledger invariants.
/// 不返回值；当确定性夹具违反账本约束时触发 panic。
pub(super) fn record_completed_test_provider_call(
    runtime_state: &mut SessionRuntimeStateEntry,
    owner_kind: &str,
    execution_id: &str,
) {
    // provider_call_id is created by the exact Started observation required before completion.
    // provider_call_id 由完成前必须存在的精确 Started 观测创建。
    let provider_call_id =
        record_started_test_provider_call(runtime_state, owner_kind, execution_id);
    // identity reproduces the immutable route identity required by the terminal observation.
    // identity 重建终态观测要求的不可变路由身份。
    let identity = test_provider_call_identity(owner_kind, execution_id);
    assert!(runtime_state.provider_call_ledger.observe_completed(
        vulcan_workbench_model::SessionProviderCallCompleted {
            provider_call_id,
            identity,
            status: vulcan_workbench_model::SessionProviderCallStatus::Succeeded,
            provider_usage_evidence: vulcan_workbench_model::SessionProviderUsageEvidence {
                availability: vulcan_workbench_model::SessionProviderUsageAvailability::Missing,
                provider_usage: None,
            },
            prepared_at_ms: 1_780_000_000_050,
            dispatched_at_ms: Some(1_780_000_000_051),
            first_event_at_ms: Some(1_780_000_000_052),
            first_token_at_ms: Some(1_780_000_000_053),
            completed_at_ms: 1_780_000_000_099,
            dispatch_outcome: Some(String::from("dispatched")),
            dispatch_outcome_known: Some(true),
            semantic_output_observed: Some(true),
            semantic_output_observed_known: Some(true),
            replay_safe: Some(false),
            error_category: None,
            error_code: None,
        }
    ));
}

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
    let project_root = TestSegmentedProjectRoot::new(format!("registry-fixture-{root_session_id}"));
    let session_dir = project_root.path().join(format!(
        "{NAVIGATION_TEST_ROOT_CREATED_DATE_FOR_SUPPORT}-{root_session_id}"
    ));
    std::fs::create_dir_all(&session_dir).expect("create registry fixture session dir");
    write_test_subagent_root_metadata(&session_dir, root_session_id);
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

/// Writes the exact root task-mirror index used by persisted subagent registry fixtures.
/// 写入持久化子代理注册表夹具使用的精确根任务镜像索引。
///
/// # Parameters
/// 参数
/// `session_directory` is the exact root-session directory and `session_id` is its owner.
/// `session_directory` 是精确根会话目录，`session_id` 是其所有者。
///
/// # Returns
/// 返回值
/// This fixture helper returns nothing and panics only when isolated file setup fails.
/// 此夹具辅助函数不返回值，仅在隔离文件准备失败时触发 panic。
fn write_test_subagent_root_metadata(session_directory: &std::path::Path, session_id: SessionId) {
    // metadata is the authoritative exact task membership index; child directories are never enumerated.
    // metadata 是权威的精确任务成员索引；绝不枚举子目录。
    let metadata = serde_json::json!({
        "schema_version": 1,
        "session_id": session_id.to_string(),
        "project_tag": "project-a",
        "created_at": "1",
        "updated_at": "1",
        "archived_at": null,
        "project_id": "project-a",
        "title": "subagent registry fixture",
        "title_source": "test",
        "canonical_source": "segmented_rollout",
        "active_segment": "s001.jsonl",
        "task_dir": "task",
        "next_global_seq": 0,
        "message_count": 0,
        "last_message_id": null,
        "last_message_fingerprint": null,
        "latest_rollback_seq": null,
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
        "tasks": [],
        "fork_source": null
    });
    // encoded_metadata is fully serialized before the exact index file is published.
    // encoded_metadata 在精确索引文件发布前完成完整序列化。
    let mut encoded_metadata =
        serde_json::to_vec_pretty(&metadata).expect("encode subagent root metadata");
    encoded_metadata.push(b'\n');
    std::fs::write(session_directory.join("metadata.json"), encoded_metadata)
        .expect("write subagent root metadata");
    std::fs::write(session_directory.join("s001.jsonl"), b"").expect("write subagent root segment");
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
            .map(|conversation_state| {
                display_messages_from_task_conversation_state(
                    conversation_state,
                    state.description.as_deref(),
                )
            })
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
        session_goal_dispatch_versions: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeMap::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        active_stream_tasks: BTreeMap::new(),
        session_terminal_persistence_owners: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        vmm_controller_registry: Default::default(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    }
}

/// Owns one isolated controller data root for Core VMM service tests.
/// 为 Core VMM 服务测试持有一个隔离的控制器数据根。
pub(crate) struct TestControllerRuntimeRoot {
    /// Exact temporary data root removed when the fixture leaves scope.
    /// 夹具离开作用域时移除的精确临时数据根。
    data_root: std::path::PathBuf,
    /// Exact segmented session directory created inside the temporary data root.
    /// 在临时数据根内创建的精确分段会话目录。
    session_directory: std::path::PathBuf,
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
        // session_dir uses the exact production segmented-session directory depth.
        // session_dir 使用与生产一致的分段会话目录层级。
        let session_dir = data_root
            .join("sessions")
            .join("project-a")
            .join(format!("1970-01-01-{session_id}"));
        std::fs::create_dir_all(session_dir.join("task"))
            .expect("create isolated VMM session directory");
        // metadata is the minimal valid production session index document needed for exact id lookup.
        // metadata 是按精确标识查找所需的最小有效生产会话索引文档。
        let metadata = serde_json::json!({
            "schema_version": 1,
            "session_id": session_id.to_string(),
            "project_tag": "project-a",
            "created_at": "1",
            "updated_at": "1",
            "archived_at": null,
            "project_id": "project-a",
            "title": "VMM test session",
            "title_source": "test",
            "canonical_source": "segmented_rollout",
            "active_segment": "s001.jsonl",
            "task_dir": "task",
            "next_global_seq": 0,
            "message_count": 0,
            "last_message_id": null,
            "last_message_fingerprint": null,
            "latest_rollback_seq": null,
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
            "tasks": [],
            "fork_source": null
        });
        // encoded_metadata is written once before the controller can discover the session.
        // encoded_metadata 在控制器发现会话前一次性写入。
        let mut encoded_metadata =
            serde_json::to_vec_pretty(&metadata).expect("encode VMM session metadata");
        encoded_metadata.push(b'\n');
        std::fs::write(session_dir.join("metadata.json"), encoded_metadata)
            .expect("write isolated VMM session metadata");
        Self {
            data_root,
            session_directory: session_dir,
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
            String::from("http://127.0.0.1:0"),
            String::from("offline-test-controller"),
            self.data_root.clone(),
            self.data_root.join("sqlite").join("offline-test.sqlite3"),
            format!("offline-test-{}", Uuid::new_v4()),
            self.data_root
                .join("statistics")
                .join("offline-test-statistics.sqlite3"),
            format!("offline-test-statistics-{}", Uuid::new_v4()),
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

    /// Returns the exact isolated segmented session directory owned by this fixture.
    /// 返回当前夹具持有的精确隔离分段会话目录。
    ///
    /// # Returns
    /// 返回值
    /// The borrowed exact session-directory path.
    /// 返回借用的精确会话目录路径。
    pub(crate) fn session_directory(&self) -> &std::path::Path {
        &self.session_directory
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
