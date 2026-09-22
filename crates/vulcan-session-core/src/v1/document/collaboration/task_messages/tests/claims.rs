//! Business-turn claim identity, queue priority and failure atomicity.
//! 业务回合认领身份、队列优先级及失败原子性。

use super::*;
use crate::v1::document::TurnOrigin;
use crate::v1::event::TurnOutcome;

/// Accepts one queued intent and closes only the previous business turn.
/// 接纳一条排队意图，仅关闭之前的业务回合。
/// Returns an idle target and the exact immutable intent.
/// 返回空闲目标及精确不可变意图。
fn idle_fixture() -> (SessionDocument, CollaborationTaskMessage) {
    let (_, mut target, message) = fixture(CollaborationTaskMessageKind::TaskStart);
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept").unwrap(),
            message.clone(),
            3,
        )
        .unwrap();
    target.close_turn(TurnOutcome::Completed, 1, 4).unwrap();
    (target, message)
}

/// A claim allocates a distinct turn atomically, preserves the human draft and replays through reload.
/// 认领原子分配独立回合、保留用户草稿，并且可以重载后重放。
#[test]
fn task_message_claim_is_atomic_and_reloads_exact_business_origin() {
    let (mut target, message) = idle_fixture();
    let old_turn = target.active_turn().unwrap().turn_id;
    target
        .set_draft("User's unfinished draft", vec![], 5)
        .unwrap();
    let draft = target.metadata.input_state.draft.clone();
    target
        .set_pending_request(
            crate::v1::config::PendingRequestConfig {
                provider: Some("target-provider".into()),
                connection_id: None,
                model: Some("target-model".into()),
                thinking_level: None,
                temperature: None,
                set_at_ms: 5,
            },
            5,
        )
        .unwrap();
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("idle").unwrap(),
            state: CollaborationRunControl::NaturallyIdle,
            changed_at_ms: 5,
        })
        .unwrap();
    let prefix = target.events().to_vec();
    let revision = target.memory_revision();
    let claim = target
        .claim_task_message_before_api(
            CollaborationOperationId::try_new("claim").unwrap(),
            message.delivery_id.clone(),
            6,
        )
        .unwrap()
        .into_value();
    let CollaborationTaskMessageEffectKind::Claimed {
        turn_id,
        input_id,
        input_seq,
        ref execution_id,
    } = claim.outcome
    else {
        panic!("expected a task claim");
    };
    assert_ne!(turn_id, old_turn);
    assert_eq!(execution_id.as_str(), "task-delivery:task-execution");
    assert_eq!(target.memory_revision(), revision + 1);
    assert_eq!(claim.target_revision, target.memory_revision());
    assert_eq!(&target.events()[..prefix.len()], prefix.as_slice());
    assert_eq!(target.metadata.input_state.draft, draft);
    assert_eq!(
        target.metadata.config.active_request.model.as_deref(),
        Some("target-model")
    );
    assert!(target.metadata.config.pending_request.is_none());
    assert_eq!(
        target.collaboration_run_control(),
        CollaborationRunControl::Running
    );
    assert!(target.active_api().is_none());
    assert_eq!(target.active_turn().unwrap().origin, TurnOrigin::AgentTask);
    assert_eq!(target.active_turn().unwrap().start_seq, claim.event_seq);
    let input = target
        .events()
        .iter()
        .find(|event| event.seq == input_seq)
        .unwrap();
    assert!(
        matches!(&input.event, EventBody::UserInput(input) if input.input_id == input_id
        && input.input_source == crate::v1::event::UserInputSource::AgentMessage
        && input.text.contains("kind=\"task_start\""))
    );
    let mut restored = reload(&target);
    assert_eq!(
        restored.active_turn().unwrap().origin,
        TurnOrigin::AgentTask
    );
    assert_eq!(
        restored
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("recovery-claim").unwrap(),
                message.delivery_id.clone(),
                7,
            )
            .unwrap()
            .into_value(),
        claim
    );
    assert_eq!(restored, reload(&target));
    restored.close_turn(TurnOutcome::Stopped, 1, 8).unwrap();
    let closed = restored.clone();
    assert_eq!(
        restored
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("claim").unwrap(),
                message.delivery_id,
                9,
            )
            .unwrap()
            .into_value(),
        claim
    );
    assert_eq!(restored, closed);
}

/// Busy targets, earlier agent tasks and queued human inputs cannot be bypassed by a claim.
/// 认领不能越过繁忙目标、较早代理任务及排队用户输入。
#[test]
fn task_message_claim_preserves_execution_and_queue_priority() {
    let (_, mut target, first) = fixture(CollaborationTaskMessageKind::TaskStart);
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept-first").unwrap(),
            first.clone(),
            3,
        )
        .unwrap();
    let before = target.clone();
    assert!(
        target
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("claim-first").unwrap(),
                first.delivery_id.clone(),
                4
            )
            .is_err()
    );
    assert_eq!(target, before);
    let mut later = first.clone();
    later.delivery_id = CollaborationDeliveryId::try_new("later").unwrap();
    later.operation_id = CollaborationOperationId::try_new("send-later").unwrap();
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept-later").unwrap(),
            later.clone(),
            4,
        )
        .unwrap();
    target.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    let before = target.clone();
    assert!(
        target
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("claim-later").unwrap(),
                later.delivery_id,
                6
            )
            .is_err()
    );
    assert_eq!(target, before);
    target
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("human-queue").unwrap(),
                "Human next task",
                vec![],
            ),
            6,
        )
        .unwrap();
    let before = target.clone();
    assert!(
        target
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("claim-first").unwrap(),
                first.delivery_id,
                7
            )
            .is_err()
    );
    assert_eq!(target, before);
}

/// A collision found after staging the claim cannot leave a turn or receipt, and tampered anchors fail reload.
/// 暂存认领后发现的冲突不能留下回合或回执，篡改锚点必须导致重载失败。
#[test]
fn task_message_claim_rolls_back_input_collision_and_rejects_tampered_anchor() {
    let (_, mut target, message) = fixture(CollaborationTaskMessageKind::TaskStart);
    target
        .append_in_turn_guidance_before_api(
            ClientActionId::try_new(format!("agent-message:{}", message.delivery_id.as_str()))
                .unwrap(),
            "Earlier unrelated guidance".into(),
            3,
        )
        .unwrap();
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept").unwrap(),
            message.clone(),
            4,
        )
        .unwrap();
    target.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    let before = target.clone();
    assert!(
        target
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("claim").unwrap(),
                message.delivery_id,
                6
            )
            .is_err()
    );
    assert_eq!(target, before);
    let (mut valid, message) = idle_fixture();
    let claim = valid
        .claim_task_message_before_api(
            CollaborationOperationId::try_new("claim").unwrap(),
            message.delivery_id,
            5,
        )
        .unwrap()
        .into_value();
    let mut events = valid.events().to_vec();
    events
        .iter_mut()
        .find(|event| event.seq == claim.event_seq)
        .unwrap()
        .turn_id = None;
    assert!(
        SessionDocument::from_parts(
            valid.metadata.clone(),
            valid.runtime_lock.clone(),
            valid.compactions.clone(),
            valid.segment_catalog().to_vec(),
            events,
        )
        .is_err()
    );
}
