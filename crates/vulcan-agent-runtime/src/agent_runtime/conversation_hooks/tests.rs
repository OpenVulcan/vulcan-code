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

/// Verifies orphan tool events do not synthesize empty user turns.
/// 验证孤立工具事件不会合成空用户回合。
#[test]
fn orphan_tool_runtime_event_does_not_create_user_turn() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(143);

    runtime.record_tool_runtime_event(
        session_id,
        &RuntimeEvent::new(RuntimeEventKind::ToolStarted, "tool", "started"),
    );

    assert!(!runtime.active_turns.contains_key(&session_id));
}

/// Verifies tool events remain hook notifications instead of completed-turn timeline rows.
/// 验证工具事件只保留为 hook 通知，不进入完成回合 timeline。
#[test]
fn tool_runtime_event_stays_out_of_completed_timeline() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(144);
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
    runtime.record_tool_runtime_event(
        session_id,
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

#[test]
fn effect_hook_dispatch_runs_without_ambient_tokio_runtime() {
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
    let _ = runtime.complete_turn(
        session_id,
        None,
        "done",
        1,
        &SessionConversationState::default(),
    );

    assert_eq!(
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("effect hook should run"),
        "done"
    );
}
