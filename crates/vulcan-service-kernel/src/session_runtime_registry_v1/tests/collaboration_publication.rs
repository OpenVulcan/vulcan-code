//! Collaboration publication through the canonical persistence owner.
//! 通过规范持久化所有者验证协作发布。

use super::*;
use vulcan_session_core::v1::collaboration::{
    AcceptNotificationInput, CollaborationDeliveryId, CollaborationNotificationId,
    CollaborationParticipant,
};

/// Root admission, message projection and natural completion each publish one complete revision.
/// 根准入、消息投影与自然完成分别发布一个完整修订。
/// Exercises the real owner gate so an otherwise correct Core draft cannot quarantine a live session.
/// 验证真实所有者门禁，防止表面正确的 Core 草稿隔离活动会话。
#[tokio::test]
async fn collaboration_composites_publish_once_without_quarantining_owner() {
    // Use the same recording backend and owner as the production mutation wrappers.
    // 使用与生产修改包装器一致的记录后端和所有者。
    let (recording, handle) = recording_handle();
    // Root input and Running control are one durable admission.
    // 根输入与 Running 控制构成一次可靠准入。
    let root = handle
        .submit_root_input(test_client_input("collaboration-root", "Original work."), 2)
        .expect("root and run control must publish atomically");
    assert_eq!(root.revision, 1);
    assert_eq!(root.changed_event_seqs.len(), 2);
    // The recipient is bound to the exact admitted turn and current document generation.
    // 接收者绑定精确已准入回合及当前文档代次。
    let target = handle
        .read(|document| {
            CollaborationParticipant::new(
                handle.session_id().to_string(),
                handle.session_id().to_string(),
                "project-1",
                document.metadata.segment_generation,
                Some(root.value),
                None,
            )
        })
        .unwrap()
        .unwrap();
    // A separate source identity supplies the accepted notification's immutable origin.
    // 独立来源身份提供已接纳通知的不可变来源。
    let source = CollaborationParticipant::new(
        "notification-source",
        "notification-source",
        "project-1",
        1,
        Some(TurnId::try_new(1).unwrap()),
        None,
    )
    .unwrap();
    // Stable notification identity is reused for the exact projection retry.
    // 稳定通知身份在精确投影重试时复用。
    let notification_id = CollaborationNotificationId::try_new("one-progress-update").unwrap();
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_notification(AcceptNotificationInput {
                        operation_id: CollaborationOperationId::try_new("accept-progress").unwrap(),
                        notification_id: notification_id.clone(),
                        delivery_id: CollaborationDeliveryId::try_new("progress-delivery").unwrap(),
                        source_operation_id: CollaborationOperationId::try_new("send-progress")
                            .unwrap(),
                        source,
                        target,
                        body: String::from("Stage one complete."),
                        accepted_at_ms: 3,
                    })
                    .map(session_change_from_idempotent)
            },
        )
        .expect("accept notification");
    // User envelope and its target-side delivery proof must reach the writer together.
    // User 信封和目标交付凭据必须一起到达写入器。
    let projection = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document
                    .project_notification_before_api(notification_id.clone(), 4)
                    .map(session_change_from_idempotent)
            },
        )
        .expect("composite projection must not quarantine the owner");
    assert_eq!(projection.revision, 3);
    assert_eq!(
        projection.value.notification.target_revision,
        projection.revision
    );
    assert_eq!(projection.changed_event_seqs.len(), 2);
    assert_eq!(recording.flush_count(), 3);
    // Replaying the same projection neither issues another revision nor writes the backend.
    // 重放同一投影既不发出新修订，也不再次写入后端。
    let replay = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document
                    .project_notification_before_api(notification_id, 5)
                    .map(session_change_from_idempotent)
            },
        )
        .unwrap();
    assert!(!replay.changed);
    assert_eq!(replay.value, projection.value);
    assert_eq!(recording.flush_count(), 3);
    // Complete a real physical API; the business gate requires its final assistant evidence.
    // 完成真实物理 API；业务门禁要求它提供最终助手回复证据。
    let api_input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                6,
            )
        })
        .unwrap();
    // Preparation publishes the exact request proof before assistant output can be admitted.
    // 准备在允许助手输出前发布精确请求凭据。
    let api_id = handle
        .prepare_and_start_api_durable(api_input)
        .unwrap()
        .value
        .api_call_id;
    handle
        .update_assistant_text(
            api_id,
            "Work complete.",
            AssistantSemanticStatus::Completed,
            7,
        )
        .unwrap();
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document
                    .complete_api_group(
                        api_id,
                        ApiCompletion {
                            request: ApiRequestSnapshot::new("openai", "gpt-5"),
                            usage: UsageSnapshot::default(),
                            status: ApiCallStatus::Completed,
                            end_reason: Some(ApiEndReason::CompletedSuccessfully),
                            error: None,
                            continuation_eligibility: ContinuationEligibility::Forbidden,
                            timing: ApiTimings {
                                completed_at_ms: Some(8),
                                ..ApiTimings::default()
                            },
                            provider_source: None,
                            completed_at_ms: 8,
                        },
                    )
                    .map(SessionMutationChange::Changed)
            },
        )
        .unwrap();
    // Capture the actual physical-call writes before checking one business publication.
    // 检查一次业务发布前，记录真实物理调用产生的写入。
    let revision_before_completion = handle.read(SessionDocument::memory_revision).unwrap();
    // The final logical transition must issue precisely one additional backend write.
    // 最终逻辑转换必须精确增加一次后端写入。
    let flushes_before_completion = recording.flush_count();
    // Turn statistics and NaturallyIdle control share the next single publication.
    // 回合统计与 NaturallyIdle 控制共享下一个单一发布。
    let completion = handle
        .complete_successful_turn(9)
        .await
        .expect("completion and idle control must publish atomically");
    assert!(matches!(
        completion.value,
        SuccessfulTurnSettlement::Completed(_)
    ));
    assert_eq!(completion.revision, revision_before_completion + 1);
    assert_eq!(completion.changed_event_seqs.len(), 3);
    assert_eq!(recording.flush_count(), flushes_before_completion + 1);
    handle
        .read(|document| {
            assert!(document.active_turn().unwrap().closed);
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::NaturallyIdle
            );
            assert_eq!(document.memory_revision(), document.durable_revision());
        })
        .unwrap();
    handle
        .ensure_admitted(SessionMutationAdmission::RootTurn)
        .unwrap();
}
