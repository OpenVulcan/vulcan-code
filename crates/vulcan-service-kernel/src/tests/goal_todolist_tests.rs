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

/// Verifies pre-existing goals prefix the current prompt while AI-created tool results prevent reinsertion.
/// 验证预先存在的目标会前置当前提示词，而 AI 创建工具结果会阻止重复插入。
#[test]
fn goal_context_attaches_only_to_the_exact_creation_boundary() {
    let goal = SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-delivery-boundary"),
        objective: String::from("验证目标投递边界"),
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
    let goal_context = session_goal_execution_context(&goal).expect("active goal context");
    let empty_history =
        vulcan_agent_runtime::conversation_protocol::SessionConversationState::default();

    let user_created_prompt = prepend_goal_context_to_current_prompt(
        String::from("当前用户提示词"),
        &empty_history,
        Some(&goal_context),
    );

    assert!(user_created_prompt.starts_with("<thread_goal_context>"));
    assert!(user_created_prompt.ends_with("当前用户提示词"));

    let mut ai_created_history =
        vulcan_agent_runtime::conversation_protocol::SessionConversationState::default();
    ai_created_history
        .messages
        .push(SessionConversationMessage {
            id: String::from("create-goal-result"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("create-goal-call")),
                text: goal_context.prompt_context.clone(),
                is_error: false,
            },
        });
    let next_prompt = prepend_goal_context_to_current_prompt(
        String::from("下一轮提示词"),
        &ai_created_history,
        Some(&goal_context),
    );

    assert_eq!(next_prompt, "下一轮提示词");
}

/// Verifies Goal delivery rewrites the exact canonical current user message instead of appending a duplicate.
/// 验证目标投递会改写精确的当前规范用户消息，而不是追加重复消息。
#[test]
fn goal_context_replaces_the_exact_canonical_current_user_prompt() {
    // conversationState models the canonical row already created by the shared prompt reducer.
    // conversationState 模拟共享提示词归约器已经创建的规范行。
    let mut conversation_state =
        vulcan_agent_runtime::conversation_protocol::SessionConversationState::default();
    conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("current-user-message"),
            payload: SessionConversationPayload::UserText {
                text: String::from("原始用户输入"),
                attachments: Vec::new(),
            },
        });
    // providerPrompt is the exact append-only text that the Provider must observe.
    // providerPrompt 是 Provider 必须观察到的精确追加式文本。
    let provider_prompt = String::from(
        "<thread_goal_context>\n<goal_version>goal-current</goal_version>\n\
         </thread_goal_context>\n\n原始用户输入\n\n\
         <goal_usage_snapshot>\n- Tokens used: 1234\n</goal_usage_snapshot>",
    );

    replace_canonical_current_user_prompt(
        &mut conversation_state,
        "current-user-message",
        &provider_prompt,
    )
    .expect("exact canonical user prompt should be replaceable");

    assert_eq!(conversation_state.messages.len(), 1);
    let SessionConversationPayload::UserText { text, .. } = &conversation_state.messages[0].payload
    else {
        panic!("current canonical message should remain user text");
    };
    assert_eq!(text, &provider_prompt);
}

/// Verifies mailbox turns bind Goal guidance and changing usage to concrete agent-message payloads.
/// 验证 mailbox 回合会把目标引导与变化用量绑定到具体代理消息载荷。
#[test]
fn mailbox_goal_context_attaches_to_concrete_agent_message_payloads() {
    // goal provides one active version whose marker is initially absent from history.
    // goal 提供一个其标记最初不在历史中的活动版本。
    let goal = SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-mailbox-delivery"),
        objective: String::from("验证 mailbox 目标投递"),
        status: SessionGoalStatus::Active,
        token_budget: Some(20_000),
        tokens_used: 1_234,
        usage_tokens: 1_234,
        api_call_count: 2,
        tool_call_count: 1,
        input_tokens: 1_000,
        cached_input_tokens: 8_000,
        output_tokens: 234,
        reasoning_tokens: 100,
        total_tokens: 9_234,
        time_used_seconds: 10,
        work_time: None,
        created_at_ms: 1_000,
        updated_at_ms: 2_000,
    };
    // goalContext and usageContext are captured from the same immutable summary.
    // goalContext 与 usageContext 从同一份不可变摘要中捕获。
    let goal_context = session_goal_execution_context(&goal).expect("active goal context");
    let usage_context = render_goal_turn_usage_context(&goal).expect("non-zero goal usage");
    // messages models the concrete child result that starts the mailbox turn.
    // messages 模拟启动 mailbox 回合的具体子任务结果。
    let mut messages = vec![
        vulcan_agent_runtime::agent_runtime::execution_models::ExecutionProviderVisibleAgentMessage {
            delivery_id: String::from("delivery-1"),
            message_type: String::from("completed"),
            task_name: String::from("reviewer"),
            sender: String::from("reviewer"),
            recipient: String::from("parent"),
            payload: String::from("子任务结果"),
            trigger_turn: true,
            timestamp_ms: 1_000,
            metadata: None,
        },
    ];
    let empty_history =
        vulcan_agent_runtime::conversation_protocol::SessionConversationState::default();

    attach_goal_context_to_provider_visible_agent_messages(
        &mut messages,
        &empty_history,
        Some(&goal_context),
        Some(&usage_context),
    );

    assert!(messages[0].payload.starts_with("<thread_goal_context>"));
    assert!(messages[0].payload.contains("子任务结果"));
    assert!(messages[0].payload.ends_with("</goal_usage_snapshot>"));

    // deliveredHistory proves later mailbox batches do not duplicate stable guidance.
    // deliveredHistory 证明后续 mailbox 批次不会重复稳定引导。
    let delivered_history = vulcan_agent_runtime::conversation_protocol::SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("delivered-mailbox-message"),
            payload: SessionConversationPayload::AgentMessage {
                message_type: String::from("completed"),
                task_name: String::from("reviewer"),
                sender: String::from("reviewer"),
                recipient: String::from("parent"),
                payload: messages[0].payload.clone(),
                trigger_turn: true,
                metadata: None,
            },
        }],
        ..Default::default()
    };
    let mut later_messages = vec![
        vulcan_agent_runtime::agent_runtime::execution_models::ExecutionProviderVisibleAgentMessage {
            delivery_id: String::from("delivery-2"),
            message_type: String::from("completed"),
            task_name: String::from("reviewer-2"),
            sender: String::from("reviewer-2"),
            recipient: String::from("parent"),
            payload: String::from("后续结果"),
            trigger_turn: true,
            timestamp_ms: 2_000,
            metadata: None,
        },
    ];

    attach_goal_context_to_provider_visible_agent_messages(
        &mut later_messages,
        &delivered_history,
        Some(&goal_context),
        Some(&usage_context),
    );

    assert!(!later_messages[0].payload.contains("<thread_goal_context>"));
    assert!(later_messages[0].payload.starts_with("后续结果"));
    assert!(
        later_messages[0]
            .payload
            .ends_with("</goal_usage_snapshot>")
    );
}

/// Verifies cumulative usage is appended per turn and omitted before any usage exists.
/// 验证累计用量按回合追加，并在尚无用量时完全省略。
#[test]
fn goal_turn_usage_is_append_only_and_omits_zero_usage() {
    let mut goal = SessionGoalSummary {
        usage_known: true,
        session_id: SessionId::new_v4().to_string(),
        goal_id: String::from("goal-usage"),
        objective: String::from("验证追加式用量"),
        status: SessionGoalStatus::Active,
        token_budget: Some(10_000),
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
    let stable_context_before_usage =
        render_session_goal_context_for_prompt(&goal).expect("active goal context");

    assert!(render_goal_turn_usage_context(&goal).is_none());
    assert_eq!(
        append_goal_turn_usage_context(String::from("用户输入"), None),
        "用户输入"
    );

    goal.tokens_used = 1_234;
    goal.usage_tokens = 1_234;
    let stable_context_after_usage =
        render_session_goal_context_for_prompt(&goal).expect("active goal context");
    let usage_context = render_goal_turn_usage_context(&goal).expect("goal usage context");
    let request_prompt =
        append_goal_turn_usage_context(String::from("继续执行"), Some(&usage_context));

    assert_eq!(stable_context_before_usage, stable_context_after_usage);
    assert!(usage_context.contains("Tokens used: 1234"));
    assert!(usage_context.contains("Tokens remaining: 8766"));
    assert!(request_prompt.starts_with("继续执行"));
    assert!(request_prompt.ends_with("</goal_usage_snapshot>"));
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
