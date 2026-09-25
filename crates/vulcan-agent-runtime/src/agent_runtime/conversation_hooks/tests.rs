use super::*;

/// Preparation remains inert until its original publication is accepted, even across cloned requests.
/// 即使请求被克隆，准备也保持不执行，直到原发布被接纳。
/// Uses a real asynchronous effect handler to verify identity and once-only dispatch.
/// 使用真实异步效果处理器验证身份及仅一次派发。
#[test]
fn deferred_user_observation_requires_publication_and_dispatches_once() {
    use std::sync::{Arc, mpsc};
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};
    // Capture actual observer invocations rather than inferring dispatch from a request field.
    // 捕获实际观察器调用，而不是根据请求字段推断派发。
    let (sender, receiver) = mpsc::channel();
    let mut runtime = ConversationHookRuntime::new();
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "deferred-user-test",
                HOOK_CONVERSATION_USER_EVENT,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                let sender = sender.clone();
                async move {
                    sender.send(invocation.payload.value).unwrap();
                    Ok(HookResult::default())
                }
            })),
        )
        .unwrap();
    // This is the same pure preparation used by the shared host prompt reducer.
    // 此处使用共享宿主提示归约器采用的同一纯准备入口。
    let prepared = runtime.prepare_user_prompt_hook_requests(
        SessionId::from_u128(0x6d01),
        "original",
        None,
        "D:/deferred-user-test".into(),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "Original task",
        &Default::default(),
        false,
    );
    assert!(matches!(
        receiver.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let observation = prepared.deferred_user_event.unwrap();
    assert!(
        observation
            .dispatch_after_publication("foreign-hook")
            .is_err()
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let duplicate = observation.clone();
    assert!(
        observation
            .dispatch_after_publication(&prepared.turn_id)
            .unwrap()
    );
    assert!(
        !duplicate
            .dispatch_after_publication(&prepared.turn_id)
            .unwrap()
    );
    let payload = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(payload["turn_id"], prepared.turn_id);
    assert_eq!(payload["user_content"], "Original task");
    assert!(matches!(
        receiver.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
}
use crate::conversation_protocol::{SessionConversationMessage, SessionConversationPayload};

/// Canonical batches deduplicate mixed queued/direct inputs and reject late conflicts atomically.
/// 规范批次对混合排队和直接输入去重，并原子拒绝后续冲突。
#[test]
fn canonical_in_turn_hook_batch_uses_input_identity_and_preserves_original_turn() {
    use vulcan_session_core::v1::{ClientActionId, PendingInputId, Seq};
    // The actual root and queue paths establish the original owner and raw queue observation.
    // 实际根和队列路径建立原所有者及原始队列观察。
    let session_id = SessionId::from_u128(155);
    let mut runtime = ConversationHookRuntime::new();
    let conversation = SessionConversationState::default();
    let original = runtime.prepare_user_prompt_hook_requests(
        session_id,
        "root",
        None,
        "D:/test".into(),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "Original task",
        &conversation,
        true,
    );
    runtime.record_queued_user_prompt(
        session_id,
        "queued",
        None,
        "D:/test".into(),
        PendingSessionPromptSource::UserFollowup,
        "@inject 2 queued-rule\nQueued work",
    );
    let inputs = vec![
        CanonicalInTurnHookInput {
            client_action_id: ClientActionId::try_new("queued").unwrap(),
            input_id: PendingInputId::try_new(3).unwrap(),
            input_seq: Seq::try_new(10).unwrap(),
            text: "@inject 2 queued-rule\nQueued work".into(),
        },
        CanonicalInTurnHookInput {
            client_action_id: ClientActionId::try_new("direct").unwrap(),
            input_id: PendingInputId::try_new(4).unwrap(),
            input_seq: Seq::try_new(11).unwrap(),
            text: "@inject 2 direct-rule\nDirect work".into(),
        },
    ];
    // Replacing the batch adapter ID does not change the individual input receipts.
    // 替换批次适配标识不会改变逐条输入回执。
    for batch in ["webui-v1-pending-in-turn-1-9", "retry-adapter"] {
        let prepared = runtime
            .prepare_canonical_in_turn_hook_requests(
                session_id,
                batch,
                None,
                "D:/test".into(),
                PendingSessionPromptSource::UserInterrupt,
                SystemPromptHookPhase::SubsequentSession,
                "Queued work\n\nDirect work",
                &conversation,
                true,
                &inputs,
            )
            .unwrap();
        assert_eq!(prepared.turn_id, original.turn_id);
        assert_eq!(
            prepared.precheck.skip().report.unwrap().payload.value["canonical_batch_timeline_prepared"],
            true
        );
    }
    let turn = &runtime.active_turns[&session_id];
    assert_eq!(turn.user_content, "Original task");
    assert_eq!(turn.timeline.len(), 2);
    assert_eq!(
        turn.timeline[0].content,
        "@inject 2 queued-rule\nQueued work"
    );
    assert_eq!(turn.timeline[1].content, "Direct work");
    assert_eq!(turn.timeline[1].source_id.as_deref(), Some("direct"));
    assert_eq!(turn.canonical_input_receipts.len(), 2);
    // A new item followed by changed coordinates must not leak that new item or readiness changes.
    // 新条目之后出现坐标变化时，不能泄漏该新条目或就绪状态变化。
    let conflicted = vec![
        CanonicalInTurnHookInput {
            client_action_id: ClientActionId::try_new("new").unwrap(),
            input_id: PendingInputId::try_new(5).unwrap(),
            input_seq: Seq::try_new(12).unwrap(),
            text: "new".into(),
        },
        CanonicalInTurnHookInput {
            input_seq: Seq::try_new(13).unwrap(),
            ..inputs[1].clone()
        },
    ];
    assert!(
        runtime
            .prepare_canonical_in_turn_hook_requests(
                session_id,
                "bad-batch",
                None,
                "D:/test".into(),
                PendingSessionPromptSource::UserInterrupt,
                SystemPromptHookPhase::SubsequentSession,
                "new\n\nDirect work",
                &conversation,
                false,
                &conflicted
            )
            .is_err()
    );
    assert_eq!(runtime.active_turns[&session_id].timeline.len(), 2);
    assert_eq!(
        runtime.active_turns[&session_id]
            .canonical_input_receipts
            .len(),
        2
    );
    assert!(runtime.active_turns[&session_id].vmm_runtime_ready);
    // A missing business owner must never be synthesized from the merged batch body.
    // 绝不能从合并批次正文合成缺失的业务所有者。
    let mut cold = ConversationHookRuntime::new();
    assert!(
        cold.prepare_canonical_in_turn_hook_requests(
            session_id,
            "cold",
            None,
            "D:/test".into(),
            PendingSessionPromptSource::UserInterrupt,
            SystemPromptHookPhase::SubsequentSession,
            "Queued work\n\nDirect work",
            &conversation,
            true,
            &inputs
        )
        .is_err()
    );
    assert!(!cold.has_active_turn(session_id));
}

/// Restores mixed input history and deduplication before continuation without replaying a user event.
/// 在续跑前恢复混合输入历史及去重，不重放用户事件。
#[test]
fn business_hook_state_restores_ordered_queue_observations_atomically() {
    use vulcan_session_core::v1::business_hook::{
        BusinessHookBinding, BusinessHookInputKind, BusinessHookMetadata, BusinessHookObservedInput,
    };
    use vulcan_session_core::v1::{ClientActionId, PendingInputId, Seq, TurnId};
    // The host-proven state carries original business ownership and three interleaved admissions.
    // 宿主已证明状态携带原业务归属及三条交错准入。
    let session_id = SessionId::from_u128(154);
    let turn_id = TurnId::try_new(1).unwrap();
    let state = BusinessHookMetadata {
        version: 1,
        binding: Some(BusinessHookBinding {
            session_id: session_id.to_string(),
            segment_generation: 1,
            turn_id,
            start_seq: Seq::try_new(1).unwrap(),
            hook_turn_id: "original-hook".into(),
            user_content: "Original work".into(),
            vmm_runtime_ready: false,
        }),
        observed_inputs: vec![
            BusinessHookObservedInput {
                turn_id,
                client_action_id: ClientActionId::try_new("queue-a").unwrap(),
                input_id: PendingInputId::try_new(2).unwrap(),
                observed_revision: 2,
                kind: BusinessHookInputKind::Queued,
                content: " first ".into(),
            },
            BusinessHookObservedInput {
                turn_id,
                client_action_id: ClientActionId::try_new("direct").unwrap(),
                input_id: PendingInputId::try_new(3).unwrap(),
                observed_revision: 3,
                kind: BusinessHookInputKind::Interrupt,
                content: "@inject 2 rule\nDirect direction".into(),
            },
            BusinessHookObservedInput {
                turn_id,
                client_action_id: ClientActionId::try_new("queue-b").unwrap(),
                input_id: PendingInputId::try_new(4).unwrap(),
                observed_revision: 4,
                kind: BusinessHookInputKind::Queued,
                content: "second".into(),
            },
        ],
    };
    let mut runtime = ConversationHookRuntime::new();
    assert!(runtime.restore_business_hook_state(&state).unwrap());
    assert!(!runtime.restore_business_hook_state(&state).unwrap());
    assert_eq!(
        runtime.active_turns[&session_id]
            .timeline
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "Direct direction", "second"]
    );
    // A queued prompt consumed later must not be appended to the restored timeline for a second time.
    // 稍后消费排队提示时不得向恢复后的时间线二次追加。
    runtime.prepare_user_prompt_hook_requests(
        session_id,
        "queue-a",
        None,
        "D:/test".into(),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::SubsequentSession,
        "first",
        &SessionConversationState::default(),
        false,
    );
    assert_eq!(runtime.active_turns[&session_id].timeline.len(), 3);
    // Exact preparation can notify the restored direct input once without appending another timeline row.
    // 精确准备可以通知恢复后的直接输入一次，无须追加另一个时间线条目。
    let direct = CanonicalInTurnHookInput {
        client_action_id: ClientActionId::try_new("direct").unwrap(),
        input_id: PendingInputId::try_new(3).unwrap(),
        input_seq: Seq::try_new(20).unwrap(),
        text: "@inject 2 rule\nDirect direction".into(),
    };
    for batch in ["direct-batch", "direct-retry"] {
        runtime
            .prepare_canonical_in_turn_hook_requests(
                session_id,
                batch,
                None,
                "D:/test".into(),
                PendingSessionPromptSource::UserInterrupt,
                SystemPromptHookPhase::SubsequentSession,
                "Direct direction",
                &SessionConversationState::default(),
                false,
                std::slice::from_ref(&direct),
            )
            .unwrap();
    }
    assert_eq!(runtime.active_turns[&session_id].timeline.len(), 3);
    assert_eq!(
        runtime.active_turns[&session_id]
            .canonical_input_receipts
            .len(),
        1
    );
    let mut changed_identity = state.clone();
    changed_identity.observed_inputs[1].kind = BusinessHookInputKind::Queued;
    assert!(
        runtime
            .restore_business_hook_state(&changed_identity)
            .is_err()
    );
    // Conflicting canonical order cannot change the retained timeline or its deduplication set.
    // 冲突规范顺序不能改变保留时间线或其去重集合。
    let mut conflict = state.clone();
    conflict.observed_inputs.insert(
        0,
        BusinessHookObservedInput {
            turn_id,
            client_action_id: ClientActionId::try_new("queue-c").unwrap(),
            input_id: PendingInputId::try_new(5).unwrap(),
            observed_revision: 5,
            kind: BusinessHookInputKind::Queued,
            content: "third".into(),
        },
    );
    assert!(runtime.restore_business_hook_state(&conflict).is_err());
    assert_eq!(runtime.active_turns[&session_id].timeline.len(), 3);
    assert_eq!(
        runtime.active_turns[&session_id]
            .recorded_prompt_entry_ids
            .len(),
        2
    );
}

/// Cold restoration retains the original correlation and never replaces a live timeline on retry.
/// 冷恢复保留原关联，重试时绝不替换活动时间线。
#[test]
fn business_hook_binding_restores_original_owner_without_replaying_input() {
    // Capture the actual Hook created by production admission, not a fabricated correlation ID.
    // 捕获生产准入实际创建的 Hook，不伪造关联标识。
    let session_id = SessionId::from_u128(153);
    let mut original = ConversationHookRuntime::new();
    let prepared = original.prepare_user_prompt_hook_requests(
        session_id,
        "root",
        None,
        "D:/workspace".into(),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "Original task",
        &SessionConversationState::default(),
        false,
    );
    let binding = original
        .capture_business_hook_binding(
            session_id,
            1,
            vulcan_session_core::v1::TurnId::try_new(1).unwrap(),
            vulcan_session_core::v1::Seq::try_new(1).unwrap(),
        )
        .unwrap();
    assert_eq!(binding.hook_turn_id, prepared.turn_id);
    // A separate cold runtime must restore that identity without inferring any recent User input.
    // 独立冷运行时必须恢复该身份，不推断任何最近 User 输入。
    let mut restored = ConversationHookRuntime::new();
    assert!(restored.restore_business_hook_binding(&binding).unwrap());
    assert_eq!(
        restored.active_turn_id(session_id),
        Some(prepared.turn_id.as_str())
    );
    assert_eq!(
        restored.active_turns[&session_id].user_content,
        "Original task"
    );
    assert!(!restored.active_turns[&session_id].vmm_runtime_ready);
    assert!(restored.active_turns[&session_id].timeline.is_empty());
    // Later steering may update readiness and timeline; retrying the original seed preserves both.
    // 后续引导可以更新就绪状态及时间线；重试原种子保留二者。
    restored.prepare_user_prompt_hook_requests(
        session_id,
        "steer",
        None,
        "D:/workspace".into(),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::SubsequentSession,
        "Change direction",
        &SessionConversationState::default(),
        true,
    );
    assert!(!restored.restore_business_hook_binding(&binding).unwrap());
    assert_eq!(restored.active_turns[&session_id].timeline.len(), 1);
    assert!(restored.active_turns[&session_id].vmm_runtime_ready);
    // A conflicting owner is an explicit error and cannot consume the retained timeline.
    // 冲突所有者是明确错误，不能消费保留时间线。
    let mut conflict = binding;
    conflict.hook_turn_id = "different-owner".into();
    assert!(restored.restore_business_hook_binding(&conflict).is_err());
    assert_eq!(
        restored.active_turn_id(session_id),
        Some(prepared.turn_id.as_str())
    );
    assert_eq!(restored.active_turns[&session_id].timeline.len(), 1);
}

/// Internal collaboration continuation preserves the business identity and user timeline verbatim.
/// 内部协作续跑原样保留业务身份和用户时间线。
#[test]
fn agent_continuation_keeps_original_hook_timeline() {
    // All internal continuation adapters retain the actual root and suppress synthetic user observations.
    // 全部内部续跑适配器保留实际根，并抑制合成用户观察。
    for source in [
        PendingSessionPromptSource::AgentContinuation,
        PendingSessionPromptSource::GoalContinuation,
        PendingSessionPromptSource::SubagentReturnContinuation,
    ] {
        // Prepare through the same runtime entry used by shared prompt admission.
        // 通过共享提示准入使用的同一运行时入口准备。
        let session_id = SessionId::from_u128(152);
        let mut runtime = ConversationHookRuntime::new();
        let state = SessionConversationState::default();
        let original = runtime.prepare_user_prompt_hook_requests(
            session_id,
            "root",
            None,
            "D:/workspace".into(),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "Original work",
            &state,
            true,
        );
        assert!(original.deferred_user_event.is_some());
        // Repeated preparation also must not append an internal prompt to the observation journal.
        // 重复准备也不得将内部提示追加到观察日志。
        for _attempt in 0..2 {
            let requests = runtime.prepare_user_prompt_hook_requests(
                session_id,
                "continuation",
                None,
                "D:/workspace".into(),
                source,
                SystemPromptHookPhase::SubsequentSession,
                "Internal continuation",
                &state,
                true,
            );
            assert_eq!(requests.turn_id, original.turn_id);
            assert!(requests.deferred_user_event.is_none());
            let turn = &runtime.active_turns[&session_id];
            assert_eq!(turn.user_content, "Original work");
            assert!(turn.timeline.is_empty());
        }
    }
}

/// Auxiliary preparation neither opens a business Hook turn nor appends to a retained one.
/// 辅助准备既不打开业务 Hook 回合，也不向保留的业务回合追加记录。
#[test]
fn question_execution_preparation_preserves_business_hook_ownership() {
    // Reuse one session to cover idle preparation and preparation beside retained business state.
    // 复用同一会话，覆盖空闲准备以及保留业务状态时的准备。
    let session_id = SessionId::from_u128(149);
    // The runtime starts without a business turn.
    // 运行时初始没有业务回合。
    let mut runtime = ConversationHookRuntime::new();
    // Canonical history remains immutable during auxiliary correlation.
    // 辅助关联期间规范历史保持不变。
    let state = SessionConversationState::default();
    for with_business in [false, true] {
        if with_business {
            runtime.prepare_user_prompt_hook_requests(
                session_id,
                "business",
                None,
                String::from("D:/workspace"),
                PendingSessionPromptSource::UserFollowup,
                SystemPromptHookPhase::FirstSession,
                "original task",
                &state,
                true,
            );
        }
        // Retain the exact business identity for comparison after auxiliary preparation.
        // 保留精确业务身份，以便与辅助准备后的状态比较。
        let original_turn_id = runtime
            .active_turns
            .get(&session_id)
            .map(|turn| turn.turn_id.clone());
        // The durable execution ID gives retries the same preparation identity.
        // 持久执行 ID 使重试具有相同准备身份。
        let requests = runtime.prepare_user_prompt_hook_requests(
            session_id,
            "qa-1",
            None,
            String::from("D:/workspace"),
            PendingSessionPromptSource::QuestionExecution,
            SystemPromptHookPhase::SubsequentSession,
            "",
            &state,
            false,
        );
        assert_eq!(
            requests.turn_id,
            format!("session-{session_id}-question-execution-qa-1")
        );
        assert_eq!(
            runtime
                .active_turns
                .get(&session_id)
                .map(|turn| turn.turn_id.clone()),
            original_turn_id
        );
        if let Some(turn) = runtime.active_turns.get(&session_id) {
            assert_eq!(turn.user_content, "original task");
            assert!(turn.timeline.is_empty());
            assert!(turn.vmm_runtime_ready);
        }
        // Skipped precheck records correlation, without claiming a new user turn.
        // 跳过的 precheck 保存关联信息，但不宣称创建了用户回合。
        let skipped = requests.precheck.skip();
        assert_eq!(
            skipped.report.expect("skip outcome").payload.value["is_new_turn"],
            false
        );
    }
}

/// Verifies profile preparation dispatches independently when user precheck is explicitly skipped.
/// 验证显式跳过用户 precheck 时画像准备仍会独立派发。
#[test]
fn profile_preparation_dispatches_when_precheck_is_skipped() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    // runtime owns two handlers registered on distinct Hook points.
    // runtime 持有注册在不同 Hook 点位上的两个处理器。
    let mut runtime = ConversationHookRuntime::new();
    // profile_calls counts only the independent profile handler.
    // profile_calls 只统计独立画像处理器。
    let profile_calls = Arc::new(AtomicUsize::new(0));
    // precheck_calls proves skipping recall never suppresses profile dispatch.
    // precheck_calls 证明跳过召回绝不会抑制画像派发。
    let precheck_calls = Arc::new(AtomicUsize::new(0));
    // profile_counter is moved into the profile-only handler.
    // profile_counter 被移入纯画像处理器。
    let profile_counter = Arc::clone(&profile_calls);
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "test.profile.prepare",
                HOOK_CONVERSATION_PROFILE_PREPARE,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |_invocation: HookInvocation| {
                let profile_counter = Arc::clone(&profile_counter);
                async move {
                    profile_counter.fetch_add(1, Ordering::SeqCst);
                    Ok(HookResult::default())
                }
            })),
        )
        .expect("profile handler should register");
    // precheck_counter is moved into the recall-only handler.
    // precheck_counter 被移入纯召回处理器。
    let precheck_counter = Arc::clone(&precheck_calls);
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "test.turn.precheck",
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Effect,
            ),
            Arc::new(FnHookHandler::new(move |_invocation: HookInvocation| {
                let precheck_counter = Arc::clone(&precheck_counter);
                async move {
                    precheck_counter.fetch_add(1, Ordering::SeqCst);
                    Ok(HookResult::default())
                }
            })),
        )
        .expect("precheck handler should register");
    // session_id identifies the single correlated prompt turn under test.
    // session_id 标识被测的唯一关联提示词回合。
    let session_id = SessionId::from_u128(148);
    // conversation_state supplies an empty canonical history for request construction.
    // conversation_state 为请求构建提供空规范历史。
    let conversation_state = SessionConversationState::default();
    // requests prepares sibling payloads without making either line own the other.
    // requests 准备同级载荷，而不让任一线路持有另一线路。
    let requests = runtime.prepare_user_prompt_hook_requests(
        session_id,
        "goal-continuation",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::GoalContinuation,
        SystemPromptHookPhase::SubsequentSession,
        "Continue working toward the active thread goal.",
        &conversation_state,
        true,
    );
    // The first Goal preparation must not announce the host's fixed instruction as a human input.
    // 首次 Goal 准备不得将宿主固定指令宣告为人类输入。
    assert!(requests.deferred_user_event.is_none());
    // profile_request owns system-profile placement metadata and its dedicated Hook dispatch.
    // profile_request 持有系统画像放置元数据及其专用 Hook 派发。
    let profile_request = requests.profile;
    // precheck_request owns only the user-recall payload and remains independently skippable.
    // precheck_request 仅持有用户召回载荷，并可独立跳过。
    let precheck_request = requests.precheck;
    // test_runtime waits for the independent profile worker without invoking user precheck.
    // test_runtime 等待独立画像工作线程，同时不调用用户 precheck。
    let test_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build");
    // profile_outcome proves the profile-only dispatch completed successfully.
    // profile_outcome 证明纯画像派发已成功完成。
    let profile_outcome = test_runtime.block_on(profile_request.run());
    // precheck_outcome intentionally skips the recall-only dispatcher.
    // precheck_outcome 刻意跳过纯召回调度器。
    let precheck_outcome = precheck_request.skip();

    assert!(profile_outcome.report.is_ok());
    assert!(precheck_outcome.report.is_ok());
    assert_eq!(profile_outcome.turn_id, precheck_outcome.turn_id);
    assert_eq!(
        profile_outcome.report.as_ref().ok().and_then(|report| {
            report
                .payload
                .value
                .get("system_prompt_hook_phase")
                .and_then(Value::as_str)
        }),
        Some("subsequent_session")
    );
    assert!(
        profile_outcome
            .report
            .as_ref()
            .is_ok_and(|report| { report.payload.value.get("user_content").is_none() })
    );
    assert!(precheck_outcome.report.as_ref().is_ok_and(|report| {
        report
            .payload
            .value
            .get("system_prompt_hook_phase")
            .is_none()
            && report
                .payload
                .value
                .get("user_content")
                .and_then(Value::as_str)
                == Some("Continue working toward the active thread goal.")
    }));
    assert_eq!(profile_calls.load(Ordering::SeqCst), 1);
    assert_eq!(precheck_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn turn_completion_keeps_supplement_inside_timeline() {
    let mut runtime = ConversationHookRuntime::new();
    let session_id = SessionId::from_u128(42);
    let state = SessionConversationState::default();

    runtime.prepare_user_prompt_hook_requests(
        session_id,
        "entry-1",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "first",
        &state,
        true,
    );
    runtime.prepare_user_prompt_hook_requests(
        session_id,
        "entry-2",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserInterrupt,
        SystemPromptHookPhase::SubsequentSession,
        "supplement",
        &state,
        true,
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

    // requests exposes the shared immutable turn correlation used by the tool event.
    // requests 暴露工具事件使用的共享不可变回合关联。
    let requests = runtime.prepare_user_prompt_hook_requests(
        session_id,
        "entry-1",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "first",
        &state,
        true,
    );
    runtime.record_tool_runtime_event_for_execution(
        session_id,
        &requests.turn_id,
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
    runtime.prepare_user_prompt_hook_requests(
        session_id,
        "entry-1",
        Some(String::from("project")),
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "spawn two subagents",
        &state,
        true,
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
            true,
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
        .prepare_user_prompt_hook_requests(
            SessionId::from_u128(44),
            "entry-1",
            None,
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "hello",
            &SessionConversationState::default(),
            true,
        )
        .precheck
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
        .prepare_user_prompt_hook_requests(
            SessionId::from_u128(45),
            "entry-1",
            None,
            String::from("D:/workspace"),
            PendingSessionPromptSource::UserFollowup,
            SystemPromptHookPhase::FirstSession,
            "hello",
            &SessionConversationState::default(),
            true,
        )
        .precheck
        .run()
        .await;

    let report = outcome
        .report
        .expect("registration-level timeout should still return a dispatch report");
    assert!(report.injections.is_empty());
    assert!(!report.failures.is_empty());
    assert!(started_at.elapsed() < Duration::from_millis(TEST_HOOK_TIMEOUT_MS + 1_500));
}

/// Verifies the caller-side deadline returns even when a handler blocks its worker thread.
/// 验证处理器阻塞工作线程且不让出时，调用方截止时间仍能按时返回。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a non-yielding handler stalls the awaiting caller.
/// 此测试无返回值；若不让出线程的处理器拖住等待方则失败。
#[tokio::test]
async fn awaited_dispatch_returns_at_deadline_when_handler_blocks_its_thread() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};

    const TEST_DEADLINE: Duration = Duration::from_millis(200);
    const HANDLER_BLOCK: Duration = Duration::from_secs(3);

    let mut runtime = ConversationHookRuntime::new();
    runtime
        .dispatcher_mut()
        .register(
            HookRegistration::new(
                "blocking",
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                // Deliberately violates the handler contract by blocking without yielding.
                // 故意违反处理器契约：阻塞线程且不让出。
                std::thread::sleep(HANDLER_BLOCK);
                Ok(HookResult::default())
            })),
        )
        .expect("hook registration should succeed");
    let context = HookContext {
        run_id: String::from("blocking-test"),
        point: HOOK_CONVERSATION_TURN_PRECHECK.to_string(),
        scope: HookScope::Session,
        actor: HookActor::new("vulcan-code-runtime"),
        workspace_id: None,
        project_id: None,
        session_id: None,
        conversation_id: None,
        user_id: None,
        capabilities: Default::default(),
        metadata: BTreeMap::new(),
    };
    let request = HookDispatchRequest::new(
        context,
        HookPayload {
            value: serde_json::json!({}),
        },
    );

    let started_at = Instant::now();
    let result = run_hook_dispatch_on_worker_thread_within(
        "blocking hook",
        String::from("vulcan-blocking-hook-test"),
        runtime.dispatcher_mut().clone(),
        request,
        TEST_DEADLINE,
    )
    .await;

    assert_eq!(
        result.expect_err("a blocked worker must be abandoned at the deadline"),
        "blocking hook dispatch timed out"
    );
    assert!(started_at.elapsed() < HANDLER_BLOCK);
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
    runtime.prepare_user_prompt_hook_requests(
        session_id,
        "entry-1",
        None,
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "hello",
        &SessionConversationState::default(),
        true,
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
    runtime.prepare_user_prompt_hook_requests(
        session_id,
        "entry-1",
        None,
        String::from("D:/workspace"),
        PendingSessionPromptSource::UserFollowup,
        SystemPromptHookPhase::FirstSession,
        "hello",
        &SessionConversationState::default(),
        true,
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
