//! Task outbox repair against actual source and target journals.
//! 基于真实来源与目标日志的任务出站修复。

use super::*;
use vulcan_session_core::v1::collaboration::{
    AcceptNotificationInput, CollaborationDeliveryFailureReason, CollaborationDeliveryId,
    CollaborationGrantAction, CollaborationNotificationId, CollaborationNotificationState,
    CollaborationTaskMessage, FailDeliveryInput, FailNotificationInput, ReplyQuestionInput,
    RecordNotificationProgressInput, SendNotificationInput, SetRunControlInput,
};
use vulcan_session_core::v1::document::SessionDocument;

mod embedded_tree;
mod main_business;

/// An accepted cross-project question must fail on both sides when its Ask grant is revoked
/// before the target's next provider request.
/// 跨项目问题已接纳但提问授权在目标下一次供应商请求前撤销时，双方必须记录失败。
#[tokio::test]
async fn accepted_question_revoked_before_projection_settles_both_sides() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let source = invocation.source_handle.clone();
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Ask].into_iter().collect(),
    )
    .await
    .unwrap();
    let asked = ask_question(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation,
        target.session_id(),
        String::from("A question accepted while the grant is valid."),
    )
    .await
    .unwrap();
    assert_eq!(asked["status"], "ok");
    let question_id =
        CollaborationQuestionId::try_new(asked["question_id"].as_str().unwrap()).unwrap();
    let delivery_id = source
        .read(|document| {
            document
                .question_snapshot(&question_id)
                .unwrap()
                .delivery_id
                .clone()
        })
        .unwrap();
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();

    project_collaboration_before_api(&invocation.state, &target, now_ms().unwrap())
        .await
        .expect("revocation must durably fail the accepted question");
    let outbound = source
        .read(|document| document.delivery_snapshot(&delivery_id).unwrap().clone())
        .unwrap();
    let inbound = target
        .read(|document| {
            document
                .inbound_delivery_snapshot(&delivery_id)
                .unwrap()
                .clone()
        })
        .unwrap();
    assert_eq!(outbound.state, CollaborationDeliveryState::Failed);
    assert_eq!(inbound.state, CollaborationDeliveryState::Failed);
    assert_eq!(
        outbound.failure,
        Some(CollaborationDeliveryFailureReason::AuthorizationRevoked)
    );
    assert_eq!(inbound.failure, outbound.failure);
    assert_eq!(outbound.target_revision, Some(inbound.target_revision));
    assert!(outbound.target_event_seq.is_some());
    assert!(!target
        .read(|document| document.events().iter().any(|event| matches!(
            &event.event,
            EventBody::UserInput(input)
                if input.input_source == vulcan_session_core::v1::event::UserInputSource::AgentMessage
        )))
        .unwrap());
    let waited = wait_question(&invocation, question_id.as_str().to_owned(), None, 0)
        .await
        .expect("the original author can inspect a failed delivery after revocation");
    assert_eq!(waited["status"], "failed");
    assert_eq!(waited["delivery"]["failure"], "authorization_revoked");
    let mut cancel_invocation = invocation.clone();
    cancel_invocation.operation_id = format!("v1:{}:cancel-after-revoke", invocation.source_session_id);
    let cancelled = cancel_question(
        &cancel_invocation,
        question_id.as_str().to_owned(),
        String::from("Delivery failed."),
    )
    .await
    .unwrap();
    assert_eq!(cancelled["question_state"], "cancelled");
    assert!(
        !source
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    assert!(
        !target
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
}

/// Recovery must copy the target's durable failure when the source acknowledgement was interrupted.
/// 来源确认中断后，恢复必须复制目标的可靠失败回执。
#[tokio::test]
async fn failed_accepted_question_repairs_missing_source_progress() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let source = invocation.source_handle.clone();
    crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Ask].into_iter().collect(),
    )
    .await
    .unwrap();
    let asked = ask_question(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation,
        target.session_id(),
        String::from("A question whose failure acknowledgement is interrupted."),
    )
    .await
    .unwrap();
    let question_id =
        CollaborationQuestionId::try_new(asked["question_id"].as_str().unwrap()).unwrap();
    let delivery_id = source
        .read(|document| {
            document
                .question_snapshot(&question_id)
                .unwrap()
                .delivery_id
                .clone()
        })
        .unwrap();
    let failed = target
        .mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.fail_delivery(FailDeliveryInput {
                    operation_id: CollaborationOperationId::try_new("interrupted-target-failure")
                        .unwrap(),
                    delivery_id: delivery_id.clone(),
                    question_id: question_id.clone(),
                    reason: CollaborationDeliveryFailureReason::AuthorizationRevoked,
                    failed_at_ms: now_ms().unwrap(),
                })
            },
        )
        .unwrap()
        .value
        .into_value();
    assert_eq!(
        source
            .read(|document| document.delivery_snapshot(&delivery_id).unwrap().state)
            .unwrap(),
        CollaborationDeliveryState::Accepted
    );
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &crate::ServiceEventEmitter::new(vec![]),
            invocation.state.clone(),
            invocation.source_session_id,
        )
        .await
        .unwrap(),
        1
    );
    let recovered = source
        .read(|document| document.delivery_snapshot(&delivery_id).unwrap().clone())
        .unwrap();
    assert_eq!(recovered.state, CollaborationDeliveryState::Failed);
    assert_eq!(
        recovered.failure,
        Some(CollaborationDeliveryFailureReason::AuthorizationRevoked)
    );
    assert_eq!(recovered.target_revision, Some(failed.revision));
    assert_eq!(recovered.target_event_seq, Some(failed.event_seq));
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &crate::ServiceEventEmitter::new(vec![]),
            invocation.state.clone(),
            invocation.source_session_id,
        )
        .await
        .unwrap(),
        0
    );
}

/// An already answered cross-project question must close without showing its answer after Reply revocation.
/// 跨项目问题已回答后撤销回复授权时，必须结清交付且不展示答案正文。
#[tokio::test]
async fn answered_question_revoked_before_model_delivery_redacts_failure() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let source = invocation.source_handle.clone();
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Ask, CollaborationGrantAction::Reply]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap();
    let asked = ask_question(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation,
        target.session_id(),
        String::from("Answer this before the reply grant is removed."),
    )
    .await
    .unwrap();
    let question_id =
        CollaborationQuestionId::try_new(asked["question_id"].as_str().unwrap()).unwrap();
    let (question, delivery, _) = read_question_source(&source, &question_id).unwrap();
    let answer_body = "private answer withheld after revocation";
    let reply_operation = CollaborationOperationId::try_new("answer-before-revoke").unwrap();
    let answered = source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reply_question(ReplyQuestionInput {
                    operation_id: reply_operation.clone(),
                    question_id: question_id.clone(),
                    responder: question.target.clone(),
                    body: answer_body.into(),
                    answered_at_ms: now_ms().unwrap(),
                })
            },
        )
        .unwrap()
        .value
        .into_value();
    let mut settlement_invocation = invocation.clone();
    settlement_invocation.operation_id =
        format!("v1:{}:answer-before-revoke-settlement", invocation.source_session_id);
    settle_and_acknowledge_delivery(
        &settlement_invocation,
        &source,
        invocation.source_session_id,
        &question,
        &delivery,
        reply_operation,
        answered.event_seq,
        CollaborationTargetSettlementState::Answered,
    )
    .await
    .unwrap();
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();

    project_collaboration_before_api(&invocation.state, &source, now_ms().unwrap())
        .await
        .expect("revoked answer delivery must become a durable failure");
    let withheld = source
        .read(|document| document.question_snapshot(&question_id).unwrap().clone())
        .unwrap();
    let answer_delivery = &withheld.answer.as_ref().unwrap().delivery;
    assert_eq!(answer_delivery.state, CollaborationAnswerDeliveryState::Failed);
    assert_eq!(
        answer_delivery.failure,
        Some(CollaborationDeliveryFailureReason::AuthorizationRevoked)
    );
    assert!(!source
        .read(|document| document.events().iter().any(|event| matches!(
            &event.event,
            EventBody::UserInput(input)
                if input.input_source == vulcan_session_core::v1::event::UserInputSource::AgentMessage
        )))
        .unwrap());
    let waited = wait_question(&invocation, question_id.as_str().to_owned(), None, 0)
        .await
        .expect("the original source must see redacted failure metadata");
    assert_eq!(waited["status"], "failed");
    assert_eq!(waited["answer"]["delivery"]["failure"], "authorization_revoked");
    assert!(!waited.to_string().contains(answer_body));
    assert!(!source
        .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
        .unwrap());
    assert!(!target
        .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
        .unwrap());
    let reloaded = load_session_v1_for_runtime(&source.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        reloaded.question_snapshot(&question_id).unwrap().answer.as_ref().unwrap().delivery.state,
        CollaborationAnswerDeliveryState::Failed
    );
}

/// A notification stranded between source commit and target acceptance must be repairable.
/// 停留在来源提交与目标接纳之间的通知必须可被修复。
///
/// # Notes
/// 说明
/// `send_notification` commits the source record first and only then accepts it on the target, so a
/// process loss in that window leaves a durable notification the target never saw — exactly the window
/// A12 names. This test records that window through the real domain creation and requires the outbox
/// pass to close it.
/// `send_notification` 先提交来源记录，之后才在目标上接纳，因此该窗口内的进程丢失会留下目标
/// 从未见过的可靠通知——这正是 A12 点名的窗口。本测试通过真实领域创建复现该窗口，并要求出站
/// 修复把它闭合。
///
/// # Requirements
/// 要求
/// Recovery must accept the notification on the target and advance the source to `Accepted`, keeping
/// the same stable notification, delivery and operation identities so a repeat cannot double-accept.
/// 恢复必须在目标上接纳该通知、并把来源推进到 `Accepted`，且沿用同一稳定通知、投递与操作身份，
/// 使重复执行不会二次接纳。
#[tokio::test]
async fn notification_stranded_before_acceptance_is_repaired_by_the_outbox_pass() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let source = invocation.source_handle.clone();
    let source_turn = source
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    let target_turn = target
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    // Both participant coordinates come from the actual durable documents, not from labels.
    // 双方参与者坐标取自实际可靠文档，而非标签。
    let (source_participant, target_participant) = {
        let source_participant = source
            .read(|document| {
                CollaborationParticipant::new(
                    source.session_id().to_string(),
                    source.session_id().to_string(),
                    invocation.source_locator.project_id(),
                    document.segment_generation(),
                    Some(source_turn),
                    None,
                )
                .unwrap()
            })
            .unwrap();
        let target_participant = target
            .read(|document| {
                CollaborationParticipant::new(
                    target.session_id().to_string(),
                    target.session_id().to_string(),
                    invocation.source_locator.project_id(),
                    document.segment_generation(),
                    Some(target_turn),
                    None,
                )
                .unwrap()
            })
            .unwrap();
        (source_participant, target_participant)
    };
    let notification_id = CollaborationNotificationId::try_new("stranded-notification").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("stranded-delivery").unwrap();
    let notification_body = "Phase one finished; no reply needed.";
    // Commit only the source half through the real domain creation, reproducing the crash window exactly.
    // 只通过真实领域创建提交来源一半，精确复现崩溃窗口。
    let created = source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.send_notification(SendNotificationInput {
                    operation_id: CollaborationOperationId::try_new("stranded-send").unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: notification_body.into(),
                    created_at_ms: 5,
                })
            },
        )
        .unwrap()
        .value
        .into_value();
    assert_eq!(
        created.notification.state,
        CollaborationNotificationState::Recorded,
        "a freshly sent notification starts recorded and not yet accepted"
    );
    // The crasher left the source owning the notification while the target owns nothing for it.
    // 崩溃留下的状态是：来源持有通知，而目标对它一无所知。
    let stranded = source
        .read(|document| {
            document
                .collaboration_state()
                .notifications
                .values()
                .find(|record| record.notification_id == notification_id)
                .cloned()
        })
        .unwrap()
        .expect("the stranded notification must exist on its source");
    assert_eq!(stranded.state, CollaborationNotificationState::Recorded);
    assert_eq!(stranded.delivery_id, delivery_id);
    assert_eq!(stranded.body, notification_body);
    assert_eq!(stranded.source, source_participant);
    assert_eq!(stranded.target, target_participant);
    assert_eq!(stranded.target_revision, None);
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_notifications
                .get(&notification_id)
                .is_none())
            .unwrap(),
        "the crash window must leave the target without the notification"
    );
    // The stranding is a live obligation: an unnotified target blocks destructive lifecycle work.
    // 该滞留是活动义务：未收到通知的目标会阻塞破坏性生命周期操作。
    assert!(
        source
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    // The real outbox pass must close the window rather than skipping notifications.
    // 真实出站修复必须闭合该窗口，而不是跳过通知。
    let repaired = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect("outbox reconciliation must repair a stranded notification");
    assert_eq!(
        repaired, 1,
        "the pass must report exactly the one notification it repaired"
    );
    // The target must now own the acceptance for the exact same stable identity.
    // 目标现在必须为同一稳定身份持有接纳记录。
    let inbound = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_notifications
                .get(&notification_id)
                .cloned()
        })
        .unwrap()
        .expect("recovery must accept the notification on its target");
    assert_eq!(inbound.notification_id, notification_id);
    assert_eq!(inbound.delivery_id, delivery_id);
    assert_eq!(inbound.body, notification_body);
    // The source must have advanced to accepted while retaining the recorded body authority.
    // 来源必须推进到已接纳，同时保留已记录正文权威。
    let recovered = source
        .read(|document| {
            document
                .collaboration_state()
                .notifications
                .get(&notification_id)
                .cloned()
        })
        .unwrap()
        .expect("the recovered notification must survive on its source");
    assert_eq!(recovered.state, CollaborationNotificationState::Accepted);
    assert_eq!(recovered.body, notification_body);
    assert_eq!(recovered.delivery_id, delivery_id);
    // The obligation legitimately persists: an accepted-but-unprojected notification has not reached
    // the target's model yet, so recovery must not pretend the target was informed.
    // 该义务合理地继续存在：已接纳但未投影的通知尚未到达目标模型，因此恢复不得假装目标已知情。
    assert!(
        source
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap(),
        "an accepted but unprojected notification must keep blocking destructive lifecycle work"
    );
    assert_eq!(
        inbound.state,
        CollaborationNotificationState::Accepted,
        "recovery settles the target at accepted, not at projected"
    );
    // A second pass must be a no-op: recovery may not double-accept the same identity.
    // 第二次修复必须无操作：恢复不得对同一身份二次接纳。
    let second = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect("a repeated pass must stay clean");
    assert_eq!(
        second, 0,
        "an already accepted notification must not be repaired twice"
    );
    let inbound_count = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_notifications
                .keys()
                .filter(|key| **key == notification_id)
                .count()
        })
        .unwrap();
    assert_eq!(inbound_count, 1, "exactly one target acceptance may exist");
}

/// A target projection survives even when the later source progress write is lost.
/// 即使后续来源进度写入丢失，目标投影仍已可靠存在。
/// Recovery must copy that exact target proof to the source without projecting the body twice.
/// 恢复必须将目标的精确凭据补回来源，且不能重复投影正文。
#[tokio::test]
async fn projected_notification_with_missing_source_progress_is_repaired() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let source = invocation.source_handle.clone();
    let send = notifications::send_notification(
        &invocation,
        target.session_id(),
        String::from("The child completed a stage; no answer is needed."),
    )
    .await
    .expect("the notification must be accepted before target projection");
    assert_eq!(send["status"], "accepted");
    let notification_id = source
        .read(|document| {
            document
                .collaboration_state()
                .notifications
                .keys()
                .next()
                .cloned()
        })
        .unwrap()
        .expect("the source must retain the notification identity");
    target
        .mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.project_notification_before_api(notification_id.clone(), now_ms().unwrap())
            },
        )
        .expect("the target must project the accepted notification");
    assert_eq!(
        source
            .read(|document| document.collaboration_state().notifications[&notification_id].state)
            .unwrap(),
        CollaborationNotificationState::Accepted,
        "the crash window leaves only the source progress write missing"
    );
    let target_proof = target
        .read(|document| {
            document.collaboration_state().inbound_notifications[&notification_id].clone()
        })
        .unwrap();
    assert_eq!(
        target_proof.state,
        CollaborationNotificationState::Projected
    );
    let repaired = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect("the recovery pass must reconcile the already projected target");
    assert_eq!(repaired, 1);
    let source_proof = source
        .read(|document| document.collaboration_state().notifications[&notification_id].clone())
        .unwrap();
    assert_eq!(
        source_proof.state,
        CollaborationNotificationState::Projected
    );
    assert_eq!(
        source_proof.target_revision,
        Some(target_proof.target_revision)
    );
    assert!(source_proof.target_event_seq.is_some());
    assert_eq!(
        target
            .read(|document| document.collaboration_state().inbound_notifications.len())
            .unwrap(),
        1,
        "repair must not add another target delivery"
    );
}

/// A revoked grant cannot undo an acceptance already committed while that grant was valid.
/// 已有效授权下提交的接纳事实不能被后来撤销的授权抹去。
/// Recovery must acknowledge the target's existing receipt without attempting another delivery.
/// 恢复必须确认目标现有回执，而不能再次尝试投递。
#[tokio::test]
async fn accepted_notification_progress_survives_later_grant_revocation() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let source = invocation.source_handle.clone();
    let (source_participant, target_participant) =
        notification_participants(&invocation, &source, &target).await;
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Notify].into_iter().collect(),
    )
    .await
    .unwrap();
    let notification_id = CollaborationNotificationId::try_new("accepted-before-revoke").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("accepted-before-revoke-delivery").unwrap();
    let operation_id = CollaborationOperationId::try_new("accepted-before-revoke-send").unwrap();
    let body = String::from("Delivered while the notify grant was valid.");
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.send_notification(SendNotificationInput {
                    operation_id: operation_id.clone(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: body.clone(),
                    created_at_ms: 5,
                })
            },
        )
        .unwrap();
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.accept_notification(AcceptNotificationInput {
                    operation_id: CollaborationOperationId::try_new(
                        "accepted-before-revoke-accept",
                    )
                    .unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source_operation_id: operation_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: body.clone(),
                    accepted_at_ms: 6,
                })
            },
        )
        .unwrap();
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    let repaired = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect("the target's earlier acceptance is sufficient to repair source progress");
    assert_eq!(repaired, 1);
    assert_eq!(
        source
            .read(|document| document.collaboration_state().notifications[&notification_id].state)
            .unwrap(),
        CollaborationNotificationState::Accepted
    );
    assert_eq!(
        target
            .read(|document| document.collaboration_state().inbound_notifications.len())
            .unwrap(),
        1,
        "recovery must not accept the same notification twice"
    );
    // Revocation before the target's next API must settle the accepted body as a failed delivery,
    // so neither document retains an unresolvable edit/delete obligation.
    // 目标下一次 API 前撤销授权时，必须把已接纳正文结算为失败投递，避免两侧文档留下无法解除的
    // 编辑或删除义务。
    project_collaboration_before_api(&invocation.state, &target, now_ms().unwrap())
        .await
        .expect("revoked accepted notification must settle without exposing its body");
    assert_eq!(
        source
            .read(|document| document.collaboration_state().notifications[&notification_id].state)
            .unwrap(),
        CollaborationNotificationState::Failed
    );
    assert_eq!(
        target
            .read(
                |document| document.collaboration_state().inbound_notifications[&notification_id]
                    .state
            )
            .unwrap(),
        CollaborationNotificationState::Failed
    );
    assert!(
        !source
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    assert!(
        !target
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
}

/// A crash after target failure but before source progress must not strand the accepted outbox.
/// 目标失败已落盘但来源进度未写入时，重启恢复不得让已接纳出站记录滞留。
#[tokio::test]
async fn failed_target_notification_repairs_missing_source_progress() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let source = invocation.source_handle.clone();
    let (source_participant, target_participant) =
        notification_participants(&invocation, &source, &target).await;
    let notification_id = CollaborationNotificationId::try_new("failed-before-source").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("failed-before-source-delivery").unwrap();
    let operation_id = CollaborationOperationId::try_new("failed-before-source-send").unwrap();
    let body = String::from("A notification awaiting target projection.");
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.send_notification(SendNotificationInput {
                    operation_id: operation_id.clone(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: body.clone(),
                    created_at_ms: 5,
                })
            },
        )
        .unwrap();
    let accepted = target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.accept_notification(AcceptNotificationInput {
                    operation_id: CollaborationOperationId::try_new(
                        "failed-before-source-delivery:notification-accept",
                    )
                    .unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source_operation_id: operation_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: body.clone(),
                    accepted_at_ms: 6,
                })
            },
        )
        .unwrap();
    let accepted = accepted.value.into_value();
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.record_notification_progress(RecordNotificationProgressInput {
                    operation_id: CollaborationOperationId::try_new(
                        "failed-before-source-send:notification-accepted",
                    )
                    .unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    state: CollaborationNotificationState::Accepted,
                    failure: None,
                    target_revision: Some(accepted.notification.target_revision),
                    target_event_seq: Some(accepted.event_seq),
                    updated_at_ms: 7,
                })
            },
        )
        .unwrap();
    target
        .mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.fail_notification(FailNotificationInput {
                    operation_id: CollaborationOperationId::try_new(
                        "failed-before-source-delivery:fail",
                    )
                    .unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    reason: CollaborationDeliveryFailureReason::InvalidBinding,
                    failed_at_ms: 8,
                })
            },
        )
        .unwrap();
    assert_eq!(
        source
            .read(|document| document.collaboration_state().notifications[&notification_id].state)
            .unwrap(),
        CollaborationNotificationState::Accepted
    );
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &crate::ServiceEventEmitter::new(vec![]),
            invocation.state.clone(),
            invocation.source_session_id,
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        source
            .read(|document| {
                let record = &document.collaboration_state().notifications[&notification_id];
                (record.state, record.failure)
            })
            .unwrap(),
        (
            CollaborationNotificationState::Failed,
            Some(CollaborationDeliveryFailureReason::InvalidBinding)
        )
    );
}

/// A failed indexed pass retains its worker and retries the actual outbox after the index becomes available.
/// 索引扫描失败后保留任务，并在索引恢复可用后重试真实出站箱。
/// Duplicate startup cannot claim another worker; success releases its registry reservation.
/// 重复启动不能认领第二个任务；成功后释放注册表预留。
#[tokio::test]
async fn session_collaboration_startup_worker_retries_real_outbox() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    // Keep the target physically active: this test exercises outbox retry, not cold business dispatch.
    // 保持目标物理活动：本测试验证出站重试，而非冷业务分发。
    start_api(&target);
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "retry-worker",
    )
    .await;
    // Temporarily withhold the published index, reproducing an actual service availability error.
    // 临时撤下已发布索引，复现真实服务可用性错误。
    let workspace = invocation.state.lock().unwrap().workspace_snapshot.take();
    let app = crate::ServiceEventEmitter::new(vec![]);
    let worker = startup_recovery::worker::schedule(&invocation.state, app.clone())
        .unwrap()
        .unwrap();
    assert!(
        startup_recovery::worker::schedule(&invocation.state, app)
            .unwrap()
            .is_none()
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if invocation
                .state
                .lock()
                .unwrap()
                .startup
                .recent_logs
                .iter()
                .any(|line| {
                    line.contains("Collaboration recovery requires a published workspace index")
                })
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_task_messages
                .is_empty())
            .unwrap()
    );
    invocation.state.lock().unwrap().workspace_snapshot = workspace;
    tokio::time::timeout(std::time::Duration::from_secs(8), worker)
        .await
        .unwrap()
        .unwrap();
    // Read both real journals so a resident-only acknowledgement cannot satisfy recovery.
    // 读取双方真实日志，防止仅驻留内存的确认满足恢复断言。
    let source = load_session_v1_for_runtime(&invocation.source_locator)
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let acceptance = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(acceptance)
    );
    assert!(
        invocation
            .registry
            .try_reserve_collaboration_recovery()
            .unwrap()
            .is_some()
    );
}

/// Shutdown suspends scans, cancellation releases ownership, and reopening admission allows real repair.
/// 关闭暂停扫描，取消释放所有权，重新开放准入后允许真实修复。
/// Uses the registry shutdown fence rather than fabricating a completed worker or canonical receipt.
/// 使用注册表关闭栅栏，不伪造已完成任务或规范回执。
#[tokio::test]
async fn session_collaboration_startup_worker_obeys_shutdown_and_cancellation() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    // A live physical owner leaves only delivery work for this lifecycle test.
    // 活动物理所有者使此生命周期测试仅保留投递待办。
    start_api(&target);
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "shutdown-worker",
    )
    .await;
    invocation.registry.begin_shutdown().unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    let cancelled = startup_recovery::worker::schedule(&invocation.state, app.clone())
        .unwrap()
        .unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    let resumed = startup_recovery::worker::schedule(&invocation.state, app)
        .unwrap()
        .unwrap();
    // Hold the shutdown fence over the worker's initial attempt, then reopen the exact same registry.
    // 在任务初次尝试期间保持关闭栅栏，再开放精确同一注册表。
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!resumed.is_finished());
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_task_messages
                .is_empty())
            .unwrap()
    );
    invocation.registry.resume_after_failed_shutdown().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(8), resumed)
        .await
        .unwrap()
        .unwrap();
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_task_messages
                .contains_key(&message.delivery_id))
            .unwrap()
    );
    assert!(
        invocation
            .registry
            .try_reserve_collaboration_recovery()
            .unwrap()
            .is_some()
    );
}

/// An indexed startup pass leaves unrelated cold histories unloaded.
/// 索引启动扫描保持无关冷历史未加载。
#[tokio::test]
async fn session_collaboration_startup_skips_empty_cold_sessions() {
    // Discard every canonical handle to reproduce a process with only an indexed workspace.
    // 丢弃所有规范句柄，复现只有索引工作区的进程。
    let (_root, invocation, target) = fixture(false).await;
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [invocation.source_session_id, target.session_id()]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap();
    assert!(registry.get(invocation.source_session_id).is_err());
    assert!(registry.get(target.session_id()).is_err());
}

/// Cold indexed repair reloads persisted intents and isolates an unavailable index entry.
/// 冷索引修复重载持久意图，并隔离不可用的索引条目。
#[tokio::test]
async fn session_collaboration_startup_repairs_cold_outbox_despite_unavailable_peer() {
    // The new registry cannot use either original live handle as a recovery shortcut.
    // 新注册表不能将任何原活动句柄作为恢复捷径。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "startup",
    )
    .await;
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    let missing = Uuid::new_v4();
    let failure = startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [missing, invocation.source_session_id]
            .into_iter()
            .collect(),
    )
    .await
    .unwrap_err();
    assert!(failure.contains(&missing.to_string()));
    let source = load_session_v1_for_runtime(&invocation.source_locator)
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let accepted = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(accepted)
    );
    assert_eq!(accepted.message, message);
    assert!(
        receiver
            .collaboration_state()
            .task_message_effects
            .is_empty()
    );
    assert!(!Arc::ptr_eq(
        &registry.get(invocation.source_session_id).unwrap(),
        &invocation.source_handle
    ));
}

/// An unpublished receiver receipt cannot become a durable source acknowledgement.
/// 尚未发布的接收方回执不能成为可靠来源确认。
#[tokio::test]
async fn session_task_outbox_recovery_rejects_unpublished_acceptance() {
    // Keep both real stores alive while simulating a receiver with an unflushed acceptance.
    // 模拟接收方尚未刷写接纳期间保留双方真实存储。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "unpublished",
    )
    .await;
    target
        .mutate_in_memory_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_task_message(
                        CollaborationOperationId::try_new("unpublished-acceptance").unwrap(),
                        message.clone(),
                        6,
                    )
                    .map(session_change)
            },
        )
        .unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap_err()
        .contains("not durably published")
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .is_none()
    );
    let before = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        !before
            .collaboration_state()
            .inbound_task_messages
            .contains_key(&message.delivery_id)
    );
    target.flush_all().unwrap();
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap(),
        1
    );
}

/// Publishes only the source intent to reproduce a crash before target delivery.
/// 仅发布来源意图，复现目标投递前崩溃。
/// `label` distinguishes operations; `kind` controls whether the original target turn is frozen.
/// `label` 区分操作；`kind` 决定是否冻结原目标回合。
/// Returns the exact stored message without accepting or scheduling it.
/// 返回精确存储消息，不接纳或调度它。
async fn record_pending_task(
    invocation: &ValidatedInvocation,
    target: &Arc<SessionHandle>,
    kind: CollaborationTaskMessageKind,
    label: &str,
) -> CollaborationTaskMessage {
    // Participants come from the same canonical resolver as normal dispatch.
    // 参与者来自与正常分发相同的规范解析器。
    let source = source_participant(invocation).await.unwrap();
    let target_turn = match kind {
        CollaborationTaskMessageKind::TaskStart => None,
        CollaborationTaskMessageKind::TaskSteer => Some(
            target
                .read(|document| document.active_turn().unwrap().turn_id.value())
                .unwrap(),
        ),
    };
    let receiver =
        participants::participant_for_handle(invocation, target.session_id(), target, target_turn)
            .await
            .unwrap();
    let message = CollaborationTaskMessage {
        delivery_id: CollaborationDeliveryId::try_new(format!("recover-delivery-{label}")).unwrap(),
        operation_id: CollaborationOperationId::try_new(format!("recover-source-{label}")).unwrap(),
        source,
        target: receiver,
        kind,
        body: format!("Durable task {label}"),
    };
    invocation
        .source_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .send_task_message(message.clone(), 5)
                    .map(session_change)
            },
        )
        .unwrap();
    message
}

/// Repairs after the original source API ends, and repeated repair changes neither journal.
/// 原来源 API 结束后仍可修复，重复修复不改变双方日志。
#[tokio::test]
async fn session_task_outbox_recovery_preserves_original_intent_after_source_api() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "original",
    )
    .await;
    invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| document.interrupt_active_api(6),
        )
        .unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap(),
        1
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let acceptance = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(acceptance.message, message);
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(acceptance)
    );
    assert!(
        receiver
            .collaboration_state()
            .task_message_effects
            .is_empty()
    );
    let source_revision = source.durable_revision();
    let target_revision = receiver.durable_revision();
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        invocation
            .source_handle
            .read(|document| document.durable_revision())
            .unwrap(),
        source_revision
    );
    assert_eq!(
        target.read(|document| document.durable_revision()).unwrap(),
        target_revision
    );
}

/// One obsolete steering delivery cannot block another queued intent or bind a replacement task.
/// 一条过期方向投递不能阻止另一条排队意图，也不能绑定替代任务。
#[tokio::test]
async fn session_task_outbox_recovery_isolates_obsolete_turns() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let obsolete = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "a-obsolete",
    )
    .await;
    let queued = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskStart,
        "b-queued",
    )
    .await;
    target
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.close_turn(vulcan_session_core::v1::event::TurnOutcome::Completed, 0, 6)
            },
        )
        .unwrap();
    let successor = target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("recovery-successor").unwrap(),
                "Unrelated work",
                vec![],
            ),
            7,
        )
        .unwrap()
        .value;
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap_err()
        .contains("target_turn")
    );
    target
        .read(|document| {
            assert_eq!(document.active_turn().unwrap().turn_id, successor);
            assert!(
                !document
                    .collaboration_state()
                    .inbound_task_messages
                    .contains_key(&obsolete.delivery_id)
            );
            assert_eq!(
                document.collaboration_state().inbound_task_messages[&queued.delivery_id].message,
                queued
            );
            assert!(
                document
                    .collaboration_state()
                    .task_message_effects
                    .is_empty()
            );
        })
        .unwrap();
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        source.collaboration_state().outbound_task_messages[&obsolete.delivery_id]
            .acceptance
            .is_none()
    );
    assert!(
        source.collaboration_state().outbound_task_messages[&queued.delivery_id]
            .acceptance
            .is_some()
    );
}

/// Revocation blocks first delivery but permits copying a preexisting acceptance without new input.
/// 撤销授权阻止首次投递，但允许复制已有接纳且不新增输入。
/// Builds both participant coordinates from the actual durable documents, never from labels.
/// 从实际可靠文档构建双方参与者坐标，绝不依赖标签。
///
/// # Notes
/// 说明
/// Each participant takes its project from its **own** locator, so a cross-project pair keeps the
/// genuinely different scopes that authorize a cross-project delivery.
/// 每个参与者的项目取自**自身**定位器，因此跨项目组合保留真正不同的项目范围，跨项目投递正是
/// 依此授权。
async fn notification_participants(
    invocation: &ValidatedInvocation,
    source: &Arc<SessionHandle>,
    target: &Arc<SessionHandle>,
) -> (CollaborationParticipant, CollaborationParticipant) {
    let source_project_id = invocation.source_locator.project_id().to_string();
    let target_project_id = target
        .storage_locator()
        .map_err(|error| error.to_string())
        .unwrap()
        .project_id()
        .to_string();
    let source_participant = source
        .read(|document| {
            CollaborationParticipant::new(
                source.session_id().to_string(),
                source.session_id().to_string(),
                source_project_id.clone(),
                document.segment_generation(),
                document
                    .active_turn()
                    .filter(|turn| !turn.closed)
                    .map(|turn| turn.turn_id),
                None,
            )
            .unwrap()
        })
        .unwrap();
    let target_participant = target
        .read(|document| {
            CollaborationParticipant::new(
                target.session_id().to_string(),
                target.session_id().to_string(),
                target_project_id.clone(),
                document.segment_generation(),
                document
                    .active_turn()
                    .filter(|turn| !turn.closed)
                    .map(|turn| turn.turn_id),
                None,
            )
            .unwrap()
        })
        .unwrap();
    (source_participant, target_participant)
}

/// A notification stranded before acceptance must not be forced onto a target that has since stopped.
/// 停留在接纳之前的通知不得被强行投递给此后已停止的目标。
///
/// # Notes
/// 说明
/// The stranded window can be entered while the target is running and only closed after the target
/// stopped. Recovery must therefore re-evaluate the target's durable run control instead of replaying
/// the acceptance blindly, so a stop genuinely protects the stopped agent (A10, A12).
/// 滞留窗口可能在目标仍在运行时进入，而在目标停止之后才被修复。因此恢复必须重新判定目标的可靠
/// 运行控制，而不是盲目重放接纳，停止才能真正保护已停止的代理（A10、A12）。
#[tokio::test]
async fn stranded_notification_is_refused_after_the_target_stops() {
    let (_root, invocation, target, _) = message_fixture(false).await;
    let source = invocation.source_handle.clone();
    let (source_participant, target_participant) =
        notification_participants(&invocation, &source, &target).await;
    let notification_id = CollaborationNotificationId::try_new("stopped-notification").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("stopped-delivery").unwrap();
    let body = "Recorded while the target was running.";
    // The source half is committed while the target is still running, which is the legal send window.
    // 来源一半在目标仍在运行时提交，这正是合法的发送窗口。
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.send_notification(SendNotificationInput {
                    operation_id: CollaborationOperationId::try_new("stopped-send").unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: body.into(),
                    created_at_ms: 5,
                })
            },
        )
        .unwrap();
    // The target then stops before the acceptance half ever ran.
    // 目标在接纳一半尚未执行之前停止。
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("stopped-control").unwrap(),
                    state: CollaborationRunControl::Stopped,
                    changed_at_ms: 6,
                })
            },
        )
        .unwrap();
    // Recovery must report the refusal instead of claiming repair or silently dropping the record.
    // 恢复必须报告拒绝，而不是声称已修复或静默丢弃该记录。
    let failure = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect_err("recovery must not deliver to a stopped target");
    assert!(
        failure.contains(notification_id.as_str()),
        "the refusal must name the exact notification it could not deliver: {failure}"
    );
    // The stopped target must remain untouched by the attempted delivery.
    // 已停止的目标必须未被本次投递尝试改动。
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_notifications
                .get(&notification_id)
                .is_none())
            .unwrap(),
        "a stopped target must not receive the notification"
    );
    // The source keeps the recorded fact so a later pass can still deliver it.
    // 来源保留已记录事实，使后续修复仍可投递。
    let retained = source
        .read(|document| {
            document
                .collaboration_state()
                .notifications
                .get(&notification_id)
                .cloned()
        })
        .unwrap()
        .expect("the recorded notification must survive a refused recovery");
    assert_eq!(retained.state, CollaborationNotificationState::Recorded);
    assert_eq!(retained.body, body);
    // An explicit restart admits the same recorded identity, proving the record was retained intact.
    // 显式恢复运行会接纳同一已记录身份，证明该记录被完整保留。
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("restarted-control").unwrap(),
                    state: CollaborationRunControl::Running,
                    changed_at_ms: 7,
                })
            },
        )
        .unwrap();
    let repaired = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect("a running target must accept the retained notification");
    assert_eq!(
        repaired, 1,
        "the retained notification must be delivered once after restart"
    );
    let inbound = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_notifications
                .get(&notification_id)
                .cloned()
        })
        .unwrap()
        .expect("the restarted target must hold the acceptance");
    assert_eq!(inbound.body, body);
    assert_eq!(inbound.delivery_id, delivery_id);
}

/// A cross-project notification stranded before acceptance must obey a revoked notify grant.
/// 停留在接纳之前的跨项目通知必须遵守已撤销的通知授权。
///
/// # Notes
/// 说明
/// `deliver_recorded_notification` re-evaluates the cross-project `Notify` grant before it accepts on
/// the target. A revoke that lands inside the stranded window must therefore block recovery, and
/// re-granting must deliver the exact same retained record rather than a reconstructed one.
/// `deliver_recorded_notification` 在目标接纳之前会重新判定跨项目 `Notify` 授权。因此落在滞留窗口
/// 内的撤销必须阻断恢复，而重新授权必须投递**同一条**被保留的记录，而不是重建一条。
#[tokio::test]
async fn revoked_cross_project_grant_blocks_stranded_notification_recovery() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let source = invocation.source_handle.clone();
    let (source_participant, target_participant) =
        notification_participants(&invocation, &source, &target).await;
    // Cross-project delivery is not implied by a shared root, so the participant scopes must differ.
    assert_ne!(
        source_participant.project_id, target_participant.project_id,
        "this test requires a genuinely cross-project target"
    );
    // The user grants Notify first, which is the authority the crash window is entered under.
    // 用户先授予 Notify，这正是进入崩溃窗口时所依据的权威。
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Notify].into_iter().collect(),
    )
    .await
    .unwrap();
    let notification_id = CollaborationNotificationId::try_new("revoked-notification").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("revoked-delivery").unwrap();
    let body = "Sent while the cross-project grant was valid.";
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.send_notification(SendNotificationInput {
                    operation_id: CollaborationOperationId::try_new("revoked-send").unwrap(),
                    notification_id: notification_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: body.into(),
                    created_at_ms: 5,
                })
            },
        )
        .unwrap();
    // The user revokes before the acceptance half runs, through the same service surface as a real host.
    // 用户在接纳一半执行之前，通过与真实宿主相同的服务入口撤销授权。
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    // Recovery must refuse instead of delivering under the revoked authority.
    // 恢复必须拒绝，而不是在已撤销的授权下投递。
    let failure = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect_err("recovery must not deliver under a revoked cross-project grant");
    assert!(
        failure.contains(notification_id.as_str()),
        "the refusal must name the exact notification it could not deliver: {failure}"
    );
    // The revoked target must remain untouched.
    // 已撤销授权的目标必须未被改动。
    assert!(
        target
            .read(|document| document
                .collaboration_state()
                .inbound_notifications
                .get(&notification_id)
                .is_none())
            .unwrap(),
        "a revoked grant must keep the notification off the target"
    );
    let retained = source
        .read(|document| {
            document
                .collaboration_state()
                .notifications
                .get(&notification_id)
                .cloned()
        })
        .unwrap()
        .expect("the recorded notification must survive a refused recovery");
    assert_eq!(retained.state, CollaborationNotificationState::Recorded);
    assert_eq!(retained.body, body);
    // Re-granting the same authority delivers that exact retained record.
    // 重新授予同一授权会投递那条被精确保留的记录。
    crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Notify].into_iter().collect(),
    )
    .await
    .unwrap();
    let repaired = reconcile_session_v1_collaboration_outbox(
        &crate::ServiceEventEmitter::new(vec![]),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .expect("a re-granted authority must deliver the retained notification");
    assert_eq!(
        repaired, 1,
        "the retained notification must be delivered exactly once after re-granting"
    );
    let inbound = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_notifications
                .get(&notification_id)
                .cloned()
        })
        .unwrap()
        .expect("the re-granted target must hold the acceptance");
    assert_eq!(inbound.body, body);
    assert_eq!(inbound.delivery_id, delivery_id);
    assert_eq!(
        source
            .read(|document| {
                document
                    .collaboration_state()
                    .notifications
                    .get(&notification_id)
                    .map(|record| record.state)
            })
            .unwrap(),
        Some(CollaborationNotificationState::Accepted)
    );
}

#[tokio::test]
async fn session_task_outbox_recovery_separates_new_delivery_from_receipt_repair() {
    let (_root, invocation, target, _) = message_fixture(true).await;
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Steer].into_iter().collect(),
    )
    .await
    .unwrap();
    let accepted = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "accepted",
    )
    .await;
    let pending = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "pending",
    )
    .await;
    let original = target
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_task_message(
                        CollaborationOperationId::try_new("original-acceptance").unwrap(),
                        accepted.clone(),
                        6,
                    )
                    .map(session_change)
            },
        )
        .unwrap()
        .value;
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    let target_revision = target.read(|document| document.durable_revision()).unwrap();
    let app = crate::ServiceEventEmitter::new(vec![]);
    assert!(
        reconcile_session_v1_collaboration_outbox(
            &app,
            invocation.state.clone(),
            invocation.source_session_id
        )
        .await
        .unwrap_err()
        .contains("authorization was revoked")
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&accepted.delivery_id]
            .acceptance
            .as_ref(),
        Some(&original)
    );
    assert!(
        source.collaboration_state().outbound_task_messages[&pending.delivery_id]
            .acceptance
            .is_none()
    );
    assert_eq!(
        target.read(|document| document.durable_revision()).unwrap(),
        target_revision
    );
}
