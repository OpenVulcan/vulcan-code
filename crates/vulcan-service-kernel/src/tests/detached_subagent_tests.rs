//! detached subagent tests.
//! detached subagent tests。

use super::*;
use crate::context_compaction::{
    ContextCompactionCommitMode, ContextCompactionFinishOutcome,
    apply_webui_context_compaction_failure_to_workspace, fail_webui_context_compaction_execution,
    finish_webui_context_compaction_execution,
};
use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
    PromptCompactionExecution, PromptCompactionExecutionProfile,
};
use vulcan_agent_runtime::conversation_protocol::conversation_state_from_session_messages;
use vulcan_context::ContextCompactionPolicy;
use vulcan_context_compaction::{CompactionPlan, CompactionResult};
use vulcan_workbench_model::SessionTurnUsageGroupKind;

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
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
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
        token_budget_runtime: None,
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
        session_runtime_kernels: BTreeMap::new(),
        session_turn_admission_reservations: BTreeMap::new(),
        project_admission_reservations: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        vmm_controller_registry: Default::default(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
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
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should apply to memory");

    assert!(applied.completion_request.is_none());
    assert!(!workspace_snapshot_contains_regular_session(
        &applied.workspace_snapshot,
        task_session_id
    ));
    assert_eq!(
        applied
            .workspace_snapshot
            .session_run_phases
            .get(&task_session_id),
        Some(&SessionRunPhase::Ready)
    );
    let messages = applied
        .workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("task session report should materialize messages");
    assert!(
        messages
            .iter()
            .any(|message| message.body.contains("子代理完成"))
    );
    let guard = state
        .lock()
        .expect("service state lock should be available after report apply");
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
    // state starts with one running session so partial terminal mutation is observable.
    // state 以一个运行中会话开始，使部分终态变更可被观察。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // controller_root keeps this test's isolated VMM files alive through every assertion.
    // controller_root 在全部断言期间保持本测试隔离 VMM 文件有效。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    {
        // guard initializes the exact pre-commit workspace facts under one service-state lock.
        // guard 在同一服务状态锁内初始化精确的提交前工作区事实。
        let mut guard = state.lock().expect("lock regular session fixture");
        guard.controller_runtime = Some(controller_root.controller_runtime_handle());
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

/// Verifies a VMM/Hook turn mismatch is rejected before either terminal state machine consumes ownership.
/// 验证 VMM 与 Hook 回合身份不匹配时，会在任一终态状态机消费所有权之前被拒绝。
#[test]
fn regular_execution_report_vmm_turn_mismatch_preserves_both_owners() {
    // session_id identifies the regular session whose exact terminal ownership is under test.
    // session_id 标识正在测试精确终态所有权的普通会话。
    let session_id = SessionId::new_v4();
    // state contains one durable VMM lease registered to a deliberately divergent turn id.
    // state 包含一条持久 VMM 租约，并将其登记到刻意分歧的回合标识。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // controller_root keeps this test's isolated VMM files alive through every assertion.
    // controller_root 在全部断言期间保持本测试隔离 VMM 文件有效。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    state
        .lock()
        .expect("lock VMM mismatch controller fixture")
        .controller_runtime = Some(controller_root.controller_runtime_handle());
    // controller_context supplies the process registry and its exact storage root without retaining the service lock.
    // controller_context 在不保留服务锁的情况下提供进程注册表及其精确存储根。
    let (vmm_controller_registry, vmm_data_root) = {
        let guard = state.lock().expect("lock VMM mismatch fixture");
        let controller_runtime = guard
            .controller_runtime
            .as_ref()
            .expect("test controller runtime should exist");
        (
            guard.vmm_controller_registry.clone(),
            controller_runtime.data_root.clone(),
        )
    };
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
        .seed_for_test(&vmm_data_root, session_id, seeded_vmm_state)
        .expect("seed VMM mismatch controller");
    // vmm_controller is the exact registry instance later used by terminal completion.
    // vmm_controller 是终态完成阶段随后使用的同一个注册表实例。
    let vmm_controller = vmm_controller_registry
        .get_or_open(&vmm_data_root, session_id, "3")
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
    // report is a successful user turn that would consume TTL if both identities matched.
    // report 是一个成功用户回合，若两个身份匹配本应消费 TTL。
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
        controller_events: Vec::new(),
    };
    // error proves the cross-state identity fence rejected the terminal commit.
    // error 证明跨状态身份栅栏拒绝了终态提交。
    let error = match apply_execution_report_to_memory(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
        report,
    ) {
        Ok(_) => panic!("VMM/Hook turn mismatch must reject terminal commit"),
        Err(error) => error,
    };
    assert!(error.contains("does not match the retained conversation Hook turn"));
    assert_eq!(
        agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("lock Hook runtime after mismatch")
            .active_turn_id(session_id),
        Some(retained_hook_turn_id.as_str())
    );
    // persisted_vmm_state confirms mismatch rejection did not consume controller ownership.
    // persisted_vmm_state 确认身份不匹配拒绝没有消费控制器所有权。
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
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    state
        .lock()
        .expect("lock compaction fallback controller fixture")
        .controller_runtime = Some(controller_root.controller_runtime_handle());
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
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: String::from("fallback-compaction"),
        created_at: String::from("1780000000100"),
        policy: ContextCompactionPolicy::default(),
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
        execution,
    )
    .expect("fallback failure record should commit");

    assert!(matches!(
        &outcome,
        ContextCompactionFinishOutcome::Rejected { .. }
    ));
    assert!(
        outcome
            .rejection_error()
            .is_some_and(|error| error.contains("已停止本次请求"))
    );
    // snapshot proves the rejected attempt remains durable without a compaction checkpoint.
    // snapshot 证明被拒绝尝试可持久化，且不会产生压缩检查点。
    let snapshot = outcome.into_workspace_snapshot();
    assert!(
        snapshot
            .session_compactions
            .get(&session_id)
            .is_none_or(Vec::is_empty)
    );
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

/// Verifies a poisoned notification runtime cannot reclassify an already committed compaction.
/// 验证通知运行时中毒不能重新分类已经提交的上下文压缩。
#[tokio::test]
async fn context_compaction_hook_lock_failure_preserves_committed_outcome() {
    // session_id identifies the regular session receiving the successful compaction.
    // session_id 标识接收成功压缩的普通会话。
    let session_id = SessionId::new_v4();
    // messages provide one compacted source turn and one retained tail response.
    // messages 提供一个被压缩的源回合和一个保留的尾部回复。
    let messages = vec![
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "请保留目标")
            .with_runtime_metadata("user-1", "1780000000000"),
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "目标已确认")
            .with_runtime_metadata("assistant-1", "1780000000001"),
    ];
    // state retains the exact workspace mutated by the completion boundary.
    // state 保留由完成边界修改的精确工作区。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // controller_root keeps this test's isolated VMM files alive through every assertion.
    // controller_root 在全部断言期间保持本测试隔离 VMM 文件有效。
    let controller_root = TestControllerRuntimeRoot::new(session_id);
    // active_stream_task_id proves the successful commit belongs to the current parent stream.
    // active_stream_task_id 证明成功提交属于当前父流。
    let active_stream_task_id = register_pending_context_compaction_test_stream(&state, session_id);
    // section_id anchors the successful summary and terminal boundary records.
    // section_id 锚定成功摘要与终态边界记录。
    let section_id = {
        // guard initializes canonical conversation and status state under one lock.
        // guard 在同一把锁内初始化规范会话与状态记录。
        let mut guard = state.lock().expect("lock compaction success fixture");
        guard.controller_runtime = Some(controller_root.controller_runtime_handle());
        // workspace is the authoritative regular-session snapshot for this fixture.
        // workspace 是当前夹具的权威普通会话快照。
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace fixture should exist");
        workspace
            .session_messages
            .insert(session_id, messages.clone());
        workspace.session_runtime_state.insert(
            session_id,
            SessionRuntimeStateEntry {
                conversation_state: conversation_state_from_session_messages(&messages)
                    .expect("fixture messages should have stable runtime identities"),
                ..SessionRuntimeStateEntry::default()
            },
        );
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
        // section_id identifies the committed visible compaction section.
        // section_id 标识已提交的可见压缩分段。
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
            "manual-compaction-turn",
            ActiveTurnKind::ContextCompaction,
        )
        .expect("manual compaction kernel turn should start");
        section_id
    };
    // plan identifies the exact source boundary present in canonical conversation state.
    // plan 标识规范会话状态中真实存在的源边界。
    let plan = CompactionPlan {
        source_messages: vec![messages[0].clone()],
        source_until_message_id: Some(String::from("user-1")),
        source_until_turn_index: Some(0),
        retained_tail_start_turn_index: Some(1),
        compaction_id: String::from("committed-compaction"),
        created_at: String::from("1780000000100"),
        policy: ContextCompactionPolicy::default(),
    };
    // execution contains the usable Provider summary that must be committed.
    // execution 包含必须提交的可用 Provider 摘要。
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
    // retry_execution reuses the exact compaction identity to verify end-to-end idempotency.
    // retry_execution 复用精确压缩身份，以验证端到端幂等性。
    let retry_execution = execution.clone();
    // repair_execution drives recovery from a VMM-only committed interruption point.
    // repair_execution 驱动从仅 VMM 已提交的中断点恢复。
    let repair_execution = execution.clone();
    // precommit_workspace preserves the exact ordinary-state baseline before the first domain commit.
    // precommit_workspace 保留首次领域提交前的精确普通状态基线。
    let precommit_workspace = state
        .lock()
        .expect("lock precommit compaction workspace")
        .workspace_snapshot
        .clone()
        .expect("precommit workspace should exist");
    // agent_runtime owns the notification mutex deliberately poisoned below.
    // agent_runtime 持有下方刻意中毒的通知互斥锁。
    let agent_runtime = test_agent_runtime_bridge();
    // poisoned_hook_runtime shares the exact mutex used by compacted-hook dispatch.
    // poisoned_hook_runtime 共享压缩 Hook 分发使用的精确互斥锁。
    let poisoned_hook_runtime = Arc::clone(&agent_runtime.conversation_hook_runtime);
    // poison_result proves notification dispatch will encounter a poisoned mutex.
    // poison_result 证明通知分发将遇到中毒互斥锁。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // _guard keeps the mutex held until the deliberate panic marks it poisoned.
        // _guard 将互斥锁保持到刻意 panic，使其被标记为中毒。
        let _guard = poisoned_hook_runtime
            .lock()
            .expect("hook runtime should lock before deliberate panic");
        panic!("poison conversation hook runtime for compaction notification test");
    }));
    assert!(poison_result.is_err());

    // outcome must remain committed because the Hook is a post-commit notification only.
    // outcome 必须保持已提交，因为该 Hook 只是提交后通知。
    let outcome = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishActiveStream {
            task_id: active_stream_task_id,
        },
        execution,
    )
    .expect("notification failure must not reject committed compaction");

    assert!(matches!(
        &outcome,
        ContextCompactionFinishOutcome::Committed { .. }
    ));
    assert!(outcome.rejection_error().is_none());
    // snapshot proves the authoritative checkpoint was committed exactly once.
    // snapshot 证明权威检查点仅提交一次。
    let snapshot = outcome.into_workspace_snapshot();
    assert_eq!(
        snapshot.session_compactions.get(&session_id).map(Vec::len),
        Some(1)
    );
    assert_eq!(
        snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Ready)
    );
    // terminal_guard proves domain commit, task removal, and kernel closure reached the retained state together.
    // terminal_guard 证明领域提交、任务移除与 kernel 关闭共同到达保留状态。
    let terminal_guard = state.lock().expect("lock committed compaction state");
    assert!(!active_stream_task_matches_id(
        &terminal_guard,
        session_id,
        active_stream_task_id
    ));
    assert!(
        !terminal_guard
            .session_runtime_kernels
            .get(&session_id)
            .is_some_and(SessionRuntimeKernel::has_active_turn)
    );
    drop(terminal_guard);
    // retry_task_id represents a duplicate completion delivery with fresh transport ownership.
    // retry_task_id 代表具有新传输所有权的重复完成投递。
    let retry_task_id = register_pending_context_compaction_test_stream(&state, session_id);
    // retry_outcome must observe both committed domains and avoid appending a second checkpoint.
    // retry_outcome 必须观察到两个已提交领域，并避免追加第二个检查点。
    let retry_outcome = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishActiveStream {
            task_id: retry_task_id,
        },
        retry_execution,
    )
    .expect("duplicate committed compaction should remain successful");
    let retry_snapshot = retry_outcome.into_workspace_snapshot();
    assert_eq!(
        retry_snapshot
            .session_compactions
            .get(&session_id)
            .map(Vec::len),
        Some(1)
    );
    {
        // repair_guard restores only ordinary state while retaining the already committed Core VMM document.
        // repair_guard 仅恢复普通状态，同时保留已经提交的 Core VMM 文档。
        let mut repair_guard = state.lock().expect("lock VMM-only repair fixture");
        repair_guard.workspace_snapshot = Some(precommit_workspace);
    }
    // repair_task_id owns the retry that must reconstruct the missing ordinary projection.
    // repair_task_id 拥有必须重建缺失普通投影的重试。
    let repair_task_id = register_pending_context_compaction_test_stream(&state, session_id);
    // repair_outcome proves a duplicate controller receipt can safely complete the other domain.
    // repair_outcome 证明重复控制器回执能够安全补齐另一领域。
    let repair_outcome = finish_webui_context_compaction_execution(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &agent_runtime,
        session_id,
        SessionRunPhase::Ready,
        &section_id,
        "预算说明",
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
        ContextCompactionCommitMode::FinishActiveStream {
            task_id: repair_task_id,
        },
        repair_execution,
    )
    .expect("VMM-only committed compaction should repair ordinary state");
    let repair_snapshot = repair_outcome.into_workspace_snapshot();
    assert_eq!(
        repair_snapshot
            .session_compactions
            .get(&session_id)
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        repair_snapshot
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| runtime_state.last_compaction_id.as_deref()),
        Some("committed-compaction")
    );
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
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should apply live usage stats");
    let messages = applied
        .workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("task session report should materialize messages");
    let stats_row = messages
        .iter()
        .rev()
        .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
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

/// Verifies a terminal TaskSession report replaces the provisional prompt row with canonical records.
/// 验证 TaskSession 终态报告会用规范记录替换临时提示词行。
#[test]
fn task_session_terminal_report_replaces_seeded_prompt_with_canonical_records() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let prompt = "Return exactly this JSON only.";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    {
        let workspace_snapshot = service_state
            .workspace_snapshot
            .as_mut()
            .expect("test workspace snapshot should exist");
        seed_webui_subagent_task_prompt_if_needed(workspace_snapshot, task_session_id, prompt);
        workspace_snapshot.session_runtime_state.insert(
            task_session_id,
            SessionRuntimeStateEntry {
                conversation_state: SessionConversationState {
                    messages: Vec::new(),
                    records: vec![
                        SessionConversationRecord {
                            id: String::from("inherited-parent-user"),
                            kind: SessionMessageKind::User,
                            record_kind: SessionMessageRecordKind::Transcript,
                            visible: true,
                            title: String::from("用户指令"),
                            body: String::from("Parent visible prompt must stay out."),
                            metadata: None,
                            created_at: String::from("0"),
                        },
                        SessionConversationRecord {
                            id: String::from("inherited-parent-output"),
                            kind: SessionMessageKind::Assistant,
                            record_kind: SessionMessageRecordKind::AssistantOutput,
                            visible: true,
                            title: String::from("输出"),
                            body: String::from("Parent visible output must stay out."),
                            metadata: None,
                            created_at: String::from("0"),
                        },
                        SessionConversationRecord {
                            id: String::from("inherited-parent-stats"),
                            kind: SessionMessageKind::Assistant,
                            record_kind: SessionMessageRecordKind::TurnStats,
                            visible: true,
                            title: String::from("Turn Stats · completed"),
                            body: String::from("Parent visible stats must stay out."),
                            metadata: None,
                            created_at: String::from("0"),
                        },
                    ],
                    continuation: Default::default(),
                },
                ..SessionRuntimeStateEntry::default()
            },
        );
    }
    let state = Arc::new(Mutex::new(service_state));
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("{\"ok\":true}")),
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: Vec::new(),
            records: vec![
                // hidden-terminal-runtime-record represents a runtime-only row that must not enter the display cache.
                // hidden-terminal-runtime-record 表示不得进入显示缓存的仅运行态记录。
                SessionConversationRecord {
                    id: String::from("hidden-terminal-runtime-record"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::SummaryOutput,
                    visible: false,
                    title: String::from("内部摘要"),
                    body: String::from("hidden terminal runtime record"),
                    metadata: None,
                    created_at: String::from("0"),
                },
                SessionConversationRecord {
                    id: String::from("canonical-user-prompt"),
                    kind: SessionMessageKind::User,
                    record_kind: SessionMessageRecordKind::Transcript,
                    visible: true,
                    title: String::from("用户指令"),
                    body: prompt.to_string(),
                    metadata: None,
                    created_at: String::from("1"),
                },
            ],
            continuation: Default::default(),
        }),
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should replace provisional prompt");
    let messages = applied
        .workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("task session report should materialize canonical messages");
    let prompt_messages = messages
        .iter()
        .filter(|message| message.kind == SessionMessageKind::User && message.body == prompt)
        .collect::<Vec<_>>();

    assert_eq!(prompt_messages.len(), 1);
    assert_eq!(prompt_messages[0].id, "canonical-user-prompt");
    assert!(
        messages
            .iter()
            .all(|message| !message.id.contains("subagent-prompt"))
    );
    assert!(
        messages
            .iter()
            .all(|message| message.id != "hidden-terminal-runtime-record")
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.body.contains("hidden terminal runtime record"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.kind == SessionMessageKind::Assistant
                && message.body.contains("{\"ok\":true}"))
    );
    assert!(messages.iter().all(|message| {
        !message
            .body
            .contains("Parent visible prompt must stay out.")
            && !message
                .body
                .contains("Parent visible output must stay out.")
            && !message.body.contains("Parent visible stats must stay out.")
    }));
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
            .count(),
        1
    );
}

/// Verifies a TaskSession checkpoint replaces provisional prompt rows with runtime records.
/// 验证 TaskSession 检查点会用运行态记录替换临时提示词行。
#[test]
fn task_session_checkpoint_replaces_seeded_prompt_with_runtime_records() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let prompt = "Return exactly this JSON only.";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    seed_webui_subagent_task_prompt_if_needed(workspace_snapshot, task_session_id, prompt);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(task_session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: Vec::new(),
                records: vec![SessionConversationRecord {
                    id: String::from("streaming-assistant-output"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::AssistantOutput,
                    visible: true,
                    title: String::from("输出"),
                    body: String::from("{\"ok\":true}"),
                    metadata: None,
                    created_at: String::from("2"),
                }],
                continuation: Default::default(),
            },
            ..SessionRuntimeStateEntry::default()
        });
    let checkpoint_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: String::from("canonical-user-prompt"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: prompt.to_string(),
            metadata: None,
            created_at: String::from("1"),
        }],
        continuation: Default::default(),
    };

    apply_checkpoint_conversation_state(runtime_state, checkpoint_state, "3")
        .expect("valid checkpoint conversation identities");
    let runtime_state_snapshot = workspace_snapshot
        .session_runtime_state
        .get(&task_session_id)
        .expect("runtime state should exist")
        .clone();
    replace_task_display_messages_with_runtime_state_records(
        workspace_snapshot,
        task_session_id,
        &runtime_state_snapshot,
        Some(prompt),
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("checkpoint records should be visible");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "canonical-user-prompt");
    assert_eq!(messages[1].id, "streaming-assistant-output");
    assert!(
        messages
            .iter()
            .all(|message| !message.id.contains("subagent-prompt"))
    );
}

/// Verifies subagent surface cache replacement never preserves stale child records.
/// 验证子代理表面缓存替换不会保留陈旧的子记录。
#[test]
fn subagent_surface_conversation_state_replaces_seeded_prompt_without_preserving_stale_records() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let prompt = "Return exactly this JSON only.";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    seed_webui_subagent_task_prompt_if_needed(workspace_snapshot, task_session_id, prompt);
    workspace_snapshot.session_runtime_state.insert(
        task_session_id,
        SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: Vec::new(),
                records: vec![SessionConversationRecord {
                    id: String::from("stale-assistant-output"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::AssistantOutput,
                    visible: true,
                    title: String::from("输出"),
                    body: String::from("stale"),
                    metadata: None,
                    created_at: String::from("1"),
                }],
                continuation: Default::default(),
            },
            ..SessionRuntimeStateEntry::default()
        },
    );
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("hydrated-parent-user"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: String::from("Hydrated parent prompt must stay out."),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionConversationRecord {
                id: String::from("hydrated-parent-stats"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::TurnStats,
                visible: true,
                title: String::from("Turn Stats · completed"),
                body: String::from("Hydrated parent stats must stay out."),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionConversationRecord {
                id: String::from("canonical-user-prompt"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: prompt.to_string(),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionConversationRecord {
                id: String::from("canonical-assistant-output"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("{\"ok\":true}"),
                metadata: None,
                created_at: String::from("3"),
            },
        ],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        Some(prompt),
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("canonical child messages should be visible");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "canonical-user-prompt");
    assert_eq!(messages[1].id, "canonical-assistant-output");
    assert!(
        messages
            .iter()
            .all(|message| !message.id.contains("subagent-prompt") && message.body != "stale")
    );
    assert!(messages.iter().all(|message| {
        !message
            .body
            .contains("Hydrated parent prompt must stay out.")
            && !message
                .body
                .contains("Hydrated parent stats must stay out.")
    }));
}

/// Verifies anchorless cloned child state starts display at the latest child task prompt.
/// 验证缺少锚点的克隆子状态会从最新子任务提示开始显示。
#[test]
fn subagent_surface_conversation_state_without_anchor_drops_inherited_parent_history() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("cloned-parent-user"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: String::from("Parent prompt cloned into runtime only."),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionConversationRecord {
                id: String::from("cloned-parent-assistant"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("Parent answer must not be shown in child panel."),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionConversationRecord {
                id: String::from("latest-child-prompt"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: String::from(
                    "Recall inherited runtime context, but do not show parent chat.",
                ),
                metadata: None,
                created_at: String::from("3"),
            },
            SessionConversationRecord {
                id: String::from("latest-child-output"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("Only child-owned display records remain."),
                metadata: None,
                created_at: String::from("4"),
            },
        ],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("scoped child messages should exist");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["latest-child-prompt", "latest-child-output"]
    );
    assert!(messages.iter().all(|message| {
        !message.body.contains("Parent prompt") && !message.body.contains("Parent answer")
    }));
}

/// Verifies subagent hydration uses canonical display-only records while dropping stale records.
/// 验证子智能体水合使用规范仅显示记录，同时丢弃陈旧记录。
#[test]
fn subagent_surface_conversation_state_uses_canonical_display_only_agent_messages() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let stale_record =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "stale output")
            .with_runtime_metadata("stale-output", "1780000000001");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![stale_record]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("display-note-send-message"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::DisplayNote,
                visible: true,
                title: String::from("智能体消息"),
                body: String::from("请用中文重新输出结论。"),
                metadata: Some(json!({
                    "display_note": true,
                    "kind": "subagent_message",
                    "tool": "send_message",
                    "delivery_id": "display-note-send-message",
                })),
                created_at: String::from("1780000000000"),
            },
            SessionConversationRecord {
                id: String::from("canonical-assistant-output"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("已按中文重新输出。"),
                metadata: None,
                created_at: String::from("1780000000003"),
            },
        ],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("hydrated child messages should exist");
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().any(|message| {
        message.record_kind == SessionMessageRecordKind::DisplayNote
            && message.body == "请用中文重新输出结论。"
    }));
    assert!(messages.iter().any(|message| {
        message.record_kind == SessionMessageRecordKind::AssistantOutput
            && message.body == "已按中文重新输出。"
    }));
    assert!(
        !messages
            .iter()
            .any(|message| message.body == "stale output")
    );
}

/// Verifies runtime refresh drops stale parent-to-child visible message notes.
/// 验证运行态刷新会丢弃陈旧的父到子可见消息记录。
#[test]
fn subagent_surface_conversation_state_drops_stale_send_message_notes() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let mut send_note = SessionMessageEntry::new(
        SessionMessageKind::User,
        "智能体消息",
        "你之前做了什么？请回忆并列出。",
    )
    .with_runtime_metadata("delivery-followup-1", "1780000000001");
    send_note.record_kind = SessionMessageRecordKind::DisplayNote;
    send_note.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message",
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "delivery_id": "delivery-followup-1",
    }));
    let stale_record =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "stale output")
            .with_runtime_metadata("stale-output", "1780000000000");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![stale_record, send_note]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: String::from("canonical-assistant-output"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("我只输出过 abcd。"),
            metadata: None,
            created_at: String::from("1780000000003"),
        }],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("hydrated child messages should exist");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.body.as_str())
            .collect::<Vec<_>>(),
        vec!["我只输出过 abcd。"]
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.body == "stale output")
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.body == "你之前做了什么？请回忆并列出。")
    );
}

/// Verifies pending parent-to-child notes stay visible until runtime owns them.
/// 验证待处理的父到子消息在运行时接管前保持可见。
#[test]
fn subagent_surface_conversation_state_preserves_pending_send_message_notes() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let mut send_note =
        SessionMessageEntry::new(SessionMessageKind::User, "智能体消息", "请继续输出 1234。")
            .with_runtime_metadata("delivery-followup-2", "1780000000005");
    send_note.record_kind = SessionMessageRecordKind::DisplayNote;
    send_note.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message",
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "delivery_id": "delivery-followup-2",
    }));
    let previous_output =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "previous output")
            .with_runtime_metadata("previous-output", "1780000000001");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![previous_output, send_note]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: String::from("canonical-previous-output"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("previous output"),
            metadata: None,
            created_at: String::from("1780000000001"),
        }],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("hydrated child messages should exist");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.body.as_str())
            .collect::<Vec<_>>(),
        vec!["previous output", "请继续输出 1234。"]
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
        .insert(parent_session_id);
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

/// Verifies missing persistence runtime rejects send-message projection before any in-memory commit.
/// 验证缺少持久化运行时时会在任何内存提交前拒绝 send-message 投影。
#[test]
fn send_message_projection_preflight_failure_preserves_in_memory_state() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("output_abcd"),
        parent_session_id,
        subagent_label: String::from("output_abcd"),
        description: Some(String::from("输出 abcd")),
        run_state: SubagentTaskRunState::Completed,
        result_text: String::from("abcd"),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut service_state, &task_states);
    // original_cache captures the loaded child surface before the preflight failure.
    // original_cache 捕获预检失败前已加载的子会话表面。
    let original_cache = service_state
        .subagent_surface_detail_cache
        .get(&task_session_id)
        .expect("loaded child detail cache should exist")
        .clone();
    // original_workspace_messages captures the retained transcript map before projection.
    // original_workspace_messages 捕获投影前保留的转录映射。
    let original_workspace_messages = serde_json::to_value(
        &service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot")
            .session_messages,
    )
    .expect("workspace messages should serialize");
    let state = Arc::new(Mutex::new(service_state));
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-send-cache",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "send_message",
        "tool_call_id": "call-send-cache",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "parent_session_id": parent_session_id.to_string(),
                "target": "output_abcd",
                (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
                "delivery_id": "delivery-cache-followup",
                "message": "你之前做了什么？请回忆并列出。",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false
            }
        }
    })));

    let projection_result =
        append_runtime_event_projection(&app, &state, parent_session_id, &event);
    let error = projection_result.expect_err("missing controller runtime must fail preflight");
    assert!(error.contains("controller runtime is not ready"));

    let guard = state.lock().expect("service state lock");
    let retained_cache = guard
        .subagent_surface_detail_cache
        .get(&task_session_id)
        .expect("loaded child detail cache should remain");
    assert_eq!(
        serde_json::to_value(&retained_cache.task_state)
            .expect("retained task state should serialize"),
        serde_json::to_value(&original_cache.task_state)
            .expect("original task state should serialize")
    );
    assert_eq!(
        serde_json::to_value(&retained_cache.messages)
            .expect("retained cache messages should serialize"),
        serde_json::to_value(&original_cache.messages)
            .expect("original cache messages should serialize")
    );
    assert_eq!(retained_cache.last_access_ms, original_cache.last_access_ms);
    assert_eq!(
        serde_json::to_value(
            &guard
                .workspace_snapshot
                .as_ref()
                .expect("workspace snapshot should remain")
                .session_messages,
        )
        .expect("retained workspace messages should serialize"),
        original_workspace_messages
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
        .insert(parent_session_id);
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

    let stale_cleared = clear_webui_detached_listener_execution_if_current(
        &mut state,
        task_id,
        "execution-old",
        true,
    );

    assert!(!stale_cleared);
    assert_eq!(
        state.detached_subagent_task_listeners.get(&task_id),
        Some(&String::from("execution-new"))
    );
    let current_cleared = clear_webui_detached_listener_execution_if_current(
        &mut state,
        task_id,
        "execution-new",
        true,
    );
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
            .contains(&parent_session_id)
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
            .insert(parent_session_id);
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
    refresh_webui_subagent_navigation_round_visibility(&state, parent_session_id)
        .expect("terminal cleanup should refresh navigation visibility");
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

/// Verifies child task tool events can patch known subagent transcripts even when the parent is active.
/// 验证即使父会话处于活动界面，子任务工具事件也允许修补已知子代理转录。
#[test]
fn child_runtime_tool_event_patch_gate_accepts_known_subagent_transcript() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    cache_test_subagent_surface_detail_states(
        &mut guard,
        &[SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("code-review"),
            parent_session_id,
            subagent_label: String::from("代码审核"),
            description: Some(String::from("审核代码")),
            run_state: SubagentTaskRunState::Running,
            result_text: String::new(),
            conversation_state: None,
        }],
    );

    assert!(workbench_should_emit_runtime_event_message_patch(
        &guard,
        &task_session_id.to_string(),
        task_session_id
    ));
    assert!(!workbench_should_emit_runtime_event_message_patch(
        &guard,
        &SessionId::new_v4().to_string(),
        SessionId::new_v4()
    ));
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
        .insert(parent_session_id);
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

/// Verifies close-agent cleanup removes closed tasks while preserving other unclosed agents.
/// 验证关闭代理清理会移除已关闭任务，同时保留其他未关闭代理。
#[test]
fn close_agent_cleanup_removes_closed_webui_subagent_cache_entries() {
    let parent_session_id = SessionId::new_v4();
    let closed_task_id = SessionId::new_v4();
    let retained_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let closed_state = test_completed_subagent_panel_state(parent_session_id, closed_task_id);
    let retained_state = test_completed_subagent_panel_state(parent_session_id, retained_task_id);
    cache_test_subagent_surface_detail_states(
        &mut guard,
        &[closed_state.clone(), retained_state.clone()],
    );
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

    assert!(remove_webui_closed_subagent_tasks_in_state(
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
    assert!(
        !guard
            .detached_subagent_task_listeners
            .contains_key(&closed_task_id)
    );
    assert!(guard.active_subagent_panel.is_none());
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
        runtime_state.last_safe_provider_request_count = Some(2);
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

/// Verifies stopped parents never dispatch queued work when detached subagents report terminal events.
/// 验证已停止父会话收到分离子代理终态事件时绝不会调度排队工作。
#[test]
fn stopped_detached_parent_event_skips_pending_prompt_dispatch() {
    assert!(webui_detached_parent_event_should_continue_pending_prompt(
        true, false
    ));
    assert!(!webui_detached_parent_event_should_continue_pending_prompt(
        true, true
    ));
    assert!(!webui_detached_parent_event_should_continue_pending_prompt(
        false, false
    ));
    assert!(webui_detached_parent_event_should_persist_terminal_projection(true, true, false));
    assert!(!webui_detached_parent_event_should_persist_terminal_projection(true, true, true));
    assert!(!webui_detached_parent_event_should_persist_terminal_projection(false, true, false));
}

/// Verifies send_message-resumed child work no longer blocks the parent completion gate.
/// 验证由 send_message 续跑的子任务不会继续阻塞父级完成闸门。
#[test]
fn completion_gate_ignores_send_message_resumed_child_task() {
    // ignored_execution_id identifies the exact send_message-resumed execution.
    // ignored_execution_id 标识由 send_message 续跑的精确执行。
    let ignored_execution_id = "ignored-completion-execution";
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let runtime = test_agent_runtime_bridge();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    {
        let mut guard = state.lock().expect("state lock");
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id, String::from(ignored_execution_id));
        let task_states = vec![SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("reviewer"),
            parent_session_id,
            subagent_label: String::from("Reviewer"),
            description: Some(String::from("Review code")),
            run_state: SubagentTaskRunState::Running,
            result_text: String::new(),
            conversation_state: None,
        }];
        cache_test_subagent_surface_detail_states(&mut guard, &task_states);
        seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            parent_session_id,
            &task_states,
        );
    }

    let default_gate = webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
        .expect("default gate should read");
    assert_eq!(default_gate.pending_subagent_count, 1);

    {
        let mut guard = state.lock().expect("state lock");
        mark_webui_subagent_completion_gate_ignored(
            &mut guard,
            task_session_id,
            ignored_execution_id,
        );
    }

    let ignored_gate = webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
        .expect("ignored gate should read");
    assert_eq!(ignored_gate.pending_subagent_count, 0);

    {
        let mut guard = state.lock().expect("state lock");
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id, String::from("replacement-execution"));
    }

    let replacement_gate =
        webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
            .expect("replacement gate should read");
    assert_eq!(replacement_gate.pending_subagent_count, 1);
}

/// Verifies runtime and live transcript writers share one monotonic session ticket.
/// 验证运行态与运行中转录写入者共享同一个单调递增的会话票据。
#[test]
fn live_transcript_persist_version_advances_shared_session_ticket() {
    // session_id identifies the session whose persistence lanes are compared.
    // session_id 标识需要比较持久化通道的会话。
    let session_id = SessionId::new_v4();
    // state carries compatibility indexes backed by one shared session ticket.
    // state 携带由共享会话票据驱动的兼容索引。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));

    // runtime_version is the first shared session ticket.
    // runtime_version 是第一个共享会话票据。
    let runtime_version = next_webui_session_runtime_persist_version(&state, session_id)
        .expect("runtime persist version should allocate");
    // transcript_version advances the same ticket and invalidates older runtime snapshots.
    // transcript_version 推进同一票据，并使旧运行态快照失效。
    let transcript_version = next_webui_session_transcript_persist_version(&state, session_id)
        .expect("transcript persist version should allocate");

    assert!(transcript_version > runtime_version);
    assert!(!webui_session_runtime_persist_version_is_current(
        &state,
        session_id,
        runtime_version,
    ));
    assert!(webui_session_transcript_persist_version_is_current(
        &state,
        session_id,
        transcript_version
    ));
    assert!(!webui_session_runtime_persist_version_is_current(
        &state,
        session_id,
        transcript_version.saturating_add(1),
    ));
}

/// Verifies detached subagent completion enqueues parent mailbox input without using the pending prompt queue.
/// 验证没有 wait_agent 接管时，分离子代理完成会写入父级 mailbox 而不使用 pending prompt 队列。
#[test]
fn subagent_completion_queues_session_mailbox_without_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "image-a", "生成完成：a.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("enqueue mailbox input");

    assert!(queued);
    let mailbox_prompt = webui_session_mailbox_input_from_kernel(
        &state,
        parent_session_id,
        Some(&task_session_id.to_string()),
    )
    .expect("mailbox prompt should read")
    .expect("mailbox prompt should exist");
    assert!(mailbox_prompt.contains("生成完成：a.jpg"));
    assert!(mailbox_prompt.contains("<message_type>FINAL_ANSWER</message_type>"));
    assert!(!mailbox_prompt.contains("<subagent_mailbox_event>"));
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();
    assert_eq!(mailbox_inputs.len(), 1);
    assert_eq!(mailbox_inputs[0].kind, TurnInputKind::SubagentMessage);
    assert!(mailbox_inputs[0].trigger_turn);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(
        mailbox_inputs[0].payload["mailbox_event"]["payload"]["result"],
        "生成完成：a.jpg"
    );
}

/// Verifies detached mailbox delivery is idempotent per execution, not per long-lived task.
/// 验证分离 mailbox 投递按单次执行幂等，而不是按长期任务幂等。
#[test]
fn subagent_completion_queues_each_distinct_execution_for_same_task() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let first_event =
        test_delegate_finished_event(task_session_id, "repeat-agent-run-1", "第一次结果");
    let second_event =
        test_delegate_finished_event(task_session_id, "repeat-agent-run-2", "第二次结果");

    let first_queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &first_event,
        true,
        false,
    )
    .expect("first mailbox input should inspect");
    let duplicate_first = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &first_event,
        true,
        false,
    )
    .expect("duplicate mailbox input should inspect");
    let second_queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &second_event,
        true,
        false,
    )
    .expect("second mailbox input should inspect");

    assert!(first_queued);
    assert!(!duplicate_first);
    assert!(second_queued);
    let mailbox_prompt = webui_session_mailbox_input_from_kernel(
        &state,
        parent_session_id,
        Some(&task_session_id.to_string()),
    )
    .expect("mailbox prompt should read")
    .expect("mailbox prompt should exist");
    assert!(mailbox_prompt.contains("第一次结果"));
    assert!(mailbox_prompt.contains("第二次结果"));
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();
    assert_eq!(mailbox_inputs.len(), 2);
    assert!(mailbox_inputs.iter().any(|input| {
        input.payload["execution_id"] == "test-execution-repeat-agent-run-1"
            && input.payload["mailbox_event"]["payload"]["result"] == "第一次结果"
    }));
    assert!(mailbox_inputs.iter().any(|input| {
        input.payload["execution_id"] == "test-execution-repeat-agent-run-2"
            && input.payload["mailbox_event"]["payload"]["result"] == "第二次结果"
    }));
}

/// Verifies detached subagent completion still enqueues mailbox input while wait_agent is active.
/// 验证 wait_agent 活跃期间，分离子代理完成仍会加入 mailbox 输入。
#[test]
fn subagent_completion_queues_session_mailbox_during_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .parent_sessions_waiting_for_subagents
        .insert(parent_session_id);
    let state = Arc::new(Mutex::new(service_state));
    let event = test_delegate_finished_event(task_session_id, "image-b", "生成完成：b.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("queue mailbox input");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies child completion mailbox input queues after wait_agent coordination ends.
/// 验证 wait_agent 协调结束后，子代理完成 mailbox 输入仍会入队。
#[test]
fn subagent_completion_queues_session_mailbox_after_completed_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "image-c", "生成完成：c.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("mailbox input should queue");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies child completion mailbox input no longer depends on legacy wait-agent selectors.
/// 验证子代理完成 mailbox 输入不再依赖旧 wait_agent 选择器。
#[test]
fn subagent_completion_after_completed_wait_agent_allows_any_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "agent-unmatched", "done");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("unmatched child should enqueue mailbox input");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies child-agent completion appends independent feedback rows instead of parent turn usage.
/// 验证智能体完成时追加独立反馈记录，而不是写入父回合用量。
#[test]
fn subagent_completion_appends_independent_parent_feedback_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "image-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-image-worker",
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "生成完成：a.jpg",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "image-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 100,
                "output_tokens": 25,
                "reasoning_tokens": 5,
                "total_tokens": 130,
                "cached_input_tokens": 40
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );
    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("duplicate delegate finished should ingest without appending")
    );

    let guard = state.lock().expect("service state lock");
    let messages = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .expect("parent should contain display feedback rows");
    let feedback_rows = messages
        .iter()
        .filter(|message| {
            message.metadata.as_ref().is_some_and(|metadata| {
                metadata
                    .get("subagent_feedback_record")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(feedback_rows.len(), 2);
    let feedback_row = feedback_rows
        .iter()
        .find(|message| message.record_kind == SessionMessageRecordKind::DisplayNote)
        .expect("one feedback row should carry child-agent result metadata");
    assert_eq!(feedback_row.body, "智能体返回处理结果");
    assert_eq!(
        feedback_row
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("host_result"))
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("result"))
            .and_then(serde_json::Value::as_str),
        Some("生成完成：a.jpg")
    );
    let stats_row = feedback_rows
        .iter()
        .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("one feedback row should carry child-agent stats");
    let stats: SessionTurnStats = serde_json::from_value(
        stats_row
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("turn_stats"))
            .expect("stats metadata should exist")
            .clone(),
    )
    .expect("stats metadata should parse");

    assert_eq!(stats.turn_kind, SessionTurnStatsKind::SubagentFeedback);
    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 100);
    assert_eq!(stats.output_tokens, 25);
    assert_eq!(stats.reasoning_tokens, 5);
    assert_eq!(stats.total_tokens, 130);
    assert_eq!(stats.cached_input_tokens, Some(40));
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::SubagentTask
    );
    assert_eq!(stats.usage_groups.len(), 1);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::Subagent
    );
    assert_eq!(stats.usage_groups[0].total_tokens, 130);
    assert_eq!(stats.usage_groups[0].cached_input_tokens, Some(40));
}

/// Verifies active parent user turns still receive independent child feedback display rows.
/// 验证活动父级用户回合仍会接收独立智能体反馈显示行。
#[test]
fn active_parent_turn_appends_independent_subagent_feedback_display_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent user turn should start");
    bind_webui_subagent_parent_turn(
        &mut service_state,
        parent_session_id,
        task_session_id,
        "test-execution-image-worker-active",
    );
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "image-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-image-worker-active",
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "生成完成：a.jpg",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "image-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 100,
                "output_tokens": 25,
                "reasoning_tokens": 5,
                "total_tokens": 130,
                "cached_input_tokens": 40
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(
        feedback_rows.len(),
        2,
        "active parent user turns without wait_agent ownership should append independent feedback and stats rows"
    );
    for row in feedback_rows {
        let metadata = row
            .metadata
            .as_ref()
            .expect("feedback metadata should exist");
        assert_eq!(
            metadata
                .get("parent_turn_id")
                .and_then(serde_json::Value::as_str),
            Some("turn-a")
        );
        assert_eq!(
            metadata
                .get("parent_turn_binding")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(
            metadata
                .get("source_generation")
                .and_then(serde_json::Value::as_u64)
                .is_some()
        );
    }
}

/// Verifies active workspace run phases still receive independent child feedback display rows.
/// 验证活跃工作区运行阶段仍会接收独立智能体反馈显示行。
#[test]
fn active_parent_workspace_phase_appends_independent_subagent_feedback_display_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist")
        .session_run_phases
        .insert(parent_session_id, SessionRunPhase::Requesting);
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = test_delegate_finished_event(task_session_id, "phase-worker", "阶段中完成");

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(
        feedback_rows.len(),
        2,
        "active parent phases without wait_agent ownership should append independent feedback and stats rows"
    );
}

/// Verifies wait_agent-owned feedback suppresses independent parent feedback rows.
/// 验证 wait_agent 接管的反馈会抑制独立父级反馈记录。
#[test]
fn wait_agent_feedback_suppresses_independent_parent_feedback_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .parent_sessions_waiting_for_subagents
        .insert(parent_session_id);
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope(
            "wait-agent-active",
            [
                task_session_id.to_string(),
                second_task_session_id.to_string(),
            ],
            current_unix_millis_u64(),
        )
        .expect("wait scope should register");
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "wait-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-wait-worker",
            "execution_status": "completed",
            "description": "等待反馈",
            "subagent": "wait-worker",
            "result": "wait 已拿到正文",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "wait-worker",
                "provider_call_count": 2,
                "tool_call_count": 1,
                "input_tokens": 200,
                "output_tokens": 50,
                "reasoning_tokens": 10,
                "total_tokens": 260,
                "cached_input_tokens": 80
            }]
        }
    })));
    let second_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "第二个智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "wait-worker-b",
            (SUBAGENT_TASK_ID_KEY): second_task_session_id.to_string(),
            "execution_id": "test-execution-wait-worker-b",
            "execution_status": "completed",
            "description": "等待反馈 B",
            "subagent": "wait-worker-b",
            "result": "wait 已拿到第二条正文",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "wait-worker-b",
                "provider_call_count": 3,
                "tool_call_count": 0,
                "input_tokens": 300,
                "output_tokens": 75,
                "reasoning_tokens": 15,
                "total_tokens": 390,
                "cached_input_tokens": 120
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );
    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &second_event,
        )
        .expect("second delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        feedback_rows.is_empty(),
        "wait_agent-owned feedback must stay inside the wait_agent tool result instead of creating separate parent rows"
    );
}

/// Verifies feedback consumed by a completed wait_agent suppresses delayed parent feedback rows.
/// 验证已完成 wait_agent 消费过的反馈会抑制迟到的父级反馈记录。
#[test]
fn consumed_wait_agent_feedback_suppresses_delayed_parent_feedback_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    assert!(consume_webui_subagent_mailbox_input_for_execution(
        &mut service_state,
        parent_session_id,
        task_session_id,
        "delayed-wait-worker-run",
    ));
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "迟到的智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "delayed-wait-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "delayed-wait-worker-run",
            "execution_status": "completed",
            "description": "迟到反馈",
            "subagent": "delayed-wait-worker",
            "result": "wait 已经展示过这段正文",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "delayed-wait-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 10,
                "output_tokens": 2,
                "reasoning_tokens": 1,
                "total_tokens": 13,
                "cached_input_tokens": 4
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delayed delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        feedback_rows.is_empty(),
        "delayed wait-owned feedback must not create separate parent rows"
    );
}

/// Verifies feedback deduplication is scoped to one concrete child-agent execution.
/// 验证反馈去重只作用于一次具体智能体执行。
#[test]
fn subagent_feedback_dedupes_by_execution_id_not_task_id_only() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let first_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体第一次任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "repeat-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "repeat-worker-run-1",
            "execution_status": "completed",
            "description": "重复智能体",
            "subagent": "repeat-worker",
            "result": "第一次完成",
            "usage_segments": []
        }
    })));
    let second_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体第二次任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "repeat-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "repeat-worker-run-2",
            "execution_status": "completed",
            "description": "重复智能体",
            "subagent": "repeat-worker",
            "result": "第二次完成",
            "usage_segments": []
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &first_event
        )
        .expect("first delegate finished should ingest")
    );
    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &first_event
        )
        .expect("duplicate delegate finished should ingest without appending")
    );
    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &second_event,
        )
        .expect("second execution should append independent feedback")
    );

    let guard = state.lock().expect("service state lock");
    let messages = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .expect("parent should contain both execution feedback rows");
    let feedback_rows = messages
        .iter()
        .filter(|message| {
            message.metadata.as_ref().is_some_and(|metadata| {
                metadata
                    .get("subagent_feedback_record")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(feedback_rows.len(), 4);
    let feedback_results = feedback_rows
        .iter()
        .filter_map(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("host_result"))
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("result"))
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    assert!(feedback_results.contains(&"第一次完成"));
    assert!(feedback_results.contains(&"第二次完成"));
}

/// Verifies malformed child-agent usage data does not append zero-token feedback stats.
/// 验证格式错误的智能体用量数据不会追加 0-token 反馈统计。
#[test]
fn malformed_subagent_feedback_usage_segments_do_not_append_zero_stats() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "bad-usage-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-bad-usage-worker",
            "execution_status": "completed",
            "description": "返回坏统计",
            "subagent": "bad-usage-worker",
            "result": "完成但统计坏了",
            "usage_segments": {
                "kind": "subagent_task",
                "input_tokens": 100
            }
        }
    })));

    let error =
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect_err("malformed usage segments must reject the feedback projection");

    assert!(error.contains("failed to parse subagent feedback usage segments"));
    let guard = state.lock().expect("service state lock");
    let feedback_count = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(feedback_count, 0);
}

/// Verifies multiple detached subagent completions render as one blank-line-separated mailbox prompt.
/// 验证多条分离子代理完成消息会渲染为一个使用空行分隔的 mailbox 提示词。
#[test]
fn subagent_completion_mailbox_prompt_joins_multiple_inputs() {
    let parent_session_id = SessionId::new_v4();
    let first_task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let first_event =
        test_delegate_finished_event(first_task_session_id, "image-a", "生成完成：a.jpg");
    let second_event =
        test_delegate_finished_event(second_task_session_id, "image-b", "生成完成：b.jpg");

    assert!(
        enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &first_event,
            true,
            false,
        )
        .expect("first mailbox input should enqueue")
    );
    assert!(
        enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &second_event,
            true,
            false,
        )
        .expect("second mailbox input should enqueue")
    );

    let mailbox_prompt = webui_session_mailbox_input_from_kernel(&state, parent_session_id, None)
        .expect("mailbox prompt should read")
        .expect("mailbox prompt should exist");

    assert!(mailbox_prompt.contains("生成完成：a.jpg"));
    assert!(mailbox_prompt.contains("生成完成：b.jpg"));
    assert_eq!(mailbox_prompt.matches("<agent_message>").count(), 2);
    assert!(mailbox_prompt.contains("</agent_message>\n\n<agent_message>"));
}
