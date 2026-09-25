//! Inbox batch priorities and lossless notification deferral.
//! 收件批次优先级与无损通知延后。

use super::*;
use vulcan_session_core::v1::collaboration::{
    AcceptNotificationInput, CollaborationNotificationId,
};
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission, SessionDocument};

/// Creates an open receiver with ordered, durably accepted stage notifications.
/// 创建开放接收方及有序可靠接纳的阶段通知。
/// `bodies` determine sizes; returns the actual domain document without fabricated projection states.
/// `bodies` 决定大小；返回真实领域文档，不伪造投影状态。
fn notification_fixture(bodies: Vec<String>) -> SessionDocument {
    let mut document = SessionDocument::new("receiver", "project", "Receiver", 1);
    let turn = document
        .submit_root_input(
            ClientInputSubmission::new(ClientActionId::try_new("root").unwrap(), "Work", vec![]),
            2,
        )
        .unwrap()
        .into_value();
    for (index, body) in bodies.into_iter().enumerate() {
        document
            .accept_notification(AcceptNotificationInput {
                operation_id: CollaborationOperationId::try_new(format!("accept-{index}")).unwrap(),
                notification_id: CollaborationNotificationId::try_new(format!(
                    "notification-{index}"
                ))
                .unwrap(),
                delivery_id: CollaborationDeliveryId::try_new(format!("delivery-{index}")).unwrap(),
                source_operation_id: CollaborationOperationId::try_new(format!("send-{index}"))
                    .unwrap(),
                source: CollaborationParticipant::new(
                    "sender",
                    "sender",
                    "project",
                    1,
                    Some(turn),
                    None,
                )
                .unwrap(),
                target: CollaborationParticipant::new(
                    "receiver",
                    "receiver",
                    "project",
                    1,
                    Some(turn),
                    None,
                )
                .unwrap(),
                body,
                accepted_at_ms: 3,
            })
            .unwrap();
    }
    document
}

/// A later question outranks stage updates, and deferred updates retain acceptance until the next batch.
/// 后来的问题先于阶段更新；延后更新保持接纳状态直到下一批。
/// Assertions follow identities and declared limits instead of unrelated event positions.
/// 断言依据身份和声明上限，不绑定无关事件位置。
#[test]
fn notification_batch_yields_to_question_and_preserves_overflow() {
    let mut document = notification_fixture(vec!["Stage".into(); NOTIFICATION_BATCH_LIMIT + 2]);
    let turn = document.active_turn().unwrap().turn_id;
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("question-accept").unwrap(),
            question_id: CollaborationQuestionId::try_new("question").unwrap(),
            delivery_id: CollaborationDeliveryId::try_new("question-delivery").unwrap(),
            source: CollaborationParticipant::new(
                "sender",
                "sender",
                "project",
                1,
                Some(turn),
                None,
            )
            .unwrap(),
            target: CollaborationParticipant::new(
                "receiver",
                "receiver",
                "project",
                1,
                Some(turn),
                None,
            )
            .unwrap(),
            bound_turn_id: Some(turn),
            execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
            accepted_at_ms: 4,
        })
        .unwrap();
    let selected = pending_projections(&document).unwrap();
    assert!(matches!(
        selected.first(),
        Some(PendingProjection::Question(_))
    ));
    let notifications: Vec<_> = selected
        .into_iter()
        .filter_map(|entry| match entry {
            PendingProjection::Notification(record) => Some(record),
            _ => None,
        })
        .collect();
    assert_eq!(notifications.len(), NOTIFICATION_BATCH_LIMIT);
    for notification in notifications {
        document
            .project_notification_before_api(notification.notification_id, 5)
            .unwrap();
    }
    let remaining = document
        .collaboration_state()
        .inbound_notifications
        .values()
        .filter(|record| record.state == CollaborationNotificationState::Accepted)
        .count();
    assert_eq!(remaining, 2);
    assert_eq!(
        pending_projections(&document)
            .unwrap()
            .into_iter()
            .filter(|entry| matches!(entry, PendingProjection::Notification(_)))
            .count(),
        remaining
    );
}

/// The shared byte budget defers whole notifications and preserves their order without truncation.
/// 共享字节预算延后完整通知，保留顺序，不截断正文。
/// A maximum-sized first message still makes progress; later small updates cannot overtake it.
/// 最大首条消息仍可推进，后来的小更新不能越过它。
#[test]
fn notification_batch_respects_total_bytes_without_reordering() {
    let mut document = notification_fixture(vec![
        "x".repeat(NOTIFICATION_BATCH_BYTES),
        "second".into(),
        "third".into(),
    ]);
    let selected = pending_projections(&document).unwrap();
    assert_eq!(selected.len(), 1);
    let Some(PendingProjection::Notification(first)) = selected.first() else {
        panic!("expected first notification");
    };
    assert_eq!(first.body.len(), NOTIFICATION_BATCH_BYTES);
    assert_eq!(first.notification_id.as_str(), "notification-0");
    document
        .project_notification_before_api(first.notification_id.clone(), 5)
        .unwrap();
    let next: Vec<_> = pending_projections(&document)
        .unwrap()
        .into_iter()
        .filter_map(|entry| match entry {
            PendingProjection::Notification(record) => Some(record.body),
            _ => None,
        })
        .collect();
    assert_eq!(next, vec!["second", "third"]);
}
