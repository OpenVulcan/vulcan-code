//! detached subagent tests.
//! detached subagent tests。

use super::*;
use crate::agent_stream::provider_v1_sink::ProviderV1FinishedDurability;
use crate::context_compaction::{
    ContextCompactionCommitMode, ContextCompactionFinishOutcome, WebuiContextCompactionExecution,
    apply_webui_context_compaction_failure_to_workspace, fail_webui_context_compaction_execution,
    finish_webui_context_compaction_execution,
};
use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
    PromptCompactionExecution, PromptCompactionExecutionProfile,
};
use vulcan_context::ContextCompactionPolicy;
use vulcan_context_compaction::{CompactionPlan, CompactionResult};
use vulcan_session_core::v1::ids::Seq;

/// Builds one frozen tail report aligned with a test policy and canonical suffix.
/// 构建与测试策略及规范后缀对齐的冻结尾部报告。
///
/// # Parameters
/// 参数
/// `policy` is the exact plan policy and `retained_message_ids` are ordered canonical suffix IDs.
/// `policy` 是精确计划策略，`retained_message_ids` 是有序规范后缀 ID。
///
/// # Returns
/// 返回值
/// Returns a structurally consistent single-segment fixture for commit-boundary tests.
/// 返回供提交边界测试使用的结构一致单执行段夹具。
fn context_compaction_test_tail_selection(
    policy: ContextCompactionPolicy,
    retained_message_ids: &[&str],
) -> vulcan_context_compaction::CompactionTailSelectionReport {
    vulcan_context_compaction::CompactionTailSelectionReport {
        context_limit: policy.tail_context_limit,
        policy_source: policy.tail_policy_source,
        nominal_token_budget: policy.preserve_recent_tokens,
        segment_limit: policy.tail_turns,
        retained_segment_count: usize::from(!retained_message_ids.is_empty()),
        retained_message_ids: retained_message_ids
            .iter()
            .map(|message_id| (*message_id).to_string())
            .collect(),
        retained_token_estimate: 0,
        previewed_tool_output_count: 0,
        omitted_tool_output_token_estimate: 0,
        newest_segment_exceeds_nominal_budget: false,
    }
}

/// Registers one pending active-stream owner for context-compaction commit tests.
/// 为上下文压缩提交测试注册一个保持 pending 的活动流所有者。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative active-stream registry.
/// `state` 持有权威活动流注册表。
/// `session_id` selects the test session slot.
/// `session_id` 选择测试会话槽位。
///
/// # Returns
/// 返回值
/// Returns the exact task id retained by the registry.
/// 返回注册表保留的精确 task id。
fn register_pending_context_compaction_test_stream(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Uuid {
    // task_id is shared by the retained entry and the commit authority under test.
    // task_id 由保留条目与被测提交权限共享。
    let task_id = Uuid::new_v4();
    spawn_registered_active_stream_task_with_mode(
        state,
        session_id,
        task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async {
            std::future::pending::<()>().await;
        },
    )
    .expect("pending context-compaction test stream should register");
    task_id
}

/// Returns rebuilt subagent navigation and fails the test on registry projection errors.
/// 返回重建后的智能体导航，并在注册表投影错误时让测试失败。
fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    super::subagent_navigation_snapshot_from_state(guard, workspace_snapshot)
        .expect("subagent navigation projection should succeed")
}

/// Captures emitted service events for projection tests.
/// 为投影测试捕获已发出的服务事件。
struct CapturingServiceEventSink {
    /// Shared event buffer populated by the sink.
    /// sink 写入的共享事件缓冲区。
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl vulcan_service_api::ServiceEventSink for CapturingServiceEventSink {
    /// Appends one emitted service event to the shared test buffer.
    /// 将单个已发出的服务事件追加到共享测试缓冲区。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().expect("event sink lock").push(event);
    }
}

/// Verifies TaskSession terminal reports use the memory-only fast path without completed-turn hooks.
/// 验证 TaskSession 终态报告使用仅内存快路径且不会生成完成回合 hook。
#[test]
fn task_session_execution_report_fast_path_skips_completion_hook() {
    let root_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            root_session_id,
            Some(String::from("project-a")),
            "主会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: root_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let state = Arc::new(Mutex::new(ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: Some(workspace_snapshot),
        pending_prompt_process_mirrors: BTreeMap::new(),
        agent_runtime: None,
        native_vmm_maintenance: None,
        workspace_watcher: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        knowledge_runtime: None,
        luaskills_runtime: None,
        host_database_providers: None,
        cached_luaskills_catalog: None,
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
        permission_judge: Arc::new(Default::default()),
        project_resource_catalog_registry: Default::default(),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
        vmm_controller_registry: Default::default(),
        browser_manager: crate::browser::BrowserManager::new(),
        mcp_server: None,
    }));
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("子代理完成")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        provider_failure_category: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should apply to memory");
    let guard = state
        .lock()
        .expect("service state lock should be available after report apply");
    let applied_workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("applied workspace should remain retained");

    assert!(applied.completion_request.is_none());
    assert!(!workspace_snapshot_contains_regular_session(
        applied_workspace,
        task_session_id
    ));
    assert_eq!(
        applied_workspace.session_run_phases.get(&task_session_id),
        Some(&SessionRunPhase::Ready)
    );
    let records = applied_workspace
        .session_runtime_state
        .get(&task_session_id)
        .expect("task session report should retain process-local runtime state")
        .conversation_state
        .records
        .as_slice();
    assert!(
        records
            .iter()
            .any(|record| record.body.contains("子代理完成"))
    );
    assert!(
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.hydrated_session_ids.contains(&task_session_id))
    );
}

/// Verifies a poisoned conversation-hook runtime rejects a regular terminal report before workspace commit.
/// 验证会话 Hook 运行时中毒时，普通会话终态报告会在工作区提交前被拒绝。
#[test]
fn regular_execution_report_hook_lock_failure_preserves_workspace_state() {
    // session_id identifies the regular session whose terminal state must remain uncommitted.
    // session_id 标识终态不得提交的普通会话。
    let session_id = SessionId::new_v4();
    // controller_root keeps this test's isolated VMM files alive through every assertion.
    // controller_root 在全部断言期间保持本测试隔离 VMM 文件有效。
    let controller_root = TestHostRuntimeRoot::new(session_id);
    // service_state starts with one registered V1 session so projection preconditions are real.
    // service_state 以一个已注册 V1 会话开始，使投影前置条件真实成立。
    let service_state = test_service_state_with_parent_session(session_id);
    let handle = crate::session_runtime_registry_v1::SessionHandle::new(
        session_id,
        controller_root.v1_storage_locator(),
        vulcan_session_core::v1::document::SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Terminal hook test",
            1,
        ),
    );
    service_state
        .session_runtime_registry
        .register(handle)
        .expect("terminal hook V1 session should register");
    let state = Arc::new(Mutex::new(service_state));
    {
        // guard initializes the exact pre-commit workspace facts under one service-state lock.
        // guard 在同一服务状态锁内初始化精确的提交前工作区事实。
        let mut guard = state.lock().expect("lock regular session fixture");
        guard.controller_runtime = Some(controller_root.host_runtime_handle());
        guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist")
            .sessions[0]
            .status = SessionState::Running;
        guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist")
            .session_run_phases
            .insert(session_id, SessionRunPhase::Thinking);
    }
    // agent_runtime owns the hook mutex deliberately poisoned below.
    // agent_runtime 持有下方刻意中毒的 Hook 互斥锁。
    let agent_runtime = test_agent_runtime_bridge();
    // poisoned_hook_runtime shares the exact mutex used by terminal report commit.
    // poisoned_hook_runtime 共享终态报告提交使用的精确互斥锁。
    let poisoned_hook_runtime = Arc::clone(&agent_runtime.conversation_hook_runtime);
    // poison_result proves the deliberate panic actually poisoned the shared mutex.
    // poison_result 证明刻意触发的 panic 确实使共享互斥锁中毒。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // _guard keeps the mutex held until the deliberate panic marks it poisoned.
        // _guard 将互斥锁保持到刻意 panic，使其被标记为中毒。
        let _guard = poisoned_hook_runtime
            .lock()
            .expect("hook runtime should lock before deliberate panic");
        panic!("poison conversation hook runtime for terminal commit test");
    }));
    assert!(poison_result.is_err());
    // report is a successful regular user turn and therefore owns completed-turn hook preparation.
    // report 是成功的普通用户回合，因此持有完成回合 Hook 准备操作。
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(String::from("主会话完成")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        provider_failure_category: None,
        controller_events: Vec::new(),
    };

    // error captures the explicit hook-lock failure without requiring the success type to be Debug.
    // error 捕获显式的 Hook 锁失败，同时避免要求成功返回类型实现 Debug。
    let error = match apply_execution_report_to_memory(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
        report,
    ) {
        Ok(_) => panic!("poisoned hook runtime must reject terminal report commit"),
        Err(error) => error,
    };

    assert!(
        error.contains("failed to lock conversation hook runtime"),
        "unexpected terminal rejection: {error}"
    );
    // workspace proves the detached runtime-state draft never became visible.
    // workspace 证明分离的运行态草稿从未变为可见状态。
    // guard reads all post-failure assertions from one consistent service-state snapshot.
    // guard 从同一份一致的服务状态快照读取全部失败后断言。
    let guard = state
        .lock()
        .expect("service state lock should remain healthy");
    // workspace is the authoritative retained snapshot after the rejected commit.
    // workspace 是提交被拒绝后的权威保留快照。
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace fixture should remain available");
    assert_eq!(workspace.sessions[0].status, SessionState::Running);
    assert_eq!(
        workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Thinking)
    );
    assert!(!workspace.session_runtime_state.contains_key(&session_id));
}

/// Verifies stale VMM ownership is ignored while no VMM hook is active.
/// 验证没有活动 VMM Hook 时会忽略陈旧的 VMM 所有权。
#[test]
fn regular_execution_report_ignores_stale_vmm_owner_without_active_hook() {
    // session_id identifies the regular session whose exact terminal ownership is under test.
    // session_id 标识正在测试精确终态所有权的普通会话。
    let session_id = SessionId::new_v4();
    // controller_root keeps this test's isolated VMM files alive through every assertion.
    // controller_root 在全部断言期间保持本测试隔离 VMM 文件有效。
    let controller_root = TestHostRuntimeRoot::new(session_id);
    // service_state keeps the VMM hook unavailable while retaining one stale controller lease.
    // service_state 保持 VMM Hook 不可用，同时保留一条陈旧控制器租约。
    let service_state = test_service_state_with_parent_session(session_id);
    let handle = crate::session_runtime_registry_v1::SessionHandle::new(
        session_id,
        controller_root.v1_storage_locator(),
        vulcan_session_core::v1::document::SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Terminal ownership test",
            1,
        ),
    );
    service_state
        .session_runtime_registry
        .register(handle)
        .expect("terminal ownership V1 session should register");
    let state = Arc::new(Mutex::new(service_state));
    state
        .lock()
        .expect("lock VMM mismatch controller fixture")
        .controller_runtime = Some(controller_root.host_runtime_handle());
    // controller_context supplies the process registry without retaining the service lock.
    // controller_context 在不保留服务锁的情况下提供进程注册表。
    let vmm_controller_registry = {
        let guard = state.lock().expect("lock VMM mismatch fixture");
        guard.vmm_controller_registry.clone()
    };
    // vmm_storage_locator is the exact V1 target shared by seeding and opening.
    // vmm_storage_locator 是播种与打开共享的精确 V1 目标。
    let vmm_storage_locator = controller_root.v1_storage_locator();
    // seeded_vmm_state is the controller-owned durable lease with a deliberately divergent turn.
    // seeded_vmm_state 是控制器持有的持久租约，并带有刻意分歧的回合。
    let mut seeded_vmm_state = vulcan_vmm::VmmSessionMemoryState::new(session_id.to_string(), "1")
        .expect("VMM test state should initialize");
    seeded_vmm_state
        .apply_recall_batch(
            &session_id.to_string(),
            "user-anchor",
            &vulcan_vmm::VmmRecallBatch {
                ttl_turns: 1,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                queries: Vec::new(),
                items: vec![vulcan_vmm::ContextItem {
                    text: String::from("terminal ownership memory"),
                    memory_id: 901,
                    ..vulcan_vmm::ContextItem::default()
                }],
            },
            "1",
        )
        .expect("VMM mismatch lease should be created");
    seeded_vmm_state
        .register_active_turn("vmm-divergent-turn", "2")
        .expect("VMM divergent turn should be registered");
    vmm_controller_registry
        .seed_for_test(vmm_storage_locator.clone(), session_id, seeded_vmm_state)
        .expect("seed VMM mismatch controller");
    // vmm_controller is the exact registry instance later used by terminal completion.
    // vmm_controller 是终态完成阶段随后使用的同一个注册表实例。
    let vmm_controller = vmm_controller_registry
        .get_or_open(vmm_storage_locator, session_id, "3")
        .expect("open seeded VMM controller");
    // agent_runtime owns the real conversation Hook turn that must survive mismatch rejection.
    // agent_runtime 拥有必须在不匹配拒绝后保留的真实会话 Hook 回合。
    let agent_runtime = test_agent_runtime_bridge();
    // retained_hook_turn_id captures the exact owner before the terminal attempt.
    // retained_hook_turn_id 捕获终态尝试前的精确所有者。
    let retained_hook_turn_id = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("lock Hook mismatch fixture");
        hook_runtime.record_queued_user_prompt(
            session_id,
            "prompt-1",
            Some(String::from("project-a")),
            String::from("D:\\openvulcan\\vulcan-code"),
            PendingSessionPromptSource::UserFollowup,
            "complete this turn",
        );
        hook_runtime
            .active_turn_id(session_id)
            .expect("Hook turn should be active")
            .to_string()
    };
    assert_ne!(retained_hook_turn_id, "vmm-divergent-turn");
    // report has no admitted preparation, so stale VMM state cannot authorize terminal participation.
    // report 不含已准入的准备事实，因此陈旧 VMM 状态不能授权终态参与。
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(String::from("terminal response")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        provider_failure_category: None,
        controller_events: Vec::new(),
    };
    // outcome proves a stopped service cannot inject a stale VMM identity check into terminal commit.
    // outcome 证明未启动服务不能把陈旧 VMM 身份校验注入终态提交。
    let outcome = apply_execution_report_to_memory(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
        report,
    )
    .expect("stopped VMM service must not reject the ordinary terminal commit");
    assert!(outcome.completion_request.is_some());
    assert_eq!(
        agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("lock Hook runtime after mismatch")
            .active_turn_id(session_id),
        None
    );
    // persisted_vmm_state confirms the stopped service neither validates nor consumes stale ownership.
    // persisted_vmm_state 确认未启动服务既不校验也不消费陈旧所有权。
    let persisted_vmm_state = vmm_controller
        .snapshot()
        .expect("read VMM controller after mismatch");
    assert_eq!(persisted_vmm_state.completed_turn_sequence, 0);
    assert_eq!(
        persisted_vmm_state.active_turn_id.as_deref(),
        Some("vmm-divergent-turn")
    );
}

/// Verifies a Provider fallback commits a durable rejection instead of authorizing continuation.
/// 验证 Provider 回退会提交可持久化拒绝，而不是授权继续执行。
#[tokio::test]
async fn context_compaction_fallback_returns_rejected_terminal_outcome() {
    // session_id identifies the regular session receiving the failed compaction attempt.
    // session_id 标识接收失败压缩尝试的普通会话。
    let session_id = SessionId::new_v4();
    // state retains the exact workspace mutated by the completion boundary.
    // state 保留由完成边界修改的精确工作区。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // controller_root keeps this test's isolated VMM files alive through every assertion.
    // controller_root 在全部断言期间保持本测试隔离 VMM 文件有效。
    let controller_root = TestHostRuntimeRoot::new(session_id);
    state
        .lock()
        .expect("lock compaction fallback controller fixture")
        .controller_runtime = Some(controller_root.host_runtime_handle());
    // active_stream_task_id proves the fallback commit belongs to the current parent stream.
    // active_stream_task_id 证明回退提交属于当前父流。
    let active_stream_task_id = register_pending_context_compaction_test_stream(&state, session_id);
    // section_id anchors the failure record to the visible compaction status section.
    // section_id 将失败记录锚定到可见压缩状态分段。
    let section_id = {
        // guard initializes the status section and in-progress phase under one lock.
        // guard 在同一把锁内初始化状态分段与进行中阶段。
        let mut guard = state.lock().expect("lock compaction fallback fixture");
        // workspace is the authoritative regular-session snapshot for this fixture.
        // workspace 是当前夹具的权威普通会话快照。
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist");
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
        append_webui_context_compaction_status_message_to_workspace(
            workspace,
            session_id,
            "预算说明",
            "auto_budget",
        )
        .expect("compaction status section should be valid")
    };
    // plan supplies a structurally valid result even though fallback prevents candidate use.
    // plan 提供结构有效的结果，即使回退分支不会使用该候选。
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: String::from("fallback-compaction"),
        created_at: String::from("1780000000100"),
        policy,
        tail_selection: context_compaction_test_tail_selection(policy, &[]),
    };
    // execution models the Provider returning no usable compaction result.
    // execution 模拟 Provider 未返回可用压缩结果。
    let execution = PromptCompactionExecution {
        compaction_result: CompactionResult::from_model_summary(&plan, "回退摘要"),
        fallback_used: true,
        provider_request_count: 2,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };

    // outcome must retain the failed snapshot while denying automatic continuation.
    // outcome 必须保留失败快照，同时拒绝自动续接。
    let outcome = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &test_agent_runtime_bridge(),
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        "auto_budget",
        ContextCompactionCommitMode::ContinueActiveStream {
            task_id: active_stream_task_id,
        },
        WebuiContextCompactionExecution {
            execution,
            retained_from_seq: Seq::try_new(1).expect("valid retained sequence"),
            durability: ProviderV1FinishedDurability::default(),
        },
    )
    .await
    .expect("fallback failure record should commit");

    assert!(matches!(
        &outcome,
        ContextCompactionFinishOutcome::Rejected { .. }
    ));
    assert!(
        outcome
            .rejection_error()
            .is_some_and(|error| error.contains("this request was stopped"))
    );
    // snapshot proves the rejected attempt remains durable in the process-local runtime state.
    // snapshot 证明被拒绝尝试仍保留在进程内运行态中。
    let snapshot = outcome.into_workspace_snapshot();
    assert_eq!(
        snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Ready)
    );
    // runtime_state contains the explicit failed terminal record used by persistence and display.
    // runtime_state 包含供持久化与展示使用的显式失败终态记录。
    let runtime_state = snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("failed compaction should retain runtime state");
    assert!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .any(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_state"))
                    .and_then(Value::as_str)
                    == Some("failed")
            })
    );
}

/// Verifies a successful Provider result cannot bypass missing V1 durable facts through legacy storage.
/// 验证成功的 Provider 结果不能在缺少 V1 可靠事实时通过旧存储绕过提交门禁。
#[tokio::test]
async fn context_compaction_without_v1_durability_cannot_write_legacy_checkpoint() {
    // session_id identifies the session whose legacy projection must remain unchanged.
    // session_id 标识旧投影必须保持不变的会话。
    let session_id = SessionId::new_v4();
    // state retains the visible workspace while V1 durable receipts are deliberately absent.
    // state 在刻意缺少 V1 可靠回执时保留可见工作区。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // active_stream_task_id proves a pre-commit rejection does not consume the stream owner.
    // active_stream_task_id 证明提交前拒绝不会消耗流所有者。
    let active_stream_task_id = register_pending_context_compaction_test_stream(&state, session_id);
    // section_id identifies the visible compaction attempt without creating a second checkpoint.
    // section_id 标识可见压缩尝试，但不会创建第二份检查点。
    let section_id = {
        // guard initializes only display state required by the completion boundary.
        // guard 仅初始化完成边界需要的显示状态。
        let mut guard = state.lock().expect("lock missing V1 durability fixture");
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist");
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
        append_webui_context_compaction_status_message_to_workspace(
            workspace,
            session_id,
            "预算说明",
            MANUAL_CONTEXT_COMPACTION_TRIGGER,
        )
        .expect("compaction status section should be valid")
    };
    // plan and execution model a successful Provider response without durable V1 facts.
    // plan 与 execution 模拟缺少 V1 可靠事实的成功 Provider 响应。
    let policy = ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: String::from("missing-v1-durability"),
        created_at: String::from("1780000000100"),
        policy,
        tail_selection: context_compaction_test_tail_selection(policy, &[]),
    };
    let execution = PromptCompactionExecution {
        compaction_result: CompactionResult::from_model_summary(&plan, "## Goal\n- 保留长期目标。"),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: None,
        model_id: None,
        execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
        input_prune_report: None,
    };

    // error proves successful compaction cannot fall back to legacy runtime or VMM persistence.
    // error 证明成功压缩不能回退到旧 runtime 或 VMM 持久化。
    let error = match finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &test_agent_runtime_bridge(),
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishActiveStream {
            task_id: active_stream_task_id,
        },
        WebuiContextCompactionExecution {
            execution,
            retained_from_seq: Seq::try_new(1).expect("valid retained sequence"),
            durability: ProviderV1FinishedDurability::default(),
        },
    )
    .await
    {
        Ok(_) => panic!("missing V1 durability must reject before any projection mutation"),
        Err(error) => error,
    };
    // The first missing durable fact, the API terminal receipt, rejects before the semantic receipt check.
    // 首个缺失的可靠事实（API 终态回执）会在语义回执检查之前拒绝。
    assert!(error.contains("no durable API terminal receipt"), "{error}");

    // guard proves no legacy checkpoint was written and ownership remains available for retry.
    // guard 证明进程内所有权仍可用于重试。
    let guard = state
        .lock()
        .expect("lock post-rejection compaction fixture");
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace fixture should exist");
    assert!(
        workspace
            .session_runtime_state
            .get(&session_id)
            .is_none_or(|runtime_state| runtime_state.last_compaction_id.is_none())
    );
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        active_stream_task_id
    ));
}

/// Verifies a stale compaction task cannot mutate or close its replacement owner.
/// 验证过期压缩任务无法修改或关闭其替代所有者。
#[tokio::test]
async fn stale_context_compaction_owner_preserves_replacement_state() {
    // session_id identifies the regular session whose original compaction owner was replaced.
    // session_id 标识原压缩所有者已被替换的普通会话。
    let session_id = SessionId::new_v4();
    // state retains the replacement registry entry, active kernel, and untouched workspace phase.
    // state 保留替代注册条目、活动 kernel 与未修改的工作区阶段。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // section_id anchors the in-progress compaction section that the stale task must not fail.
    // section_id 锚定过期任务不得标记失败的进行中压缩分段。
    let section_id = {
        // guard installs the in-progress domain and kernel state before task registration.
        // guard 在任务注册前安装进行中的领域与 kernel 状态。
        let mut guard = state.lock().expect("lock stale-owner fixture");
        // workspace is the authoritative retained session projection.
        // workspace 是权威保留会话投影。
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist");
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
        // section_id is the committed visible boundary owned by the replacement turn.
        // section_id 是替代回合拥有的已提交可见边界。
        let section_id = append_webui_context_compaction_status_message_to_workspace(
            workspace,
            session_id,
            "预算说明",
            MANUAL_CONTEXT_COMPACTION_TRIGGER,
        )
        .expect("compaction status section should be valid");
        start_webui_session_kernel_turn(
            &mut guard,
            session_id,
            "replacement-compaction-turn",
            ActiveTurnKind::ContextCompaction,
        )
        .expect("replacement compaction kernel turn should start");
        section_id
    };
    // replacement_task_id is the only task identity authorized to commit the current section.
    // replacement_task_id 是唯一获准提交当前分段的任务身份。
    let replacement_task_id = register_pending_context_compaction_test_stream(&state, session_id);
    // stale_task_id models the terminal callback from the superseded original task.
    // stale_task_id 模拟被替代原任务发出的终态回调。
    let stale_task_id = Uuid::new_v4();

    // error proves stale ownership is rejected before any domain mutation occurs.
    // error 证明过期所有权会在任何领域修改前被拒绝。
    let error = fail_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishActiveStream {
            task_id: stale_task_id,
        },
        "stale compaction failure",
    )
    .expect_err("stale compaction owner must not commit failure state");
    assert!(error.contains("ownership was superseded"));

    // guard proves the replacement task, kernel, and preparing phase remain authoritative.
    // guard 证明替代任务、kernel 与 Preparing 阶段仍保持权威。
    let guard = state.lock().expect("lock stale-owner assertions");
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        replacement_task_id
    ));
    assert!(
        guard
            .session_runtime_kernels
            .get(&session_id)
            .is_some_and(SessionRuntimeKernel::has_active_turn)
    );
    // workspace retains the exact pre-attempt phase and contains no stale failure terminal record.
    // workspace 保留尝试前的精确阶段，且不包含过期失败终态记录。
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace fixture should remain available");
    assert_eq!(
        workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Preparing)
    );
    assert!(
        workspace
            .session_runtime_state
            .get(&session_id)
            .is_none_or(
                |runtime_state| runtime_state.conversation_state.records.iter().all(
                    |record| record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("context_compaction_state"))
                        .and_then(Value::as_str)
                        != Some("failed")
                )
            )
    );
}

/// Verifies owned registration failure commits a durable compaction rejection with kernel closure.
/// 验证拥有所有权的注册失败会提交可持久化压缩拒绝并关闭内核。
#[test]
fn context_compaction_registration_failure_commits_domain_terminal_state() {
    // session_id identifies the regular session whose compaction task failed to register.
    // session_id 标识压缩任务注册失败的普通会话。
    let session_id = SessionId::new_v4();
    // service_state owns the committed Running section and active compaction kernel turn.
    // service_state 持有已提交的 Running 分段与活动压缩内核回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // section_id anchors the formal registration failure record.
    // section_id 锚定正式注册失败记录。
    let section_id = append_webui_context_compaction_status_message_to_workspace(
        service_state
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist"),
        session_id,
        "预算说明",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
    )
    .expect("compaction status section should be valid");
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace fixture should exist")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    start_webui_session_kernel_turn(
        &mut service_state,
        session_id,
        "context-compaction-registration-test",
        ActiveTurnKind::ContextCompaction,
    )
    .expect("compaction kernel turn should start");
    // expected_generation binds domain failure commit to the exact compaction kernel turn.
    // expected_generation 将领域失败提交绑定到精确压缩内核回合。
    let expected_generation =
        webui_session_kernel_generation(&service_state, session_id).expect("kernel generation");
    // state exposes the retained ownership transaction to registration failure commit.
    // state 向注册失败提交提供保留所有权事务。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // registration_error proves start-gate cleanup removed this attempt's exact task entry.
    // registration_error 证明启动闸门清理移除了当前尝试的精确任务条目。
    let registration_error = ActiveStreamTaskRegistrationError::start_gate_closed(true);
    // failure_commit carries the snapshot produced inside the exact ownership transaction.
    // failure_commit 携带在精确所有权事务内产生的快照。
    let failure_commit = commit_stream_task_registration_failure(
        &state,
        session_id,
        Some(expected_generation),
        &registration_error,
        |service_state| {
            // workspace owns the Running section converted to a formal failure record.
            // workspace 持有被转换为正式失败记录的 Running 分段。
            let workspace = service_state
                .workspace_snapshot
                .as_mut()
                .expect("workspace fixture should remain available");
            apply_webui_context_compaction_failure_to_workspace(
                workspace,
                session_id,
                SessionRunPhase::Ready,
                &section_id,
                "预算说明",
                MANUAL_CONTEXT_COMPACTION_TRIGGER,
                "registered task start gate closed",
            )
            .expect("formal compaction failure should commit");
            Ok(())
        },
        |service_state, ()| {
            Ok(service_state
                .workspace_snapshot
                .as_ref()
                .expect("workspace fixture should remain available")
                .clone())
        },
    )
    .expect("owned registration failure should commit domain state");
    // snapshot is available only when exact ownership allowed the domain commit.
    // snapshot 仅在精确所有权允许领域提交时可用。
    let snapshot = match failure_commit {
        StreamTaskLaunchFailureCommit::Committed(snapshot) => snapshot,
        StreamTaskLaunchFailureCommit::Skipped(outcome) => {
            panic!("owned registration failure was unexpectedly skipped: {outcome:?}")
        }
    };

    assert_eq!(
        snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Ready)
    );
    // runtime_state exposes the durable failed terminal record.
    // runtime_state 提供可持久化的失败终态记录。
    let runtime_state = snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("failed compaction should retain runtime state");
    assert!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .any(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_state"))
                    .and_then(Value::as_str)
                    == Some("failed")
            })
    );
    // guard proves the exact kernel turn closed in the same retained transaction.
    // guard 证明精确内核回合在同一保留事务内关闭。
    let guard = state.lock().expect("service state lock");
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies TaskSession terminal reports project provider usage into live panel turn stats.
/// 验证智能体终态报告会把 Provider 用量投影到实时面板轮次统计。
#[test]
fn task_session_execution_report_fast_path_projects_live_usage_stats() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let provider_usage = ProviderUsageSnapshot {
        input_tokens: Some(31_295),
        uncached_input_tokens: None,
        output_tokens: Some(16),
        reasoning_tokens: Some(14),
        total_tokens: Some(31_311),
        cached_input_tokens: Some(5_888),
        cache_creation_input_tokens: None,
        ..ProviderUsageSnapshot::default()
    };
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("abcd")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: Some(provider_usage.clone()),
        cumulative_successful_provider_usage: Some(provider_usage),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        provider_failure_category: None,
        controller_events: Vec::new(),
    };

    let _applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should apply live usage stats");
    let guard = state.lock().expect("lock applied TaskSession state");
    let records = guard
        .workspace_snapshot
        .as_ref()
        .expect("applied workspace should remain retained")
        .session_runtime_state
        .get(&task_session_id)
        .expect("task session report should retain process-local runtime state")
        .conversation_state
        .records
        .as_slice();
    let stats_row = records
        .iter()
        .rev()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("task session should show one turn stats row");
    let stats: SessionTurnStats = serde_json::from_value(
        stats_row
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("turn_stats"))
            .expect("turn stats metadata should exist")
            .clone(),
    )
    .expect("turn stats metadata should parse");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 31_295);
    assert_eq!(stats.output_tokens, 16);
    assert_eq!(stats.reasoning_tokens, 14);
    assert_eq!(stats.total_tokens, 31_311);
    assert_eq!(stats.cached_input_tokens, Some(5_888));
    assert_eq!(stats.usage_segments.len(), 1);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
}
/// Verifies render cache alone does not create subagent navigation membership.
/// 验证仅有渲染缓存不会创建子代理导航成员关系。
#[test]
fn delegate_started_cache_without_registry_does_not_create_navigation_membership() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("slow-worker"),
        parent_session_id,
        subagent_label: String::from("慢速子代理"),
        description: Some(String::from("等待 30 秒")),
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id, BTreeSet::new());
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();

    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot);

    assert!(
        navigation.is_none(),
        "render-only cache must not create subagent navigation membership without a registry entry"
    );
}

/// Verifies a live detached listener keeps the parent subagent navigation visible after the parent stream finishes.
/// 验证父级流结束后，实时分离监听器仍会让父级子代理导航保持可见。
#[test]
fn webui_detached_listener_keeps_running_subagent_navigation_visible() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("image-worker"),
        parent_session_id,
        subagent_label: String::from("图片子代理"),
        description: Some(String::from("生成图片")),
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id, BTreeSet::new());
    guard.detached_subagent_task_listeners.insert(
        task_session_id,
        String::from("navigation-visible-execution"),
    );
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();

    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("running subagent navigation should stay visible");

    assert_eq!(
        navigation.parent_conversation_id,
        parent_session_id.to_string()
    );
    assert_eq!(
        navigation.active_conversation_id,
        parent_session_id.to_string()
    );
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    assert!(navigation.tasks[0].is_running);
    assert!(session_has_running_subagent_tasks_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id
    ));
}

/// Verifies a terminal listener can clear only the execution generation it owns.
/// 验证终态监听器只能清理其拥有的执行代际。
#[test]
fn detached_listener_cleanup_preserves_replacement_execution() {
    // parent_session_id owns the child task surface used by the test state.
    // parent_session_id 持有测试状态使用的子任务界面。
    let parent_session_id = SessionId::from_u128(0x1180);
    // task_id is reused by old and replacement execution attempts.
    // task_id 被旧执行与替代执行尝试复用。
    let task_id = SessionId::from_u128(0x1181);
    // state stores the replacement execution as the current listener owner.
    // state 将替代执行存储为当前监听器所有者。
    let mut state = test_service_state_with_parent_session(parent_session_id);
    state
        .detached_subagent_task_listeners
        .insert(task_id, String::from("execution-new"));

    let stale_cleared =
        clear_webui_detached_listener_execution_if_current(&mut state, task_id, "execution-old");

    assert!(!stale_cleared);
    assert_eq!(
        state.detached_subagent_task_listeners.get(&task_id),
        Some(&String::from("execution-new"))
    );
    let current_cleared =
        clear_webui_detached_listener_execution_if_current(&mut state, task_id, "execution-new");
    assert!(current_cleared);
    assert!(
        !state
            .detached_subagent_task_listeners
            .contains_key(&task_id)
    );
}

/// Verifies an old listener terminal event cannot overwrite the replacement execution projection.
/// 验证旧监听器终态事件不能覆盖替代执行投影。
#[test]
fn stale_detached_terminal_event_skips_replacement_projection() {
    // parent_session_id owns the stable child task across both executions.
    // parent_session_id 跨两次执行持有稳定子任务。
    let parent_session_id = SessionId::from_u128(0x1182);
    // task_id is reused by the stale and replacement executions.
    // task_id 被过期执行与替代执行复用。
    let task_id = SessionId::from_u128(0x1183);
    // state declares the replacement execution as the only current listener owner.
    // state 将替代执行声明为唯一当前监听器所有者。
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .detached_subagent_task_listeners
        .insert(task_id, String::from("execution-new"));
    // state exposes the shared projection state consumed by the ingest function.
    // state 暴露 ingest 函数使用的共享投影状态。
    let state = Arc::new(Mutex::new(service_state));
    // app collects no external sinks while exercising projection ownership.
    // app 在执行投影所有权测试时不收集外部接收器。
    let app = ServiceEventEmitter::new(Vec::new());
    // runtime supplies the existing ingest signature without external provider work.
    // runtime 提供现有 ingest 签名且不触发外部 Provider 工作。
    let runtime = test_agent_runtime_bridge();
    // event is the stale execution's terminal delegate result.
    // event 是过期执行的终态委派结果。
    let event = test_delegate_finished_event(task_id, "repeat-agent", "old result");

    let consumed = ingest_webui_subagent_runtime_event_for_listener_execution(
        &app,
        &state,
        &runtime,
        parent_session_id,
        &event,
        Some("execution-old"),
    )
    .expect("stale listener event should be safely consumed");

    assert!(consumed);
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .detached_subagent_task_listeners
            .get(&task_id)
            .map(String::as_str),
        Some("execution-new")
    );
    assert!(!guard.subagent_surface_detail_cache.contains_key(&task_id));
}

/// Verifies subagent ingest leaves listener and navigation state untouched when workspace preflight fails.
/// 验证工作区预检失败时，子代理摄取不会修改监听器与导航状态。
#[test]
fn subagent_ingest_workspace_preflight_failure_has_no_partial_state_commit() {
    // parentSessionId owns the child execution whose projection is being attempted.
    // parentSessionId 持有本次尝试投影的子执行。
    let parent_session_id = SessionId::from_u128(0x1184);
    // taskId identifies the exact detached listener registered before event ingest.
    // taskId 标识事件摄取前已经注册的精确分离监听器。
    let task_id = SessionId::from_u128(0x1185);
    // executionId is the immutable execution owner expected by the listener path.
    // executionId 是监听器路径要求的不可变执行所有者。
    let execution_id = String::from("execution-current");
    // serviceState deliberately lacks a workspace so preflight fails before any shared mutation.
    // serviceState 刻意移除工作区，使预检在任何共享变更前失败。
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state.workspace_snapshot = None;
    service_state
        .detached_subagent_task_listeners
        .insert(task_id, execution_id.clone());
    // state exposes the shared service transaction inspected after the failed ingest.
    // state 暴露失败摄取后需要检查的共享服务事务状态。
    let state = Arc::new(Mutex::new(service_state));
    // app provides the required event sink without external side effects.
    // app 提供所需事件接收器且不产生外部副作用。
    let app = ServiceEventEmitter::new(Vec::new());
    // runtime satisfies the ingest dependency without starting provider work.
    // runtime 满足摄取依赖且不会启动 Provider 工作。
    let runtime = test_agent_runtime_bridge();
    // event is a valid terminal event owned by the current listener execution.
    // event 是当前监听器执行持有的合法终态事件。
    let event = test_delegate_finished_event(task_id, "atomic-agent", "done");

    // error proves the workspace dependency was rejected explicitly.
    // error 证明缺失工作区依赖被显式拒绝。
    let error = ingest_webui_subagent_runtime_event_for_listener_execution(
        &app,
        &state,
        &runtime,
        parent_session_id,
        &event,
        Some(execution_id.as_str()),
    )
    .expect_err("missing workspace should reject subagent ingest");

    assert!(error.contains("workspace snapshot is not ready"));
    // guard verifies every state surface that the former first lock could partially mutate.
    // guard 验证旧版第一次加锁可能局部修改的全部状态表面。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .detached_subagent_task_listeners
            .get(&task_id)
            .map(String::as_str),
        Some(execution_id.as_str())
    );
    assert!(
        !guard
            .session_subagent_navigation_rounds
            .contains_key(&parent_session_id)
    );
    assert!(!guard.subagent_surface_detail_cache.contains_key(&task_id));
}

/// Verifies terminal listener cleanup immediately emits a non-running subagent navigation delta.
/// 验证终态监听器清理后会立即发出非运行中的子代理导航增量。
#[test]
fn terminal_listener_cleanup_emits_non_running_subagent_navigation_delta() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    {
        let mut guard = state
            .lock()
            .expect("service state lock should be available for terminal cleanup setup");
        let task_states = vec![SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("image-worker"),
            parent_session_id,
            subagent_label: String::from("图片子代理"),
            description: Some(String::from("生成图片")),
            run_state: SubagentTaskRunState::Completed,
            result_text: String::from("图片生成完成"),
            conversation_state: Some(SessionConversationState {
                messages: vec![SessionConversationMessage {
                    id: String::from("child-assistant-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("图片生成完成"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                }],
                ..SessionConversationState::default()
            }),
        }];
        cache_test_subagent_surface_detail_states(&mut guard, &task_states);
        seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            parent_session_id,
            &task_states,
        );
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id, BTreeSet::new());
        guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
            parent_session_id,
            task_id: task_session_id,
        });
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id, String::from("terminal-cleanup-execution"));
    }
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    {
        let mut guard = state
            .lock()
            .expect("service state lock should be available for listener cleanup");
        guard
            .detached_subagent_task_listeners
            .remove(&task_session_id);
    }
    let navigation =
        emit_webui_subagent_navigation_changed_delta_for_parent(&app, &state, parent_session_id)
            .expect("terminal cleanup should emit navigation")
            .expect("active subagent panel should keep navigation mounted");

    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    assert_eq!(navigation.tasks[0].run_state, "completed");
    assert!(!navigation.tasks[0].is_running);
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(
        event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(
                &envelope.delta,
                WorkbenchDelta::SubagentNavigationChanged(delta)
                    if delta
                        .subagent_navigation
                        .as_ref()
                        .and_then(|navigation| navigation.tasks.first())
                        .is_some_and(|task| !task.is_running && task.run_state == "completed")
            )
    )));
}

/// Verifies empty subagent navigation deltas still identify the parent conversation to clear stale UI cache.
/// 验证空子智能体导航增量仍携带父会话标识，以清理前端旧缓存。
#[test]
fn empty_subagent_navigation_delta_preserves_parent_conversation_id() {
    let parent_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    let navigation =
        emit_webui_subagent_navigation_changed_delta_for_parent(&app, &state, parent_session_id)
            .expect("empty navigation should emit");

    assert!(navigation.is_none());
    let expected_parent_conversation_id = parent_session_id.to_string();
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(
        event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(
                &envelope.delta,
                WorkbenchDelta::SubagentNavigationChanged(delta)
                    if delta.parent_conversation_id.as_deref()
                        == Some(expected_parent_conversation_id.as_str())
                        && delta.subagent_navigation.is_none()
            )
    )));
}

/// Verifies background empty subagent navigation deltas keep their requested parent id.
/// 验证后台空子智能体导航增量会保留被请求的父会话标识。
#[test]
fn background_empty_subagent_navigation_delta_preserves_requested_parent_conversation_id() {
    let selected_session_id = SessionId::new_v4();
    let background_parent_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        selected_session_id,
    )));
    {
        let mut guard = state.lock().expect("service state lock");
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace snapshot");
        workspace.sessions.push(SessionSummary::new(
            background_parent_session_id,
            Some(String::from("project-a")),
            "后台父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        ));
        workspace
            .session_run_phases
            .insert(background_parent_session_id, SessionRunPhase::Ready);
    }
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    let navigation = emit_webui_subagent_navigation_changed_delta_for_parent(
        &app,
        &state,
        background_parent_session_id,
    )
    .expect("background empty navigation should emit");

    assert!(navigation.is_none());
    let expected_parent_conversation_id = background_parent_session_id.to_string();
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(
        event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(
                &envelope.delta,
                WorkbenchDelta::SubagentNavigationChanged(delta)
                    if delta.parent_conversation_id.as_deref()
                        == Some(expected_parent_conversation_id.as_str())
                        && delta.subagent_navigation.is_none()
            )
    )));
}
/// Verifies two detached WebUI subagents stop blocking the parent turn after both reach terminal states.
/// 验证两个 WebUI 分离子代理全部进入终态后不再阻塞父级回合收尾。
#[test]
fn webui_completion_gate_clears_after_two_detached_subagents_finish() {
    let parent_session_id = SessionId::new_v4();
    let first_task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let mut task_states = vec![
        SubagentTaskPanelState {
            task_id: first_task_session_id,
            public_task_id: String::from("image-worker-a"),
            parent_session_id,
            subagent_label: String::from("图片子代理 A"),
            description: Some(String::from("生成第一张图片")),
            run_state: SubagentTaskRunState::Running,
            result_text: String::new(),
            conversation_state: None,
        },
        SubagentTaskPanelState {
            task_id: second_task_session_id,
            public_task_id: String::from("image-worker-b"),
            parent_session_id,
            subagent_label: String::from("图片子代理 B"),
            description: Some(String::from("生成第二张图片")),
            run_state: SubagentTaskRunState::Running,
            result_text: String::new(),
            conversation_state: None,
        },
    ];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id, BTreeSet::new());
    guard.detached_subagent_task_listeners.extend([
        (
            first_task_session_id,
            String::from("first-completion-execution"),
        ),
        (
            second_task_session_id,
            String::from("second-completion-execution"),
        ),
    ]);
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();

    let running_tasks = running_webui_subagent_task_states_for_tree_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
    );
    assert_eq!(running_tasks.len(), 2);
    let running_gate = WebuiTurnCompletionGate {
        pending_subagent_count: running_tasks.len(),
        has_active_hook_turn: false,
    };
    assert!(matches!(
        running_gate.completion_status(),
        ConversationTurnCompletionStatus::AwaitingSubagents
    ));

    for task_state in &mut task_states {
        task_state.run_state = SubagentTaskRunState::Completed;
        task_state.result_text = format!("任务 {} 已完成", task_state.task_id);
        guard
            .detached_subagent_task_listeners
            .remove(&task_state.task_id);
    }
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);

    let finished_tasks = running_webui_subagent_task_states_for_tree_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
    );
    assert!(finished_tasks.is_empty());
    assert!(!session_has_running_subagent_tasks_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id
    ));
    let finished_gate = WebuiTurnCompletionGate {
        pending_subagent_count: finished_tasks.len(),
        has_active_hook_turn: false,
    };
    assert!(matches!(
        finished_gate.completion_status(),
        ConversationTurnCompletionStatus::Completed
    ));
}

/// Verifies delegate-finished events force stale running payloads into a terminal panel state.
/// 验证 delegate-finished 事件会将陈旧 running 载荷强制归一为终态面板状态。
#[test]
fn delegate_finished_ingest_terminalizes_stale_running_payload() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    {
        let mut guard = state
            .lock()
            .expect("service state lock should be available for stale payload setup");
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id, String::from("stale-running-execution"));
    }
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "子代理完成",
        "子代理任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "stale-running-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-stale-running-worker",
            "execution_status": "running",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "已完成但状态载荷陈旧",
        }
    })));

    let consumed =
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest");

    assert!(consumed);
    let guard = state
        .lock()
        .expect("service state lock should be available after stale payload ingest");
    assert!(
        !guard
            .detached_subagent_task_listeners
            .contains_key(&task_session_id)
    );
}

/// Verifies close-agent cleanup removes closed task surfaces while preserving other agents and admitted terminals.
/// 验证关闭代理清理会移除已关闭任务界面，同时保留其他代理及已准入终态。
#[test]
fn close_agent_cleanup_unloads_runtime_state_and_retains_current_navigation_round() {
    let parent_session_id = SessionId::new_v4();
    let closed_task_id = SessionId::new_v4();
    let retained_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let closed_state = test_completed_subagent_panel_state(parent_session_id, closed_task_id);
    let retained_state = test_completed_subagent_panel_state(parent_session_id, retained_task_id);
    cache_test_subagent_surface_detail_states(&mut guard, &[closed_state, retained_state.clone()]);
    seed_test_live_subagent_registry_from_panel_states(
        &mut guard,
        parent_session_id,
        &[retained_state],
    );
    guard
        .detached_subagent_task_listeners
        .insert(closed_task_id, String::from("closed-task-execution"));
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: closed_task_id,
    });
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id, BTreeSet::from([closed_task_id]));

    assert!(unload_webui_closed_subagent_runtime_state(
        &mut guard,
        &[closed_task_id]
    ));

    assert!(
        !guard
            .subagent_surface_detail_cache
            .contains_key(&closed_task_id)
    );
    assert!(
        guard
            .subagent_surface_detail_cache
            .contains_key(&retained_task_id)
    );
    // Closing a runtime slot cannot revoke ownership of an already admitted child terminal.
    // 关闭运行时槽位不能撤销已准入子终态的归属。
    assert!(
        guard
            .detached_subagent_task_listeners
            .contains_key(&closed_task_id)
    );
    assert!(guard.active_subagent_panel.is_none());
    assert!(
        guard
            .session_subagent_navigation_rounds
            .contains_key(&parent_session_id)
    );
}

/// Verifies a close tool event immediately publishes closed status for a child carried across turns.
/// 验证关闭工具事件会立即为跨回合保留的子任务发布已关闭状态。
/// Parameters: none. Returns: none; asserts archive authority beats stale completed detail.
/// 参数：无。返回值：无；断言归档权威优先于陈旧的已完成详情。
#[test]
fn close_agent_event_emits_closed_subagent_navigation_for_carried_child() {
    // parent_id owns the child that completed earlier but remained open into this turn.
    // parent_id 拥有此前已完成、但保持开放进入当前回合的子任务。
    let parent_id = SessionId::new_v4();
    // child_id is the exact target archived by the close tool result.
    // child_id 是关闭工具结果归档的精确目标。
    let child_id = SessionId::new_v4();
    // completed intentionally leaves stale completed detail until production close cleanup removes it.
    // completed 刻意保留陈旧已完成详情，直到生产关闭清理将其移除。
    let mut completed = test_completed_subagent_panel_state(parent_id, child_id);
    // guard contains the live registry and lightweight navigation membership.
    // guard 包含实时注册表和轻量导航成员关系。
    let mut guard = test_service_state_with_parent_session(parent_id);
    cache_test_subagent_surface_detail_states(&mut guard, std::slice::from_ref(&completed));
    seed_test_live_subagent_registry_from_panel_states(
        &mut guard,
        parent_id,
        std::slice::from_ref(&completed),
    );
    assert!(
        subagent_navigation_snapshot_for_parent_from_cached_state(&mut guard, parent_id, true)
            .expect("initial navigation")
            .is_some()
    );
    guard.session_subagent_navigation_rounds.remove(&parent_id);
    assert!(
        subagent_navigation_snapshot_for_parent_from_cached_state(&mut guard, parent_id, true)
            .expect("unclosed child carries into next turn")
            .is_some()
    );
    completed.run_state = SubagentTaskRunState::Closed;
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_id, &[completed]);
    // state exposes the committed registry archive to the same projection path as real ToolFinished events.
    // state 将已提交的注册表归档暴露给真实 ToolFinished 事件使用的投影路径。
    let state = Arc::new(Mutex::new(guard));
    // emitted_events and app capture the delta without requiring a user click or refresh.
    // emitted_events 和 app 捕获增量，不要求用户点击或刷新。
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);
    // event uses the canonical close-agent host result fields consumed by production projection.
    // event 使用生产投影消费的规范关闭智能体宿主结果字段。
    let event = RuntimeEvent::new(RuntimeEventKind::ToolFinished, "close_agent", "closed")
        .with_metadata(Some(json!({
            "tool_name": "close_agent",
            "host_result": {
                "kind": "subagent_control",
                "payload": {
                    "tool": "close_agent",
                    "matched_agents": [{ (SUBAGENT_TASK_ID_KEY): child_id.to_string() }],
                    "stopped_subagent_task_ids": []
                }
            }
        })));
    append_runtime_event_projection(&app, &state, parent_id, &event)
        .expect("close event projection");
    // events must include the closed row even though the last execution phase was completed.
    // events 必须包含已关闭行，即使最近执行阶段为已完成。
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(&envelope.delta, WorkbenchDelta::SubagentNavigationChanged(delta)
                if delta.subagent_navigation.as_ref().is_some_and(|navigation|
                    navigation.tasks.len() == 1 && navigation.tasks[0].run_state == "closed"
                        && !navigation.tasks[0].is_running))
    )));
    // guard confirms only lightweight navigation survives; the child detail is unloaded.
    // guard 确认仅轻量导航保留，子任务详情已卸载。
    let guard = state.lock().expect("service state lock");
    assert!(!guard.subagent_surface_detail_cache.contains_key(&child_id));
    assert!(
        guard
            .session_subagent_navigation_rounds
            .get(&parent_id)
            .expect("current round")
            .contains(&child_id)
    );
}

/// Verifies the WebUI close path cannot reintroduce the removed cascade argument.
/// 验证 WebUI 关闭路径不会重新引入已移除的 cascade 参数。
#[test]
fn close_agent_request_uses_target_only_runtime_contract() {
    // arguments is the exact payload passed from the service boundary to TaskRuntime.
    // arguments 是服务边界传递给 TaskRuntime 的精确载荷。
    let arguments = close_agent_request_arguments("analysis-worker");

    assert_eq!(arguments, json!({ "target": "analysis-worker" }));
}

/// Builds one completed subagent panel state for cache mutation tests.
/// 为缓存变更测试构建一条已完成子代理面板状态。
fn test_completed_subagent_panel_state(
    parent_session_id: SessionId,
    task_id: SessionId,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id,
        public_task_id: task_id.to_string(),
        parent_session_id,
        subagent_label: String::from("Review Worker"),
        description: Some(String::from("审核代码")),
        run_state: SubagentTaskRunState::Completed,
        result_text: String::from("审核完成"),
        conversation_state: None,
    }
}

/// Verifies child completion feedback does not close the parent completed-turn hook.
/// 验证智能体完成反馈不会闭合父级完成回合 hook。
#[test]
fn subagent_completion_feedback_keeps_parent_hook_turn_open() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    {
        let mut guard = state
            .lock()
            .expect("service state lock should be available for awaiting hook setup");
        let task_states = vec![SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("review-worker"),
            parent_session_id,
            subagent_label: String::from("Review Worker"),
            description: Some(String::from("审核代码")),
            run_state: SubagentTaskRunState::Completed,
            result_text: String::from("审核完成"),
            conversation_state: None,
        }];
        cache_test_subagent_surface_detail_states(&mut guard, &task_states);
        seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            parent_session_id,
            &task_states,
        );
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace should exist");
        let runtime_state = workspace
            .session_runtime_state
            .entry(parent_session_id)
            .or_default();
        runtime_state
            .conversation_state
            .messages
            .push(SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("父代理已等待子代理结果。"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            });
    }
    {
        let mut hook_runtime = runtime
            .conversation_hook_runtime
            .lock()
            .expect("conversation hook runtime lock should be available for setup");
        hook_runtime.record_queued_user_prompt(
            parent_session_id,
            "prompt-1",
            Some(String::from("project-a")),
            String::from("D:\\openvulcan\\vulcan-code"),
            PendingSessionPromptSource::UserFollowup,
            "请调用子代理审核代码",
        );
        assert!(hook_runtime.has_active_turn(parent_session_id));
    }
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "review-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-review-worker",
            "execution_status": "completed",
            "description": "审核代码",
            "subagent": "review-worker",
            "result": "审核完成"
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );
    assert!(
        runtime
            .conversation_hook_runtime
            .lock()
            .expect("conversation hook runtime lock should be available after feedback")
            .has_active_turn(parent_session_id)
    );
}
