//! goal todolist tests.
//! goal todolist tests。

use super::*;
use crate::session_goal_lifecycle::session_goal_execution_context;
use crate::session_runtime_registry_v1::{SessionHandle, SessionPersistenceBackend};
use std::sync::atomic::{AtomicU64, Ordering};
use vulcan_agent_runtime::tooling::native::{
    NativeGoalRuntime, NativeGoalStatus, NativeGoalTerminalStatus,
};
use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, EventBody, SystemContextOperation, SystemContextScope,
    SystemRoleHint,
};
use vulcan_workbench_model::{SessionTodolistItem, SessionTodolistState};
use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

/// Successful in-memory persistence seam for native V1 Goal service tests.
/// 用于 native V1 Goal 服务测试的成功内存持久化接缝。
struct NativeGoalTestPersistenceBackend {
    /// Number of durable candidates accepted by the adapter.
    /// 适配器接纳的可靠候选数量。
    flush_count: AtomicU64,
}

impl NativeGoalTestPersistenceBackend {
    /// Creates one empty deterministic persistence recorder.
    /// 创建一个空的确定性持久化记录器。
    fn new() -> Self {
        Self {
            flush_count: AtomicU64::new(0),
        }
    }
}

impl SessionPersistenceBackend for NativeGoalTestPersistenceBackend {
    /// Accepts one complete V1 candidate and returns its published metadata.
    /// 接纳一份完整 V1 候选并返回其已发布 metadata。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        self.flush_count.fetch_add(1, Ordering::SeqCst);
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Builds a native Goal runtime backed only by one registered V1 SessionHandle.
/// 构建一个仅由已注册 V1 SessionHandle 支撑的 native Goal 运行时。
fn registered_native_goal_runtime(
    session_id: SessionId,
) -> (
    Arc<dyn NativeGoalRuntime>,
    Arc<SessionHandle>,
    Arc<NativeGoalTestPersistenceBackend>,
    SharedServiceBridgeState,
) {
    let backend = Arc::new(NativeGoalTestPersistenceBackend::new());
    let document = SessionDocument::new(session_id.to_string(), "project-a", "Goal test", 1);
    let handle = SessionHandle::with_backend(session_id, document, backend.clone());
    let service_state = test_service_state_with_parent_session(session_id);
    service_state
        .session_runtime_registry
        .register(handle.clone())
        .expect("V1 Goal test handle should register");
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    (
        crate::session_goals::session_v1_native_goal_runtime(
            state.clone(),
            ServiceEventEmitter::new(Vec::new()),
        ),
        handle,
        backend,
        state,
    )
}

/// Verifies native get/create/block/complete use the registered V1 document exclusively.
/// 验证 native get/create/block/complete 只使用已注册的 V1 文档。
#[tokio::test]
async fn native_goal_runtime_get_create_block_and_complete_use_v1() {
    let session_id = SessionId::from_u128(0x6101);
    let (runtime, handle, backend, _state) = registered_native_goal_runtime(session_id);

    assert_eq!(
        runtime
            .get_goal(session_id)
            .await
            .expect("empty V1 Goal should read"),
        None
    );
    let created = runtime
        .create_goal(session_id, String::from("finish stage six"), Some(2_000))
        .await
        .expect("native Goal should create through V1");
    assert_eq!(created.status, NativeGoalStatus::Active);
    assert_eq!(created.objective, "finish stage six");
    assert_eq!(created.token_budget, Some(2_000));
    assert_eq!(
        runtime
            .get_goal(session_id)
            .await
            .expect("created V1 Goal should read")
            .as_ref()
            .map(|goal| goal.goal_id.as_str()),
        Some(created.goal_id.as_str())
    );

    let blocked = runtime
        .update_goal_status(session_id, NativeGoalTerminalStatus::Blocked)
        .await
        .expect("active Goal should block through V1");
    assert_eq!(blocked.status, NativeGoalStatus::Blocked);
    let completed = runtime
        .update_goal_status(session_id, NativeGoalTerminalStatus::Complete)
        .await
        .expect("blocked Goal should complete through V1");
    assert_eq!(completed.status, NativeGoalStatus::Complete);
    assert_eq!(completed.goal_id, created.goal_id);
    assert_eq!(
        handle
            .read(|document| document.metadata.mode.goal.state)
            .expect("V1 Goal state should read"),
        vulcan_session_core::v1::config::GoalState::Completed
    );
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 3);
}

/// Observes Goal deltas at emission time and verifies their source is already durable.
/// 在 Goal 增量发送时观察状态，并验证其来源已经可靠落盘。
struct NativeGoalEventCapture {
    /// Exact V1 owner whose state must precede each emitted delta.
    /// 状态必须先于每条增量提交的精确 V1 所有者。
    handle: Arc<SessionHandle>,
    /// Committed statuses received through the same event interface as the WebSocket bus.
    /// 经由与 WebSocket 总线相同事件接口接收的已提交状态。
    statuses: Mutex<Vec<SessionGoalStatus>>,
}

impl vulcan_service_api::ServiceEventSink for NativeGoalEventCapture {
    /// Records the incoming Goal delta after checking durability; returns no value.
    /// 检查可靠性后记录传入的 Goal 增量；无返回值。
    fn emit_service_event(&self, event: ServiceEvent) {
        if let ServiceEvent::WorkbenchDelta(envelope) = event
            && let WorkbenchDelta::SessionAddonsChanged(delta) = envelope.delta
            && let Some(goal) = delta.session_goal
        {
            self.handle
                .read(|document| {
                    assert_eq!(document.memory_revision(), document.durable_revision())
                })
                .unwrap();
            self.statuses.lock().unwrap().push(goal.status);
        }
    }
}

/// Verifies native mutations push current Goal status without opening a panel or sending another prompt.
/// 验证原生修改无需打开面板或发送下一条提示词即可推送当前 Goal 状态。
/// Parameters: none. Returns: none; asserts the ordered creation/completion/replacement events.
/// 参数：无。返回值：无；断言创建、完成、替换的有序事件。
#[tokio::test]
async fn native_goal_runtime_pushes_committed_state_immediately() {
    // Fixture and capture share the same registered V1 document.
    // 夹具与观察器共享同一已注册 V1 文档。
    let session_id = SessionId::new_v4();
    let (_, handle, _, state) = registered_native_goal_runtime(session_id);
    let capture = Arc::new(NativeGoalEventCapture {
        handle,
        statuses: Default::default(),
    });
    let runtime = crate::session_goals::session_v1_native_goal_runtime(
        state,
        ServiceEventEmitter::new(vec![capture.clone()]),
    );
    runtime
        .create_goal(session_id, String::from("first"), None)
        .await
        .unwrap();
    runtime
        .update_goal_status(session_id, NativeGoalTerminalStatus::Complete)
        .await
        .unwrap();
    runtime
        .create_goal(session_id, String::from("next"), None)
        .await
        .unwrap();
    assert_eq!(
        *capture.statuses.lock().unwrap(),
        vec![
            SessionGoalStatus::Active,
            SessionGoalStatus::Complete,
            SessionGoalStatus::Active
        ]
    );
}

/// Verifies saving after completion creates one new active Goal generation atomically.
/// 验证完成后保存会原子创建一个新的活动 Goal 代际。
#[tokio::test]
async fn native_goal_runtime_completed_goal_creates_new_active_generation() {
    let session_id = SessionId::from_u128(0x6102);
    let (runtime, handle, backend, _state) = registered_native_goal_runtime(session_id);
    let first = runtime
        .create_goal(session_id, String::from("first generation"), Some(100))
        .await
        .expect("first Goal should create");
    runtime
        .update_goal_status(session_id, NativeGoalTerminalStatus::Complete)
        .await
        .expect("first Goal should complete");

    let second = runtime
        .create_goal(session_id, String::from("second generation"), Some(200))
        .await
        .expect("completed Goal should be replaced through save_goal");

    assert_eq!(second.status, NativeGoalStatus::Active);
    assert_eq!(second.objective, "second generation");
    assert_eq!(second.token_budget, Some(200));
    assert_ne!(second.goal_id, first.goal_id);
    assert!(
        handle
            .read(|document| {
                document.metadata.mode.goal.enabled
                    && document.metadata.mode.goal.state
                        == vulcan_session_core::v1::config::GoalState::Active
            })
            .expect("replacement Goal state should read")
    );
    assert_eq!(backend.flush_count.load(Ordering::SeqCst), 3);
}

/// Verifies WebUI todolist summaries preserve status keys and completed counts.
/// 验证 WebUI todolist 摘要会保留状态键与完成数量。
#[test]
fn session_todolist_summary_counts_completed_items() {
    let runtime_state = SessionRuntimeStateEntry {
        todolist: SessionTodolistState {
            explanation: Some(String::from("审核修复")),
            items: vec![
                SessionTodolistItem {
                    step: String::from("完成菜单接入"),
                    status: SessionTodolistItemStatus::Completed,
                },
                SessionTodolistItem {
                    step: String::from("补充测试"),
                    status: SessionTodolistItemStatus::InProgress,
                },
                SessionTodolistItem {
                    step: String::from("等待复核"),
                    status: SessionTodolistItemStatus::Pending,
                },
            ],
            updated_at: Some(String::from("2026-06-17T00:00:00Z")),
        },
        ..SessionRuntimeStateEntry::default()
    };

    let summary = session_todolist_summary_from_runtime_state(Some(&runtime_state))
        .expect("summary should exist");

    assert_eq!(summary.explanation.as_deref(), Some("审核修复"));
    assert_eq!(summary.total_count, 3);
    assert_eq!(summary.completed_count, 1);
    assert_eq!(summary.items[0].step, "完成菜单接入");
    assert_eq!(summary.items[0].status, "completed");
    assert_eq!(summary.items[1].step, "补充测试");
    assert_eq!(summary.items[1].status, "in_progress");
    assert_eq!(summary.items[2].step, "等待复核");
    assert_eq!(summary.items[2].status, "pending");
    assert_eq!(summary.updated_at.as_deref(), Some("2026-06-17T00:00:00Z"));
}

/// Verifies the active-goal continuation prompt keeps Codex's full completion and blocked audits.
/// 验证活跃目标续跑提示词保留 Codex 的完整完成审计与阻塞审计。
#[test]
fn goal_continuation_prompt_keeps_codex_audit_details() {
    let prompt = render_goal_continuation_context(&SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成目标模式"),
        status: SessionGoalStatus::Active,
        token_budget: Some(10_000),
        tokens_used: 1_234,
        usage_tokens: 1_234,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 56,
        work_time: None,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    });

    assert!(prompt.contains("<objective>\n完成目标模式\n</objective>"));
    assert!(prompt.contains("Token budget: 10000"));
    assert!(!prompt.contains("Tokens used:"));
    assert!(!prompt.contains("Tokens remaining:"));
    assert!(prompt.contains("Derive concrete requirements"));
    assert!(prompt.contains("The audit must prove completion"));
    assert!(prompt.contains("same blocking condition"));
    assert!(prompt.contains("original/user-triggered turn"));
    assert!(prompt.contains("truly at an impasse"));
    assert!(prompt.contains("call update_goal with status \"blocked\""));
}

/// Verifies prompt rendering keeps the durable goal identity in the execution binding.
/// 验证提示词渲染会在执行绑定中保留持久目标身份。
#[test]
fn goal_execution_context_binds_prompt_to_goal_identity() {
    let goal = SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-bound-to-run"),
        objective: String::from("验证目标身份绑定"),
        status: SessionGoalStatus::Active,
        token_budget: None,
        tokens_used: 0,
        usage_tokens: 0,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 0,
        work_time: None,
        created_at_ms: 1_000,
        updated_at_ms: 1_000,
    };

    let context = session_goal_execution_context(&goal).expect("active goal context");

    assert_eq!(context.goal_id, goal.goal_id);
    assert!(context.prompt_context.contains("验证目标身份绑定"));
}

/// Verifies a stopped goal keeps owning later physical requests from the same execution only.
/// 验证停止目标仅继续拥有同一次执行中的后续物理请求。
#[tokio::test]
async fn provider_request_goal_binding_survives_same_execution_stop() {
    // sessionId identifies the regular session whose live goal projection owns request binding.
    // sessionId 标识其实时目标投影拥有请求绑定的常规会话。
    let session_id = SessionId::new_v4();
    // runtime and handle provide the real V1 Goal authority used by Provider request binding.
    // runtime 与 handle 提供 Provider 请求绑定使用的真实 V1 Goal 权威。
    let (runtime, _handle, _backend, state) = registered_native_goal_runtime(session_id);
    let completed_goal = runtime
        .create_goal(
            session_id,
            String::from("验证停止后的最终答复仍记账"),
            Some(10_000),
        )
        .await
        .expect("provider-bound Goal should create");
    runtime
        .update_goal_status(session_id, NativeGoalTerminalStatus::Complete)
        .await
        .expect("provider-bound Goal should complete");
    assert_eq!(
        session_goal_id_for_provider_request(
            &state,
            session_id,
            Some(completed_goal.goal_id.as_str()),
        )
        .expect("same execution binding"),
        Some(completed_goal.goal_id.clone())
    );
    assert_eq!(
        session_goal_id_for_provider_request(&state, session_id, Some("older-goal"))
            .expect("stale execution binding"),
        None
    );
    let active_goal = runtime
        .create_goal(session_id, String::from("new active goal"), None)
        .await
        .expect("completed Goal should be replaced by a new active generation");
    assert_eq!(
        session_goal_id_for_provider_request(
            &state,
            session_id,
            Some(completed_goal.goal_id.as_str()),
        )
        .expect("new active goal binding"),
        Some(active_goal.goal_id)
    );
}


/// Verifies the authoritative V1 UserInput body remains unchanged while usage becomes a system event.
/// 验证权威 V1 UserInput 正文保持不变，同时用量变为系统事件。
#[test]
fn v1_user_input_event_stays_original_while_goal_usage_is_system_context() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let handle = register_test_session_v1_document(
        &mut service_state,
        session_id,
        SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Goal usage authority",
            1_700_000_000_000,
        ),
    );
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("goal-authority-root")
                    .expect("goal authority action id should be valid"),
                "真实用户正文",
                Vec::new(),
            ),
            1_700_000_000_001,
        )
        .expect("root input should be durable before usage persistence");

    crate::session_goal_lifecycle::persist_goal_turn_usage_context(
        handle.as_ref(),
        Some("<goal_usage_snapshot>\nTokens used: 123\n</goal_usage_snapshot>"),
    )
    .expect("usage system context should persist");

    let (turn_id, turn_closed, user_text, usage_contexts) = handle
        .read(|document| {
            let active_turn = document.active_turn().expect("the root turn should remain active");
            let user_text = document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::UserInput(input) => Some(input.text.clone()),
                    _ => None,
                })
                .expect("the authoritative user input event should remain present");
            let usage_contexts = document
                .events()
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::SystemContext(context)
                        if context.context_key == "goal_usage_snapshot" =>
                    {
                        Some((
                            event.turn_id,
                            context.context_key.clone(),
                            context.operation,
                            context.scope,
                            context.ttl_turns,
                            context.provider_role_hint,
                            context.content.clone(),
                        ))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            (
                active_turn.turn_id,
                active_turn.closed,
                user_text,
                usage_contexts,
            )
        })
        .expect("authoritative V1 document should remain readable");

    assert!(!turn_closed, "usage persistence requires an open active turn");
    assert_eq!(user_text, "真实用户正文");
    assert_eq!(usage_contexts.len(), 1);
    let usage_context = &usage_contexts[0];
    assert_eq!(usage_context.0, Some(turn_id));
    assert_eq!(usage_context.1, "goal_usage_snapshot");
    assert_eq!(usage_context.2, SystemContextOperation::Upsert);
    assert_eq!(usage_context.3, SystemContextScope::Turn);
    assert_eq!(usage_context.4, Some(1));
    assert_eq!(usage_context.5, Some(SystemRoleHint::HiddenUserShell));
    assert_eq!(
        usage_context.6,
        "<goal_usage_snapshot>\nTokens used: 123\n</goal_usage_snapshot>"
    );
}

/// Verifies same-turn usage snapshots are idempotent for repeats and append when changed.
/// 验证同回合重复用量快照幂等，变化用量只追加新系统上下文。
#[test]
fn goal_usage_system_context_is_idempotent_per_turn_and_appends_changes() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let handle = register_test_session_v1_document(
        &mut service_state,
        session_id,
        SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Goal usage append",
            1_700_000_000_000,
        ),
    );
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("goal-usage-append-root")
                    .expect("goal usage append action id should be valid"),
                "保持原样的用户输入",
                Vec::new(),
            ),
            1_700_000_000_001,
        )
        .expect("root input should be durable before usage persistence");

    crate::session_goal_lifecycle::persist_goal_turn_usage_context(handle.as_ref(), Some("usage-1"))
        .expect("first usage snapshot should persist");
    let first = handle
        .read(|document| {
            (
                document.memory_revision(),
                document.durable_revision(),
                document.events().len(),
            )
        })
        .expect("first usage state should be readable");

    crate::session_goal_lifecycle::persist_goal_turn_usage_context(handle.as_ref(), Some("usage-1"))
        .expect("repeated usage snapshot should be idempotent");
    let repeated = handle
        .read(|document| {
            (
                document.memory_revision(),
                document.durable_revision(),
                document.events().len(),
            )
        })
        .expect("repeated usage state should be readable");
    assert_eq!(repeated, first, "same-turn usage must not create a duplicate event");

    crate::session_goal_lifecycle::persist_goal_turn_usage_context(handle.as_ref(), Some("usage-2"))
        .expect("changed usage snapshot should append");
    let (user_text, turn_id, usage_contexts, revisions) = handle
        .read(|document| {
            let turn_id = document
                .active_turn()
                .expect("the root turn should remain active")
                .turn_id;
            let user_text = document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::UserInput(input) => Some(input.text.clone()),
                    _ => None,
                })
                .expect("the original user input event should remain present");
            let usage_contexts = document
                .events()
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::SystemContext(context)
                        if context.context_key == "goal_usage_snapshot" =>
                    {
                        Some((event.turn_id, context.content.clone()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            (
                user_text,
                turn_id,
                usage_contexts,
                (document.memory_revision(), document.durable_revision()),
            )
        })
        .expect("changed usage state should be readable");

    assert_eq!(user_text, "保持原样的用户输入");
    assert_eq!(
        usage_contexts,
        vec![
            (Some(turn_id), String::from("usage-1")),
            (Some(turn_id), String::from("usage-2")),
        ]
    );
    assert_eq!(usage_contexts.len(), 2);
    assert_eq!(revisions.0, revisions.1);
    assert_eq!(revisions.0, first.0 + 1);
}

/// Verifies a storage flush failure is returned explicitly and a retry can publish the pending event.
/// 验证存储刷新失败会显式返回，重试可以发布待持久化事件。
#[test]
fn goal_usage_flush_failure_is_explicit_and_retryable() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let handle = register_test_session_v1_document_with_scripted_flush_failures(
        &mut service_state,
        session_id,
        SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Goal usage failure",
            1_700_000_000_000,
        ),
        std::collections::BTreeSet::from([2_usize]),
    );
    handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("goal-usage-failure-root")
                    .expect("goal usage failure action id should be valid"),
                "失败后仍保持不变的用户输入",
                Vec::new(),
            ),
            1_700_000_000_001,
        )
        .expect("the first scripted flush should accept the root input");

    let error = crate::session_goal_lifecycle::persist_goal_turn_usage_context(
        handle.as_ref(),
        Some("usage-before-flush-failure"),
    )
    .expect_err("a scripted flush failure must not report success");
    assert!(error.contains("Failed to persist ordered Goal usage context"));
    let (memory_after_failure, durable_after_failure, usage_count_after_failure) = handle
        .read(|document| {
            (
                document.memory_revision(),
                document.durable_revision(),
                document
                    .events()
                    .iter()
                    .filter(|event| {
                        matches!(
                            &event.event,
                            EventBody::SystemContext(context)
                                if context.context_key == "goal_usage_snapshot"
                        )
                    })
                    .count(),
            )
        })
        .expect("failed usage state should remain readable");
    assert!(memory_after_failure > durable_after_failure);
    assert_eq!(usage_count_after_failure, 1);

    crate::session_goal_lifecycle::persist_goal_turn_usage_context(
        handle.as_ref(),
        Some("usage-before-flush-failure"),
    )
    .expect("retry should flush the pending unchanged event");
    let (memory_after_retry, durable_after_retry, usage_count_after_retry) = handle
        .read(|document| {
            (
                document.memory_revision(),
                document.durable_revision(),
                document
                    .events()
                    .iter()
                    .filter(|event| {
                        matches!(
                            &event.event,
                            EventBody::SystemContext(context)
                                if context.context_key == "goal_usage_snapshot"
                        )
                    })
                    .count(),
            )
        })
        .expect("retried usage state should remain readable");
    assert_eq!(memory_after_retry, durable_after_retry);
    assert_eq!(usage_count_after_retry, 1);
}

/// Verifies objective-updated steering mirrors Codex's superseding-objective prompt.
/// 验证目标更新引导会对齐 Codex 的新目标覆盖旧目标提示词。
#[test]
fn goal_objective_updated_prompt_supersedes_previous_goal() {
    let prompt = render_goal_objective_updated_context(&SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-1"),
        objective: String::from("完成修改后的目标"),
        status: SessionGoalStatus::Active,
        token_budget: Some(10_000),
        tokens_used: 1_234,
        usage_tokens: 1_234,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 56,
        work_time: None,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    });

    assert!(prompt.contains("edited by the user"));
    assert!(prompt.contains("supersedes any previous thread goal objective"));
    assert!(prompt.contains("<untrusted_objective>\n完成修改后的目标\n</untrusted_objective>"));
    assert!(prompt.contains("Tokens remaining: 8766"));
    assert!(
        prompt.contains("Do not call update_goal unless the updated goal is actually complete.")
    );
}

/// Verifies budget-limited goals render a stage wrap-up instead of a completion instruction.
/// 验证预算耗尽目标会渲染阶段收尾提示而不是完成指令。
#[test]
fn goal_budget_limited_prompt_waits_for_user_resume() {
    let prompt = render_goal_budget_limited_context(&SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-budget"),
        objective: String::from("完成预算测试"),
        status: SessionGoalStatus::BudgetLimited,
        token_budget: Some(2_000),
        tokens_used: 2_345,
        usage_tokens: 2_345,
        api_call_count: 1,
        tool_call_count: 2,
        input_tokens: 1_000,
        cached_input_tokens: 100,
        output_tokens: 1_445,
        reasoning_tokens: 0,
        total_tokens: 2_445,
        time_used_seconds: 60,
        work_time: None,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    });

    assert!(prompt.contains("stage wrap-up, not goal completion"));
    assert!(prompt.contains("Wait for the user to decide whether to continue"));
    assert!(prompt.contains("Do not continue the next stage automatically"));
    assert!(prompt.contains("Do not call update_goal unless the goal is actually complete."));
}

/// Verifies stopped budget-limited goals do not inject ordinary prompt goal context.
/// 验证已预算耗尽的停止目标不会注入普通提示词目标上下文。
#[test]
fn budget_limited_goal_is_not_regular_prompt_context() {
    let goal = SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-budget"),
        objective: String::from("完成预算测试"),
        status: SessionGoalStatus::BudgetLimited,
        token_budget: Some(2_000),
        tokens_used: 2_345,
        usage_tokens: 2_345,
        api_call_count: 0,
        tool_call_count: 0,
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        time_used_seconds: 60,
        work_time: None,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    };

    assert!(render_session_goal_context_for_prompt(&goal).is_none());
}
