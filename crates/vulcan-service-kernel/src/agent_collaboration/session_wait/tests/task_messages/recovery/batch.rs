//! Recovery batches retain per-intent operation identities.
//! 批量恢复保留逐意图操作身份。

use super::*;

/// Recovers two source-only notifications and verifies independent acceptance and stable replay.
/// 恢复两条仅来源已提交的通知，验证独立接纳及稳定重放。
#[tokio::test]
async fn notification_batch_recovers_each_original_operation() {
    // Exercise both successful acceptance and definitive rejection through the same outbox entry.
    // 通过同一出站箱入口覆盖成功接纳与确定拒收。
    for stopped in [false, true] {
        // Use real canonical storage so duplicate operation keys surface as domain conflicts.
        // 使用真实规范存储，使重复操作键表现为领域冲突。
        let (_root, invocation, target, _) = message_fixture(false).await;
        // Resolve identities from persisted source and target documents.
        // 从持久来源和目标文档解析身份。
        let (sender, receiver) =
            notification_participants(&invocation, &invocation.source_handle, &target).await;
        for label in ["first", "second"] {
            invocation
                .source_handle
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.send_notification(SendNotificationInput {
                            operation_id: CollaborationOperationId::try_new(format!(
                                "batch-send-{label}"
                            ))
                            .unwrap(),
                            notification_id: CollaborationNotificationId::try_new(format!(
                                "batch-notification-{label}"
                            ))
                            .unwrap(),
                            delivery_id: CollaborationDeliveryId::try_new(format!(
                                "batch-delivery-{label}"
                            ))
                            .unwrap(),
                            source: sender.clone(),
                            target: receiver.clone(),
                            body: label.to_owned(),
                            created_at_ms: 5,
                        })
                    },
                )
                .unwrap();
        }
        if stopped {
            target
                .set_collaboration_run_control_durable(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("batch-stop").unwrap(),
                    state: CollaborationRunControl::Stopped,
                    changed_at_ms: 6,
                })
                .unwrap();
        }
        assert_eq!(
            reconcile_session_v1_collaboration_outbox(
                &crate::ServiceEventEmitter::new(vec![]),
                invocation.state.clone(),
                invocation.source_session_id,
            )
            .await
            .unwrap(),
            2
        );
        invocation
            .source_handle
            .read(|document| {
                assert_eq!(document.collaboration_state().notifications.len(), 2);
                for record in document.collaboration_state().notifications.values() {
                    assert_eq!(
                        record.state,
                        if stopped {
                            CollaborationNotificationState::Failed
                        } else {
                            CollaborationNotificationState::Accepted
                        }
                    );
                    if stopped {
                        assert_eq!(
                            record.failure,
                            Some(CollaborationDeliveryFailureReason::TargetStopped)
                        );
                    } else {
                        // Stable acknowledgement keys must use the original intent, never the scanner ID.
                        // 稳定确认键必须使用原始意图，不能使用扫描器身份。
                        let operation = CollaborationOperationId::try_new(format!(
                            "{}:notification-accepted",
                            record.operation_id.as_str()
                        ))
                        .unwrap();
                        assert!(
                            document
                                .collaboration_state()
                                .operations
                                .contains_key(&operation)
                        );
                    }
                }
            })
            .unwrap();
        // Repeating a completed batch cannot append journal events on either side.
        // 重放已完成批次不能在任一侧追加日志事件。
        let revisions = (
            invocation
                .source_handle
                .read(SessionDocument::memory_revision)
                .unwrap(),
            target.read(SessionDocument::memory_revision).unwrap(),
        );
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
        assert_eq!(
            revisions,
            (
                invocation
                    .source_handle
                    .read(SessionDocument::memory_revision)
                    .unwrap(),
                target.read(SessionDocument::memory_revision).unwrap()
            )
        );
    }
}

/// Rejects multiple source-only questions independently when their target stops before acceptance.
/// 目标在接纳前停止时，独立拒收多个仅来源已提交的问题。
#[tokio::test]
async fn question_batch_rejection_uses_each_original_operation() {
    // Keep the canonical fixture alive until all durable reads complete.
    // 在全部可靠读取完成前保留规范存储夹具。
    let (_root, invocation, target, _) = message_fixture(false).await;
    // Freeze both participant coordinates before the target stop.
    // 在目标停止前冻结双方参与者坐标。
    let (sender, receiver) =
        notification_participants(&invocation, &invocation.source_handle, &target).await;
    // Match the production question purpose for the current target business turn.
    // 匹配当前目标业务回合在生产链路中的问题执行用途。
    let (execution_purpose, _) = target.read(target_question_binding).unwrap();
    for label in ["first", "second"] {
        invocation
            .source_handle
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.create_question(CreateQuestionInput {
                        operation_id: CollaborationOperationId::try_new(format!(
                            "batch-question-send-{label}"
                        ))
                        .unwrap(),
                        question_id: CollaborationQuestionId::try_new(format!(
                            "batch-question-{label}"
                        ))
                        .unwrap(),
                        delivery_id: CollaborationDeliveryId::try_new(format!(
                            "batch-question-delivery-{label}"
                        ))
                        .unwrap(),
                        source: sender.clone(),
                        target: receiver.clone(),
                        body: label.to_owned(),
                        execution_purpose,
                        created_at_ms: 5,
                    })
                },
            )
            .unwrap();
    }
    target
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("batch-question-stop").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 6,
        })
        .unwrap();
    assert_eq!(
        reconcile_session_v1_collaboration_outbox(
            &crate::ServiceEventEmitter::new(vec![]),
            invocation.state.clone(),
            invocation.source_session_id,
        )
        .await
        .unwrap(),
        2
    );
    invocation
        .source_handle
        .read(|document| {
            assert_eq!(document.collaboration_state().outbound_deliveries.len(), 2);
            for record in document.collaboration_state().outbound_deliveries.values() {
                assert_eq!(record.state, CollaborationDeliveryState::Failed);
                assert_eq!(
                    record.failure,
                    Some(CollaborationDeliveryFailureReason::TargetStopped)
                );
            }
        })
        .unwrap();
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
