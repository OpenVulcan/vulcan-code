//! Unit tests for the shared hook dispatcher.
//! 共享 hook 调度器的单元测试。

use crate::*;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;

/// Builds a registration for the test hook point.
/// 为测试 hook 点位构建注册项。
fn registration(id: &str, mode: HookMode) -> HookRegistration {
    HookRegistration::new(id, "conversation.turn.precheck", mode)
}

/// Builds a dispatch request for the test hook point.
/// 为测试 hook 点位构建调度请求。
fn request() -> HookDispatchRequest {
    HookDispatchRequest::new(
        HookContext::new("run-1", "conversation.turn.precheck"),
        HookPayload::from_serializable(json!({"value": 1})).expect("payload should serialize"),
    )
}

#[tokio::test]
async fn dispatcher_runs_hooks_by_priority_then_registration_order() {
    let order = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut registry = HookRegistry::new();

    for (id, priority) in [("late", 10), ("early", -1), ("middle", 10)] {
        let order = Arc::clone(&order);
        let mut hook = registration(id, HookMode::Observe);
        hook.priority = priority;
        registry
            .register(
                hook,
                Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                    let order = Arc::clone(&order);
                    async move {
                        order
                            .lock()
                            .expect("order lock should not be poisoned")
                            .push(invocation.registration.id);
                        Ok(HookResult::default())
                    }
                })),
            )
            .expect("registration should succeed");
    }

    HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect("dispatch should succeed");

    assert_eq!(
        *order.lock().expect("order lock should not be poisoned"),
        vec!["early", "late", "middle"]
    );
}

#[tokio::test]
async fn transform_hook_replaces_payload_for_later_handlers() {
    let mut registry = HookRegistry::new();
    registry
        .register(
            registration("replace", HookMode::Transform),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::with_payload(
                    HookPayload::from_serializable(json!({"value": 2}))
                        .expect("payload should serialize"),
                ))
            })),
        )
        .expect("registration should succeed");
    registry
        .register(
            registration("read", HookMode::Observe),
            Arc::new(FnHookHandler::new(
                |invocation: HookInvocation| async move {
                    assert_eq!(invocation.payload.value["value"], json!(2));
                    Ok(HookResult::default())
                },
            )),
        )
        .expect("registration should succeed");

    let report = HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect("dispatch should succeed");

    assert_eq!(report.payload.value["value"], json!(2));
    assert_eq!(report.completed_hooks, vec!["replace", "read"]);
}

#[tokio::test]
async fn dispatcher_collects_system_prompt_hook_blocks() {
    let mut registry = HookRegistry::new();
    registry
        .register(
            registration("profile", HookMode::Transform),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult {
                    system_prompt_blocks: vec![SystemPromptHookBlock {
                        tag: String::from("vmm_profile"),
                        description: String::from("Profile details"),
                        content: String::from("User prefers concise answers."),
                        source: Some(String::from("test")),
                        phase: SystemPromptHookPhase::FirstSession,
                    }],
                    ..HookResult::default()
                })
            })),
        )
        .expect("registration should succeed");

    let report = HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect("dispatch should succeed");

    assert_eq!(report.system_prompt_blocks.len(), 1);
    assert_eq!(report.system_prompt_blocks[0].tag, "vmm_profile");
    assert_eq!(
        report.system_prompt_blocks[0].phase,
        SystemPromptHookPhase::FirstSession
    );
}

#[tokio::test]
async fn decision_hook_blocks_later_handlers() {
    let mut registry = HookRegistry::new();
    registry
        .register(
            registration("block", HookMode::Decision),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::with_decision(HookDecision::Block))
            })),
        )
        .expect("registration should succeed");
    registry
        .register(
            registration("after", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("registration should succeed");

    let report = HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect("dispatch should succeed");

    assert_eq!(report.decision, HookDecision::Block);
    assert_eq!(report.completed_hooks, vec!["block"]);
}

#[tokio::test]
async fn effect_failure_can_continue_and_report_diagnostics() {
    let mut registry = HookRegistry::new();
    registry
        .register(
            registration("failing-effect", HookMode::Effect),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Err(HookError::handler("writeback failed"))
            })),
        )
        .expect("registration should succeed");
    registry
        .register(
            registration("after", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("registration should succeed");

    let report = HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect("dispatch should continue");

    assert_eq!(report.completed_hooks, vec!["after"]);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].hook_id, "failing-effect");
    assert!(report.diagnostics[0].message.contains("writeback failed"));
}

#[tokio::test]
async fn timeout_policy_records_queueable_failure() {
    let mut hook = registration("slow", HookMode::Effect);
    hook.timeout_ms = Some(1);
    hook.failure_policy = HookFailurePolicy::Queue;

    let mut registry = HookRegistry::new();
    registry
        .register(
            hook,
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                time::sleep(Duration::from_millis(30)).await;
                Ok(HookResult::default())
            })),
        )
        .expect("registration should succeed");

    let report = HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect("queue policy should continue");

    assert!(report.has_queueable_failure);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].message.contains("timed out"));
}

#[tokio::test]
async fn stop_policy_returns_dispatch_error() {
    let mut hook = registration("stopper", HookMode::Effect);
    hook.failure_policy = HookFailurePolicy::Stop;

    let mut registry = HookRegistry::new();
    registry
        .register(
            hook,
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Err(HookError::handler("fatal"))
            })),
        )
        .expect("registration should succeed");

    let error = HookDispatcher::new(registry)
        .dispatch(request())
        .await
        .expect_err("stop policy should fail dispatch");

    assert!(matches!(error, HookError::DispatchStopped(_)));
}

#[tokio::test]
async fn lifecycle_listener_observes_registration_and_dispatch_events() {
    let events = Arc::new(Mutex::new(Vec::<HookEventKind>::new()));
    let mut dispatcher = HookDispatcher::default();
    dispatcher.add_listener(Arc::new(FnHookEventListener::new({
        let events = Arc::clone(&events);
        move |event: HookEvent| {
            events
                .lock()
                .expect("events lock should not be poisoned")
                .push(event.kind);
        }
    })));

    dispatcher
        .register(
            registration("observe", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("registration should succeed");

    dispatcher
        .dispatch(request())
        .await
        .expect("dispatch should succeed");

    assert_eq!(
        *events.lock().expect("events lock should not be poisoned"),
        vec![
            HookEventKind::Registered,
            HookEventKind::DispatchStarted,
            HookEventKind::HandlerStarted,
            HookEventKind::HandlerCompleted,
            HookEventKind::DispatchCompleted,
        ]
    );
}

#[tokio::test]
async fn lifecycle_listener_observes_handler_failure_events() {
    let events = Arc::new(Mutex::new(Vec::<HookEvent>::new()));
    let mut dispatcher = HookDispatcher::default();
    dispatcher.add_listener(Arc::new(FnHookEventListener::new({
        let events = Arc::clone(&events);
        move |event: HookEvent| {
            events
                .lock()
                .expect("events lock should not be poisoned")
                .push(event);
        }
    })));

    dispatcher
        .register(
            registration("failing", HookMode::Effect),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Err(HookError::handler("listener failure sample"))
            })),
        )
        .expect("registration should succeed");

    dispatcher
        .dispatch(request())
        .await
        .expect("continue policy should keep dispatch successful");

    let events = events.lock().expect("events lock should not be poisoned");
    let failure = events
        .iter()
        .find(|event| event.kind == HookEventKind::HandlerFailed)
        .expect("failure event should be emitted");
    assert_eq!(failure.hook_id.as_deref(), Some("failing"));
    assert!(
        failure
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("listener failure sample")
    );
}

#[test]
fn point_descriptors_validate_allowed_hook_modes() {
    let mut registry = HookRegistry::new();
    registry
        .register_point(
            HookPointDescriptor::new(
                "conversation.turn.completed",
                [HookMode::Observe, HookMode::Effect],
                "completed turn",
            )
            .with_default_scope(HookScope::Session),
        )
        .expect("point descriptor should register");

    assert_eq!(registry.point_descriptors().len(), 1);
    assert!(
        registry
            .register(
                HookRegistration::new(
                    "bad-mode",
                    "conversation.turn.completed",
                    HookMode::Transform,
                ),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .is_err()
    );
}

#[test]
fn registry_rejects_duplicate_hook_ids() {
    let mut registry = HookRegistry::new();
    registry
        .register(
            registration("same-id", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("first registration should succeed");

    assert!(
        registry
            .register(
                HookRegistration::new("same-id", "conversation.turn.completed", HookMode::Observe,),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .is_err()
    );
}

#[test]
fn listener_registration_rejects_empty_ids() {
    let mut dispatcher = HookDispatcher::default();
    assert!(
        dispatcher
            .add_listener_with_id(
                " ",
                Arc::new(FnHookEventListener::new(|_event: HookEvent| {})),
            )
            .is_err()
    );
}

#[tokio::test]
async fn unregister_removes_handler_and_emits_lifecycle_event() {
    let events = Arc::new(Mutex::new(Vec::<HookEventKind>::new()));
    let mut dispatcher = HookDispatcher::default();
    dispatcher
        .add_listener_with_id(
            "audit",
            Arc::new(FnHookEventListener::new({
                let events = Arc::clone(&events);
                move |event: HookEvent| {
                    events
                        .lock()
                        .expect("events lock should not be poisoned")
                        .push(event.kind);
                }
            })),
        )
        .expect("listener id should be valid");
    dispatcher
        .register(
            registration("observe", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("registration should succeed");

    assert_eq!(dispatcher.unregister("observe", None), 1);
    let report = dispatcher
        .dispatch(request())
        .await
        .expect("dispatch should succeed without handlers");

    assert!(report.completed_hooks.is_empty());
    assert_eq!(
        dispatcher
            .registry()
            .registration_snapshots(Some("conversation.turn.precheck")),
        Vec::<HookRegistrationSnapshot>::new()
    );
    assert!(
        events
            .lock()
            .expect("events lock should not be poisoned")
            .contains(&HookEventKind::Unregistered)
    );
}

#[tokio::test]
async fn listener_can_be_removed_by_id() {
    let events = Arc::new(Mutex::new(Vec::<HookEventKind>::new()));
    let mut dispatcher = HookDispatcher::default();
    dispatcher
        .add_listener_with_id(
            "audit",
            Arc::new(FnHookEventListener::new({
                let events = Arc::clone(&events);
                move |event: HookEvent| {
                    events
                        .lock()
                        .expect("events lock should not be poisoned")
                        .push(event.kind);
                }
            })),
        )
        .expect("listener id should be valid");

    assert!(dispatcher.remove_listener("audit"));
    dispatcher
        .register(
            registration("observe", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("registration should succeed");

    assert!(
        events
            .lock()
            .expect("events lock should not be poisoned")
            .is_empty()
    );
}
