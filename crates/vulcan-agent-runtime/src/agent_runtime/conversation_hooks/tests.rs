use super::*;
use crate::conversation_protocol::{SessionConversationMessage, SessionConversationPayload};

#[test]
fn turn_completion_keeps_supplement_inside_timeline() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(42);
    let state = SessionConversationState::default();

    runtime.prepare_user_prompt_precheck(
        session_id,
        "entry-1",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "first",
        &state,
    );
    runtime.prepare_user_prompt_precheck(
        session_id,
        "entry-2",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserInterrupt,
        SystemPromptHookPhase::SubsequentSession,
        "supplement",
        &state,
    );

    let turn = runtime
        .active_turns
        .get(&session_id)
        .expect("turn should be active");
    assert_eq!(turn.user_content, "first");
    assert_eq!(turn.timeline.len(), 1);
    assert_eq!(turn.timeline[0].kind, "user_interrupt");
    assert_eq!(turn.timeline[0].content, "supplement");
}

/// Verifies independently reconstructed logical turns receive distinct collision-resistant identities.
/// 验证独立重建的逻辑回合会获得不同且抗碰撞的身份。
#[test]
fn reconstructed_turn_ids_are_unique_and_session_scoped() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(43);
    let state = SessionConversationState::default();

    let first = runtime.reconstruct_turn_from_state(session_id, &state);
    let second = runtime.reconstruct_turn_from_state(session_id, &state);
    let expected_prefix = format!("session-{session_id}-turn-");

    assert_ne!(first.turn_id, second.turn_id);
    assert!(first.turn_id.starts_with(&expected_prefix));
    assert!(second.turn_id.starts_with(&expected_prefix));
}

/// Verifies an explicit child execution dispatches tool hooks without creating memory lifecycle turns.
/// 验证显式子代理执行会派发工具 hook，但不会创建记忆生命周期回合。
#[test]
fn explicit_execution_tool_event_dispatches_without_creating_user_turn() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    let mut runtime = ConversationHookRuntime::new();
    let (sender, receiver) = mpsc::channel();
    let sender = Arc::new(sender);
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "child-tool-listener",
                HOOK_CONVERSATION_TOOL_STARTED,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                let sender = Arc::clone(&sender);
                async move {
                    let payload = invocation.payload.value;
                    let _ = sender.send((
                        payload
                            .get("session_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        payload
                            .get("turn_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    ));
                    Ok(HookResult::default())
                }
            })),
        )
        .expect("child tool hook registration should succeed");
    let session_id = SessionId::from_u128(147);

    runtime.record_tool_runtime_event_for_execution(
        session_id,
        "child-execution-7",
        &RuntimeEvent::new(RuntimeEventKind::ToolStarted, "shell_command", "started"),
    );

    assert!(!runtime.active_turns.contains_key(&session_id));
    assert_eq!(
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("child tool hook should run"),
        (session_id.to_string(), String::from("child-execution-7"))
    );
}

/// Verifies tool events remain hook notifications instead of completed-turn timeline rows.
/// 验证工具事件只保留为 hook 通知，不进入完成回合 timeline。
#[test]
fn tool_runtime_event_stays_out_of_completed_timeline() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(144);
    let state = SessionConversationState::default();

    // precheck_request owns the immutable turn correlation used by the tool event.
    // precheck_request 持有工具事件使用的不可变回合关联。
    let precheck_request = runtime.prepare_user_prompt_precheck(
        session_id,
        "entry-1",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "first",
        &state,
    );
    runtime.record_tool_runtime_event_for_execution(
        session_id,
        precheck_request.turn_id(),
        &RuntimeEvent::new(RuntimeEventKind::ToolFinished, "tool", "finished"),
    );

    let (_turn_id, request) =
        runtime.prepare_turn_completion_dispatch(session_id, None, "done", 1, &state);
    let timeline = request
        .payload
        .get("timeline")
        .and_then(Value::as_array)
        .expect("completion payload should expose a timeline array");
    assert!(timeline.is_empty());
}

/// Verifies unfinished subagent completion dispatch keeps the original user turn open.
/// 验证未完成的子代理完成调度会保留原始用户回合。
#[test]
fn awaiting_subagent_completion_keeps_active_turn() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(145);
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("m1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("spawn two subagents"),
                attachments: Vec::new(),
            },
        }],
        ..SessionConversationState::default()
    };
    runtime.prepare_user_prompt_precheck(
        session_id,
        "entry-1",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "spawn two subagents",
        &state,
    );

    let (awaiting_turn_id, awaiting_request) = runtime
        .prepare_turn_completion_dispatch_with_status(
            session_id,
            Some(String::from("project")),
            "children are still running",
            1,
            &state,
            ConversationTurnCompletionStatus::AwaitingSubagents,
            2,
        );

    assert!(runtime.has_active_turn(session_id));
    assert_eq!(
        runtime.active_turn_id(session_id),
        Some(awaiting_turn_id.as_str())
    );
    assert_eq!(
        awaiting_request
            .payload
            .get("completion_status")
            .and_then(Value::as_str),
        Some("awaiting_subagents")
    );
    assert_eq!(
        awaiting_request
            .payload
            .get("turn_complete")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        awaiting_request
            .payload
            .get("pending_subagent_count")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert!(
        awaiting_request
            .payload
            .get("turn_messages")
            .and_then(Value::as_array)
            .is_some_and(|messages| messages.len() == 1)
    );

    let (completed_turn_id, completed_request) = runtime.prepare_turn_completion_dispatch(
        session_id,
        Some(String::from("project")),
        "all children returned",
        2,
        &state,
    );

    assert_eq!(awaiting_turn_id, completed_turn_id);
    assert!(!runtime.has_active_turn(session_id));
    assert_eq!(runtime.active_turn_id(session_id), None);
    assert_eq!(
        completed_request
            .payload
            .get("completion_status")
            .and_then(Value::as_str),
        Some("completed")
    );
    assert_eq!(
        completed_request
            .payload
            .get("turn_complete")
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn completed_turn_falls_back_to_latest_user_from_conversation_state() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(43);
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("m1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("latest user"),
                attachments: Vec::new(),
            },
        }],
        ..SessionConversationState::default()
    };

    let turn = runtime.reconstruct_turn_from_state(session_id, &state);
    assert_eq!(turn.user_content, "latest user");
}

#[tokio::test]
async fn precheck_hook_injections_are_returned_to_send_pipeline() {
    use std::sync::Arc;
    use vulcan_context_injection::{InjectionInsert, InjectionKind};
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    let mut runtime = ConversationHookRuntime::new();
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "injector",
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult {
                    injections: vec![InjectionInsert::single(
                        InjectionKind::Transient,
                        "test_inject",
                        "test description",
                        "remember this",
                        Some(1),
                    )],
                    ..HookResult::default()
                })
            })),
        )
        .expect("hook registration should succeed");

    let outcome = runtime
        .prepare_user_prompt_precheck(
            SessionId::from_u128(44),
            "entry-1",
            None,
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "hello",
            &SessionConversationState::default(),
        )
        .run()
        .await;
    let report = outcome.report.expect("precheck hook should complete");

    assert_eq!(report.injections.len(), 1);
    assert_eq!(report.injections[0].tag, "test_inject");
}

#[tokio::test]
async fn precheck_hook_timeout_records_failure_without_hanging() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    const TEST_HOOK_TIMEOUT_MS: u64 = 250;

    let mut runtime = ConversationHookRuntime::new();
    let mut registration =
        HookRegistration::new("slow", HOOK_CONVERSATION_TURN_PRECHECK, HookMode::Transform);
    registration.timeout_ms = Some(TEST_HOOK_TIMEOUT_MS);
    runtime
        .dispatcher_mut()
        .register(
            registration,
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                tokio::time::sleep(Duration::from_millis(TEST_HOOK_TIMEOUT_MS + 500)).await;
                Ok(HookResult::default())
            })),
        )
        .expect("hook registration should succeed");

    let started_at = Instant::now();
    let outcome = runtime
        .prepare_user_prompt_precheck(
            SessionId::from_u128(45),
            "entry-1",
            None,
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "hello",
            &SessionConversationState::default(),
        )
        .run()
        .await;

    let report = outcome
        .report
        .expect("registration-level timeout should still return a dispatch report");
    assert!(report.injections.is_empty());
    assert!(!report.failures.is_empty());
    assert!(started_at.elapsed() < Duration::from_millis(TEST_HOOK_TIMEOUT_MS + 1_500));
}

/// Verifies completed-turn effect Hooks execute successfully through the private worker runtime.
/// 验证完成回合 effect Hook 可通过私有工作线程运行时成功执行。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if the asynchronous completion path cannot run an effect Hook.
/// 此测试无返回值；若异步完成路径无法执行 effect Hook 则失败。
#[tokio::test]
async fn effect_hook_dispatch_runs_on_private_worker_runtime() {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    let mut runtime = ConversationHookRuntime::new();
    let (sender, receiver) = mpsc::channel();
    let sender = Arc::new(sender);
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "effect-listener",
                HOOK_CONVERSATION_TURN_COMPLETED,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                let sender = Arc::clone(&sender);
                async move {
                    let assistant_content = invocation
                        .payload
                        .value
                        .get("assistant_content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let _ = sender.send(assistant_content);
                    Ok(HookResult::default())
                }
            })),
        )
        .expect("hook registration should succeed");

    let session_id = SessionId::from_u128(46);
    runtime.prepare_user_prompt_precheck(
        session_id,
        "entry-1",
        None,
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "hello",
        &SessionConversationState::default(),
    );
    let (_, request) = runtime.prepare_turn_completion_dispatch(
        session_id,
        None,
        "done",
        1,
        &SessionConversationState::default(),
    );
    let outcome = request.run().await;

    assert!(
        outcome.report.is_ok(),
        "effect hook dispatch should succeed"
    );

    assert_eq!(
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("effect hook should run"),
        "done"
    );
}

/// Verifies completed-turn Stop failures retain a dispatcher-stage diagnostic.
/// 验证完成回合 Stop 失败保留调度器阶段诊断。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if dispatcher errors collapse into a missing-report error.
/// 此测试无返回值；若调度器错误退化为缺失报告错误则失败。
#[tokio::test]
async fn completion_hook_dispatch_preserves_dispatcher_failure_stage() {
    use std::sync::Arc;
    use vulcan_hooks::{
        FnHookHandler, HookFailurePolicy, HookInvocation, HookMode, HookRegistration,
    };

    let mut runtime = ConversationHookRuntime::new();
    // registration models a handler whose declared Stop policy must surface as a dispatch error.
    // registration 模拟声明 Stop 策略且必须显式产生调度错误的处理器。
    let mut registration = HookRegistration::new(
        "completion-stopper",
        HOOK_CONVERSATION_TURN_COMPLETED,
        HookMode::Effect,
    );
    registration.failure_policy = HookFailurePolicy::Stop;
    runtime
        .dispatcher_mut()
        .register(
            registration,
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Err(HookError::handler("completion handler rejected"))
            })),
        )
        .expect("completion hook registration should succeed");

    let session_id = SessionId::from_u128(146);
    runtime.prepare_user_prompt_precheck(
        session_id,
        "entry-1",
        None,
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "hello",
        &SessionConversationState::default(),
    );
    let (_, request) = runtime.prepare_turn_completion_dispatch(
        session_id,
        None,
        "done",
        1,
        &SessionConversationState::default(),
    );
    let outcome = request.run().await;
    let error = outcome
        .report
        .expect_err("Stop-policy completion hook should fail dispatch");

    assert!(error.starts_with("completed-turn hook dispatch failed:"));
    assert!(error.contains("hook dispatch stopped after 1 failure(s)"));
    assert!(!error.contains("produced no report"));
}
/// Verifies asynchronous effect Hook diagnostics preserve continued and stopped failure identities.
/// 验证异步 effect Hook 诊断保留继续执行与停止失败的身份信息。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if correlation or failure-policy formatting regresses.
/// 此测试无返回值；关联信息或失败策略格式退化时测试失败。
#[test]
fn async_hook_failure_diagnostics_preserve_dispatch_context() {
    // failure models one Continue-policy handler failure returned inside a successful report.
    // failure 模拟成功报告内返回的单个 Continue 策略处理器失败。
    let failure = HookFailure {
        hook_id: String::from("hook-28"),
        policy: HookFailurePolicy::Continue,
        message: String::from("handler rejected"),
    };
    // handler_diagnostic captures the exact continued-handler diagnostic contract.
    // handler_diagnostic 捕获继续执行处理器的精确诊断契约。
    let handler_diagnostic =
        async_hook_handler_failure_diagnostic("conversation.user.event", "run-28", &failure);
    assert!(handler_diagnostic.contains("hook_id=hook-28"));
    assert!(handler_diagnostic.contains("policy=Continue"));
    assert!(handler_diagnostic.contains("run_id=run-28"));

    // stopped_failure models the exact handler failure that terminates a Stop-policy dispatch.
    // stopped_failure 模拟终结停止策略调度的精确处理器失败。
    let stopped_failure = HookFailure {
        hook_id: String::from("hook-stop-28"),
        policy: HookFailurePolicy::Stop,
        message: String::from("stop handler rejected"),
    };
    // stopped_error models the top-level error returned by a Stop-policy dispatch.
    // stopped_error 模拟停止策略调度返回的顶层错误。
    let stopped_error = HookError::dispatch_stopped(vec![stopped_failure]);
    // stopped_diagnostics capture nested handler identity before the top-level dispatch summary.
    // stopped_diagnostics 在顶层调度摘要前捕获嵌套处理器身份。
    let stopped_diagnostics =
        async_hook_dispatch_error_diagnostics("conversation.user.event", "run-28", &stopped_error);
    assert_eq!(stopped_diagnostics.len(), 2);
    assert!(stopped_diagnostics[0].contains("hook_id=hook-stop-28"));
    assert!(stopped_diagnostics[0].contains("policy=Stop"));
    assert!(stopped_diagnostics[0].contains("error=stop handler rejected"));
    assert!(stopped_diagnostics[1].contains("hook dispatch stopped after 1 failure(s)"));
    assert!(stopped_diagnostics[1].contains("point=conversation.user.event"));
}
