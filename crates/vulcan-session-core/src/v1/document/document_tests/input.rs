//! Client input admission, queue resolution, draft ownership, and idempotency regression tests.
//! 客户端输入准入、队列处置、草稿归属及幂等回归测试。

use super::*;

/// Root input is rejected while a turn is still open.
/// 回合仍然开放时根输入被拒绝。
#[test]
fn root_input_rejected_while_turn_open() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let result = doc.submit_root_input_fixture("b", Vec::new(), T0 + 1);
    assert!(matches!(
        result,
        Err(DomainError::RootInputWhileTurnOpen { .. })
    ));
}

/// A root user admission activates its exact bound request before the provider-visible input.
/// 根用户输入准入会在 Provider 可见输入之前激活其精确绑定请求。
#[test]
fn root_input_activates_bound_request_at_admission() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.metadata.config.active_request = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: None,
        model: Some(String::from("gpt-5")),
        thinking_level: Some(String::from("high")),
        temperature: None,
    };
    let bound_request = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: None,
        model: Some(String::from("gpt-5.1")),
        thinking_level: Some(String::from("low")),
        temperature: None,
    };
    doc.set_pending_request(
        PendingRequestConfig {
            provider: bound_request.provider.clone(),
            connection_id: bound_request.connection_id.clone(),
            model: bound_request.model.clone(),
            thinking_level: bound_request.thinking_level.clone(),
            temperature: bound_request.temperature,
            set_at_ms: T0 + 1,
        },
        T0 + 1,
    )?;

    doc.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("bound-root-request")?,
            "run with the selected request",
            Vec::new(),
        )
        .with_bound_request_config(bound_request.clone()),
        T0 + 2,
    )?;

    assert_eq!(doc.metadata.config.active_request, bound_request);
    assert!(doc.metadata.config.pending_request.is_none());
    let request_change_seq = doc
        .events()
        .iter()
        .find(|event| matches!(event.event, EventBody::RequestConfigChanged(_)))
        .map(ConversationEvent::seq)
        .expect("request switch event");
    let user_input_seq = doc
        .events()
        .iter()
        .find(|event| matches!(event.event, EventBody::UserInput(_)))
        .map(ConversationEvent::seq)
        .expect("root input event");
    assert!(request_change_seq < user_input_seq);
    Ok(())
}

/// A queued user input atomically supersedes an undispatched Goal continuation and is restart-idempotent.
/// 一条排队用户输入会原子取代尚未派发的 Goal 续跑，并在重启后保持幂等。
#[test]
fn enqueue_preempts_undispatched_goal_and_replays_exact_receipt() -> Result<(), DomainError> {
    let mut doc = new_document();
    let goal = doc.create_goal("finish", None, "Keep working.", T0 + 1)?;
    let goal_turn = goal
        .continuation_turn_id
        .expect("creating a Goal without an open Turn must start its continuation");
    let action_id = ClientActionId::try_new("goal-preempting-user")?;
    let submission = ClientInputSubmission::new(action_id.clone(), "user priority", Vec::new());
    let before_revision = doc.memory_revision();

    let committed = doc.enqueue_input(submission.clone(), T0 + 2)?;
    assert!(committed.changed());
    let receipt = committed.into_value();
    let EnqueueInputDisposition::SupersededGoalAndScheduled {
        stopped_turn_id,
        stopped_turn_stats_seq,
        scheduled_turn_id,
        input_seq,
    } = receipt.disposition.clone()
    else {
        panic!("an undispatched Goal continuation must be superseded immediately");
    };
    assert_eq!(stopped_turn_id, goal_turn);
    assert_eq!(doc.memory_revision(), before_revision + 1);
    assert!(doc.metadata.input_state.queued_inputs.is_empty());
    assert_eq!(
        doc.active_turn()
            .map(|turn| (turn.turn_id, turn.origin, turn.start_seq, turn.closed)),
        Some((scheduled_turn_id, TurnOrigin::UserRoot, input_seq, false))
    );
    assert_eq!(
        doc.events()
            .iter()
            .filter(|event| {
                event.turn_id == Some(goal_turn) && matches!(event.event, EventBody::TurnStats(_))
            })
            .count(),
        1
    );
    assert!(matches!(
        doc.events()
            .iter()
            .find(|event| event.seq == stopped_turn_stats_seq)
            .map(|event| &event.event),
        Some(EventBody::TurnStats(TurnStatsEvent {
            outcome: TurnOutcome::Stopped,
            ..
        }))
    ));

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let restored_revision = restored.memory_revision();
    let event_count = restored.events().len();
    let replay = restored.enqueue_input(submission.clone(), T0 + 3)?;
    assert!(!replay.changed());
    assert_eq!(replay.into_value(), receipt);
    assert_eq!(restored.memory_revision(), restored_revision);
    assert_eq!(restored.events().len(), event_count);

    let before_conflict = restored.clone();
    assert!(matches!(
        restored.enqueue_input(
            ClientInputSubmission::new(action_id, "different", Vec::new()),
            T0 + 4,
        ),
        Err(DomainError::ClientActionConflict { .. })
    ));
    assert_eq!(restored, before_conflict);
    Ok(())
}

/// Goal preemption stages all ids and events before publishing, while dispatched Goal work only queues.
/// Goal 抢占会在发布前暂存全部 id 与事件，而已经派发的 Goal 工作只会进入队列。
#[test]
fn enqueue_goal_preemption_is_failure_atomic_and_never_crosses_active_api()
-> Result<(), DomainError> {
    let mut doc = new_document();
    doc.create_goal("finish", None, "Keep working.", T0 + 1)?;
    let (_, next_seq) = doc.id_allocator.frontier();
    let mut exhausted = doc.clone();
    exhausted.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_failure = exhausted.clone();
    assert!(
        exhausted
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("goal-preempt-allocation-failure")?,
                    "user priority",
                    Vec::new(),
                ),
                T0 + 2,
            )
            .is_err()
    );
    assert_eq!(exhausted, before_failure);

    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let goal_turn = doc.require_open_turn()?.turn_id;
    let outcome = doc.enqueue_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("goal-active-api-queued")?,
            "wait behind active Goal API",
            Vec::new(),
        ),
        T0 + 3,
    )?;
    assert!(matches!(
        outcome.into_value().disposition,
        EnqueueInputDisposition::Queued
    ));
    assert_eq!(doc.active_api().map(|active| active.api_call_id), Some(api));
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(goal_turn));
    assert_eq!(doc.metadata.input_state.queued_inputs.len(), 1);
    Ok(())
}

/// Draft and queued-input CRUD remain metadata-authoritative, idempotent, and strict after reload.
/// 草稿与排队输入 CRUD 在重载后仍保持元数据权威、幂等且严格。
#[test]
fn draft_and_queued_crud_are_atomic_idempotent_and_reload_strict() -> Result<(), DomainError> {
    let mut doc = new_document();
    let first = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-crud-first")?,
                "first",
                vec![ResourceId::try_new(11)?],
            ),
            T0 + 1,
        )?
        .into_value();
    let second = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-crud-second")?,
                "second",
                vec![ResourceId::try_new(12)?],
            ),
            T0 + 2,
        )?
        .into_value();
    let first_identity = QueuedInputIdentity {
        client_action_id: first.client_action_id,
        input_id: first.input_id,
    };
    let second_identity = QueuedInputIdentity {
        client_action_id: second.client_action_id,
        input_id: second.input_id,
    };
    assert_eq!(
        doc.queued_input_identity_for_action(&first_identity.client_action_id)?,
        Some(first_identity.clone())
    );
    assert_eq!(
        doc.queued_input_identity_for_action(&second_identity.client_action_id)?,
        Some(second_identity.clone())
    );

    let reverse = vec![second_identity.clone(), first_identity.clone()];
    assert!(
        doc.reorder_queued_inputs(reverse.clone(), T0 + 3)?
            .changed()
    );
    let revision = doc.memory_revision();
    assert!(
        !doc.reorder_queued_inputs(reverse.clone(), T0 + 4)?
            .changed()
    );
    assert_eq!(doc.memory_revision(), revision);
    let before_bad_order = doc.clone();
    assert!(matches!(
        doc.reorder_queued_inputs(vec![first_identity.clone()], T0 + 5),
        Err(DomainError::InvalidQueuedInputOrder {
            field: "member_count"
        })
    ));
    assert_eq!(doc, before_bad_order);

    let moved = doc.move_queued_input_to_draft(second_identity.clone(), T0 + 6)?;
    assert!(moved.changed());
    assert_eq!(
        moved.into_value().draft.restored_from_queue,
        Some(second_identity.clone())
    );
    assert_eq!(doc.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        doc.queued_input_identity_for_action(&second_identity.client_action_id)?,
        Some(second_identity.clone())
    );
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let revision = restored.memory_revision();
    assert!(
        !restored
            .move_queued_input_to_draft(second_identity.clone(), T0 + 7)?
            .changed()
    );
    assert_eq!(restored.memory_revision(), revision);

    assert!(
        restored
            .set_draft("manual replacement", Vec::new(), T0 + 8)?
            .changed()
    );
    assert!(
        restored
            .metadata
            .input_state
            .draft
            .restored_from_queue
            .is_none()
    );
    let revision = restored.memory_revision();
    let replayed_move = restored.move_queued_input_to_draft(second_identity, T0 + 9)?;
    assert!(!replayed_move.changed());
    assert_eq!(restored.memory_revision(), revision);
    assert_eq!(replayed_move.into_value().draft.text, "second");
    let revision = restored.memory_revision();
    assert!(
        !restored
            .set_draft("manual replacement", Vec::new(), T0 + 10)?
            .changed()
    );
    assert_eq!(restored.memory_revision(), revision);
    assert!(restored.clear_draft(T0 + 11)?.changed());
    let revision = restored.memory_revision();
    assert!(!restored.clear_draft(T0 + 12)?.changed());
    assert_eq!(restored.memory_revision(), revision);

    assert!(
        restored
            .remove_queued_input(first_identity.clone(), T0 + 13)?
            .changed()
    );
    let persisted_metadata = serde_json::from_str(
        &serde_json::to_string(&restored.metadata).expect("metadata must serialize"),
    )
    .expect("metadata must strictly reload");
    let persisted_events = serde_json::from_str(
        &serde_json::to_string(restored.events()).expect("events must serialize"),
    )
    .expect("events must strictly reload");
    let mut restored = SessionDocument::from_parts(
        persisted_metadata,
        restored.runtime_lock.clone(),
        restored.compactions.clone(),
        restored.segment_catalog().to_vec(),
        persisted_events,
    )?;
    let revision = restored.memory_revision();
    let replayed_admission = restored.enqueue_input(
        ClientInputSubmission::new(
            first_identity.client_action_id.clone(),
            "first",
            vec![ResourceId::try_new(11)?],
        ),
        T0 + 14,
    )?;
    assert!(!replayed_admission.changed());
    assert_eq!(
        replayed_admission.into_value().input_id,
        first_identity.input_id
    );
    assert_eq!(restored.memory_revision(), revision);
    let later = restored
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-crud-later")?,
                "later",
                Vec::new(),
            ),
            T0 + 15,
        )?
        .into_value();
    let revision = restored.memory_revision();
    assert!(
        !restored
            .remove_queued_input(first_identity.clone(), T0 + 16)?
            .changed()
    );
    assert_eq!(restored.memory_revision(), revision);
    assert_eq!(restored.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        restored.metadata.input_state.queued_inputs[0].input_id,
        later.input_id
    );
    assert_eq!(
        restored.queued_input_identity_for_action(&first_identity.client_action_id)?,
        Some(first_identity.clone())
    );
    assert_eq!(
        restored.queued_input_identity_for_action(&ClientActionId::try_new(
            "queue-crud-unknown-action"
        )?)?,
        None
    );

    let wrong_local_id = QueuedInputIdentity {
        client_action_id: first_identity.client_action_id.clone(),
        input_id: PendingInputId::try_new(first_identity.input_id.value() + 100)?,
    };
    let before_conflict = restored.clone();
    assert!(matches!(
        restored.remove_queued_input(wrong_local_id, T0 + 17),
        Err(DomainError::ClientActionConflict { .. })
    ));
    assert_eq!(restored, before_conflict);
    let unknown = QueuedInputIdentity {
        client_action_id: ClientActionId::try_new("queue-crud-unknown")?,
        input_id: PendingInputId::try_new(first_identity.input_id.value() + 101)?,
    };
    let before_missing = restored.clone();
    assert!(matches!(
        restored.remove_queued_input(unknown, T0 + 18),
        Err(DomainError::QueuedInputNotFound { .. })
    ));
    assert_eq!(restored, before_missing);
    Ok(())
}

/// User-input admission consumes only the exact durable draft and preserves later edits.
/// 用户输入准入只消费完全匹配的持久化草稿，并保留随后产生的新编辑。
#[test]
fn input_admission_consumes_only_the_matching_draft_atomically() -> Result<(), DomainError> {
    let root_resources = vec![ResourceId::try_new(101)?, ResourceId::try_new(102)?];
    let root_action = ClientActionId::try_new("draft-root-admission")?;
    let root_submission =
        ClientInputSubmission::new(root_action, "root draft", root_resources.clone());
    let mut root = new_document();
    root.set_draft("root draft", root_resources, T0 + 1)?;
    root.submit_root_input(root_submission.clone(), T0 + 2)?;
    assert_eq!(root.metadata.input_state.draft, DraftState::default());
    root.set_draft("newer root draft", Vec::new(), T0 + 3)?;
    let revision = root.memory_revision();
    assert!(!root.submit_root_input(root_submission, T0 + 4)?.changed());
    assert_eq!(root.memory_revision(), revision);
    assert_eq!(root.metadata.input_state.draft.text, "newer root draft");

    let queued_resources = vec![ResourceId::try_new(201)?];
    let queued_submission = ClientInputSubmission::new(
        ClientActionId::try_new("draft-queued-admission")?,
        "queued draft",
        queued_resources.clone(),
    );
    let mut queued = new_document();
    queued.set_draft("queued draft", queued_resources, T0 + 5)?;
    queued.enqueue_input(queued_submission.clone(), T0 + 6)?;
    assert_eq!(queued.metadata.input_state.draft, DraftState::default());
    queued.set_draft("newer queued draft", Vec::new(), T0 + 7)?;
    let revision = queued.memory_revision();
    assert!(!queued.enqueue_input(queued_submission, T0 + 8)?.changed());
    assert_eq!(queued.memory_revision(), revision);
    assert_eq!(queued.metadata.input_state.draft.text, "newer queued draft");

    let mut pending = new_document();
    pending.submit_root_input_fixture("open", Vec::new(), T0 + 9)?;
    let pending_resources = vec![ResourceId::try_new(301)?];
    let pending_submission = ClientInputSubmission::new(
        ClientActionId::try_new("draft-pending-admission")?,
        "pending draft",
        pending_resources.clone(),
    );
    pending.set_draft("pending draft", pending_resources, T0 + 10)?;
    pending.add_pending_in_turn_input(pending_submission.clone(), T0 + 11)?;
    assert_eq!(pending.metadata.input_state.draft, DraftState::default());
    pending.set_draft("newer pending draft", Vec::new(), T0 + 12)?;
    let revision = pending.memory_revision();
    assert!(
        !pending
            .add_pending_in_turn_input(pending_submission, T0 + 13)?
            .changed()
    );
    assert_eq!(pending.memory_revision(), revision);
    assert_eq!(
        pending.metadata.input_state.draft.text,
        "newer pending draft"
    );

    let mut continued = new_document();
    continued.submit_root_input_fixture("continue", Vec::new(), T0 + 14)?;
    let api = continued.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 15,
    )?;
    complete_user_interrupted(&mut continued, api, T0 + 16)?;
    let continue_resources = vec![ResourceId::try_new(401)?];
    let continue_request = ContinuationRequest::new(
        ClientActionId::try_new("draft-continue-admission")?,
        "continue draft",
        continue_resources.clone(),
    );
    continued.set_draft("continue draft", continue_resources, T0 + 17)?;
    continued.continue_open_turn(continue_request.clone(), T0 + 18)?;
    assert_eq!(continued.metadata.input_state.draft, DraftState::default());
    continued.set_draft("newer continue draft", Vec::new(), T0 + 19)?;
    let revision = continued.memory_revision();
    assert!(
        !continued
            .continue_open_turn(continue_request, T0 + 20)?
            .changed()
    );
    assert_eq!(continued.memory_revision(), revision);
    assert_eq!(
        continued.metadata.input_state.draft.text,
        "newer continue draft"
    );

    let mut rejected = new_document();
    rejected.submit_root_input_fixture("already open", Vec::new(), T0 + 21)?;
    rejected.set_draft("must survive", Vec::new(), T0 + 22)?;
    let before_rejection = rejected.clone();
    assert!(matches!(
        rejected.submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("draft-rejected-root")?,
                "must survive",
                Vec::new(),
            ),
            T0 + 23,
        ),
        Err(DomainError::RootInputWhileTurnOpen { .. })
    ));
    assert_eq!(rejected, before_rejection);
    Ok(())
}

/// Guided queue input moves into the open Turn once and survives promotion plus strict reload.
/// 引导的排队输入只进入开放回合一次，并在提升及严格重载后保持幂等。
#[test]
fn guided_queued_input_is_durable_idempotent_and_same_turn() -> Result<(), DomainError> {
    let mut document = new_document();
    let root_turn_id = document.submit_root_input_fixture("root", Vec::new(), T0 + 1)?;
    let api_call_id = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("guided-queued-input")?,
                "supplement",
                vec![ResourceId::try_new(501)?],
            ),
            T0 + 3,
        )?
        .into_value();
    let identity = QueuedInputIdentity {
        client_action_id: queued.client_action_id,
        input_id: queued.input_id,
    };

    let first = document.promote_queued_input_to_pending_turn(identity.clone(), T0 + 4)?;
    assert!(first.changed());
    let first_receipt = first.into_value();
    assert_eq!(first_receipt.turn_id, root_turn_id);
    assert!(document.metadata.input_state.queued_inputs.is_empty());
    assert_eq!(
        document.metadata.input_state.pending_in_turn_inputs.len(),
        1
    );
    assert_eq!(
        document.metadata.input_state.pending_in_turn_inputs[0].input_id,
        identity.input_id
    );

    let revision = document.memory_revision();
    let retry = document.promote_queued_input_to_pending_turn(identity.clone(), T0 + 5)?;
    assert!(!retry.changed());
    assert_eq!(retry.into_value(), first_receipt);
    assert_eq!(document.memory_revision(), revision);

    document.append_assistant_text(
        api_call_id,
        "continue",
        AssistantSemanticStatus::Completed,
        T0 + 6,
    )?;
    let promoted = document
        .complete_api_group(
            api_call_id,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: T0 + 7,
            },
        )?
        .promoted_input_seqs;
    assert_eq!(promoted.len(), 1);
    let promoted_event = document
        .events()
        .iter()
        .find(|event| event.seq == promoted[0])
        .expect("promoted user input must exist");
    assert_eq!(promoted_event.turn_id, Some(root_turn_id));
    assert!(matches!(
        &promoted_event.event,
        EventBody::UserInput(input)
            if input.input_source == UserInputSource::PendingInTurn
                && input.input_id == identity.input_id
                && input.client_action_id == identity.client_action_id
    ));

    let mut restored = SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )?;
    let restored_revision = restored.memory_revision();
    let restored_retry = restored.promote_queued_input_to_pending_turn(identity, T0 + 8)?;
    assert!(!restored_retry.changed());
    assert_eq!(restored_retry.into_value(), first_receipt);
    assert_eq!(restored.memory_revision(), restored_revision);
    Ok(())
}

/// Guided queue input cannot leave the future queue when no logical Turn is open.
/// 没有开放逻辑回合时，引导操作不能让输入离开未来队列。
#[test]
fn guided_queued_input_without_open_turn_is_failure_atomic() -> Result<(), DomainError> {
    let mut document = new_document();
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("guided-without-turn")?,
                "queued",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let identity = QueuedInputIdentity {
        client_action_id: queued.client_action_id,
        input_id: queued.input_id,
    };
    let before = document.clone();
    assert!(
        document
            .promote_queued_input_to_pending_turn(identity, T0 + 2)
            .is_err()
    );
    assert_eq!(document, before);
    Ok(())
}

/// Queued terminal resolutions publish no partial metadata when seq validation fails.
/// 排队输入终结处置在 seq 校验失败时不发布任何部分元数据。
#[test]
fn queued_terminal_resolutions_are_failure_atomic() -> Result<(), DomainError> {
    let mut removed = new_document();
    let remove_receipt = removed
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-remove-seq-failure")?,
                "remove",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let remove_identity = QueuedInputIdentity {
        client_action_id: remove_receipt.client_action_id,
        input_id: remove_receipt.input_id,
    };
    let (next_local_id, next_seq) = removed.id_allocator.frontier();
    removed.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before_remove = removed.clone();
    assert!(matches!(
        removed.remove_queued_input(remove_identity, T0 + 2),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(removed, before_remove);

    let mut moved = new_document();
    let move_receipt = moved
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-move-seq-failure")?,
                "move",
                Vec::new(),
            ),
            T0 + 3,
        )?
        .into_value();
    let move_identity = QueuedInputIdentity {
        client_action_id: move_receipt.client_action_id,
        input_id: move_receipt.input_id,
    };
    let (next_local_id, next_seq) = moved.id_allocator.frontier();
    moved.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before_move = moved.clone();
    assert!(matches!(
        moved.move_queued_input_to_draft(move_identity, T0 + 4),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(moved, before_move);
    Ok(())
}

/// Strict reload rejects a queue-restored draft whose local input id is still reused elsewhere.
/// 严格重载会拒绝本地输入 id 仍在其他位置复用的队列恢复草稿。
#[test]
fn reload_rejects_reused_queue_restored_draft_identity() -> Result<(), DomainError> {
    let mut doc = new_document();
    let receipt = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-corrupt-draft")?,
                "queued",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value();
    let identity = QueuedInputIdentity {
        client_action_id: ClientActionId::try_new("different-action")?,
        input_id: receipt.input_id,
    };
    doc.metadata.input_state.draft = DraftState {
        resource_sources: Vec::new(),
        staged_attachment_ids: Vec::new(),
        parameters: None,
        text: "draft".to_string(),
        resource_ids: Vec::new(),
        restored_from_queue: Some(identity),
    };
    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog().to_vec(),
            doc.events().to_vec(),
        ),
        Err(DomainError::InvalidQueuedInputOrder {
            field: "draft_input_id_reused"
        })
    ));
    Ok(())
}

/// All input entry points reject empty facts without creating false revisions.
/// 所有输入入口都会拒绝空事实且不会制造虚假修订。
#[test]
fn user_input_validation_does_not_create_false_revisions() -> Result<(), DomainError> {
    let mut doc = new_document();
    let fresh = doc.clone();
    assert!(matches!(
        doc.submit_root_input_fixture("   ", Vec::new(), T0),
        Err(DomainError::EmptyUserInput)
    ));
    assert_eq!(doc, fresh);

    let resource = ResourceId::try_new(1)?;
    doc.submit_root_input_fixture("", vec![resource], T0)?;

    let before_queue = doc.clone();
    assert!(matches!(
        doc.enqueue_input_fixture("\t", Vec::new(), T0 + 2),
        Err(DomainError::EmptyUserInput)
    ));
    assert_eq!(doc, before_queue);
    let before_pending = doc.clone();
    assert!(matches!(
        doc.add_pending_in_turn_input_fixture("\n", Vec::new(), T0 + 3),
        Err(DomainError::EmptyUserInput)
    ));
    assert_eq!(doc, before_pending);
    Ok(())
}
