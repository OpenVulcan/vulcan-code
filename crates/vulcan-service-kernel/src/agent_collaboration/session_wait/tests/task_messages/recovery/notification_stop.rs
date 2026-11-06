//! Stop settlement of notifications accepted before the next provider boundary.
//! 下一供应商边界前已接纳通知的停止结算。

use super::*;

/// A volatile stop failure is published on the target before the sender acknowledges it.
/// 易失停止失败先在目标侧可靠发布，发送方才确认它。
#[tokio::test]
async fn notification_stop_return_publishes_volatile_target_failure_first() {
    // Keep the original source receipt durable while staging only the target stop in memory.
    // 原来源回执保持可靠，仅在内存暂存目标停止。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let sent = notifications::send_notification(
        &invocation,
        target.session_id(),
        "Unpublished stop.".to_owned(),
    )
    .await
    .unwrap();
    let notification_id =
        CollaborationNotificationId::try_new(sent["notification_id"].as_str().unwrap()).unwrap();
    target
        .mutate_in_memory_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .set_collaboration_run_control(SetRunControlInput {
                        operation_id: CollaborationOperationId::try_new(
                            "volatile-notification-stop",
                        )
                        .unwrap(),
                        state: CollaborationRunControl::Stopped,
                        changed_at_ms: now_ms().unwrap(),
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    assert_eq!(
        reconcile_stopped_question_deliveries_for_session(&invocation.state, target.session_id())
            .await
            .unwrap(),
        1
    );
    // The acknowledged failure must already be the target's durable state.
    // 已确认的失败必须已经是目标的可靠状态。
    let published = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        published.collaboration_run_control(),
        CollaborationRunControl::Stopped
    );
    assert_eq!(
        invocation
            .source_handle
            .read(|document| document.collaboration_state().notifications[&notification_id].state)
            .unwrap(),
        CollaborationNotificationState::Failed
    );
}

/// Full stop fails an unprojected notification atomically and both live and cold paths return it.
/// 全停原子失败未投影通知，实时及冷恢复路径均能回传。
#[tokio::test]
async fn accepted_notification_stop_returns_durable_failure() {
    // Test both explicit stop controls and both sides of the acknowledgement crash window.
    // 覆盖两种明确停止控制及确认崩溃窗口两侧。
    for control in [
        CollaborationRunControl::Stopped,
        CollaborationRunControl::MainAndSubagents,
    ] {
        for live_return in [false, true] {
            // Real files retain exact source and target state for reload assertions.
            // 真实文件保留双方精确状态，用于重载断言。
            let (_root, invocation, target, _) = message_fixture(false).await;
            // A normal send must succeed before the target stop is introduced.
            // 引入目标停止前，正常发送必须成功。
            let sent = notifications::send_notification(
                &invocation,
                target.session_id(),
                "A completed stage.".to_owned(),
            )
            .await
            .unwrap();
            let notification_id =
                CollaborationNotificationId::try_new(sent["notification_id"].as_str().unwrap())
                    .unwrap();
            let before = target.read(SessionDocument::memory_revision).unwrap();
            let stop = SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("accepted-notification-stop")
                    .unwrap(),
                state: control,
                changed_at_ms: now_ms().unwrap(),
            };
            target
                .set_collaboration_run_control_durable(stop.clone())
                .unwrap();
            assert_eq!(
                target.read(SessionDocument::memory_revision).unwrap(),
                before + 1
            );
            // Replaying the original stop cannot append another failure event.
            // 重放原停止不能追加另一条失败事件。
            target.set_collaboration_run_control_durable(stop).unwrap();
            assert_eq!(
                target.read(SessionDocument::memory_revision).unwrap(),
                before + 1
            );
            let proof = load_session_v1_for_runtime(&target.storage_locator().unwrap())
                .unwrap()
                .document
                .collaboration_state()
                .inbound_notifications[&notification_id]
                .clone();
            assert_eq!(proof.state, CollaborationNotificationState::Failed);
            assert_eq!(
                proof.failure,
                Some(CollaborationDeliveryFailureReason::TargetStopped)
            );
            assert!(proof.projected_seq.is_none());
            if live_return {
                assert_eq!(
                    reconcile_stopped_question_deliveries_for_session(
                        &invocation.state,
                        target.session_id()
                    )
                    .await
                    .unwrap(),
                    1
                );
                assert_eq!(
                    reconcile_stopped_question_deliveries_for_session(
                        &invocation.state,
                        target.session_id()
                    )
                    .await
                    .unwrap(),
                    0
                );
            } else {
                assert_eq!(
                    reconcile_session_v1_collaboration_outbox(
                        &crate::ServiceEventEmitter::new(vec![]),
                        invocation.state.clone(),
                        invocation.source_session_id
                    )
                    .await
                    .unwrap(),
                    1
                );
            }
            let source =
                load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
                    .unwrap()
                    .document;
            let returned = &source.collaboration_state().notifications[&notification_id];
            assert_eq!(returned.state, CollaborationNotificationState::Failed);
            assert_eq!(returned.failure, proof.failure);
            assert_eq!(returned.target_revision, Some(proof.target_revision));
            assert!(returned.target_event_seq.is_some());
            assert!(!source.has_unsettled_collaboration_for_edit_or_delete());
            assert!(
                !target
                    .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
                    .unwrap()
            );
            // Restart cannot project the stopped notification into a later model request.
            // 恢复不能把已停止通知投影到后续模型请求。
            target
                .set_collaboration_run_control_durable(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new(
                        "restart-after-notification-stop",
                    )
                    .unwrap(),
                    state: CollaborationRunControl::Running,
                    changed_at_ms: now_ms().unwrap(),
                })
                .unwrap();
            project_collaboration_before_api(&invocation.state, &target, now_ms().unwrap())
                .await
                .unwrap();
            assert!(
                target
                    .read(
                        |document| document.collaboration_state().inbound_notifications
                            [&notification_id]
                            .projected_seq
                            .is_none()
                    )
                    .unwrap()
            );
        }
    }
}

/// Main-only stop keeps accepted notifications pending for the retained parent turn.
/// 仅主会话停止使已接纳通知在保留的父回合继续待处理。
#[tokio::test]
async fn main_only_retains_accepted_notification() {
    // The same domain transition must preserve the explicit MainOnly exception.
    // 同一领域转换必须保留明确的仅主会话停止例外。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let sent = notifications::send_notification(
        &invocation,
        target.session_id(),
        "A stage report.".to_owned(),
    )
    .await
    .unwrap();
    let notification_id =
        CollaborationNotificationId::try_new(sent["notification_id"].as_str().unwrap()).unwrap();
    target
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("notification-main-only").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: now_ms().unwrap(),
        })
        .unwrap();
    assert_eq!(
        target
            .read(
                |document| document.collaboration_state().inbound_notifications[&notification_id]
                    .state
            )
            .unwrap(),
        CollaborationNotificationState::Accepted
    );
}
