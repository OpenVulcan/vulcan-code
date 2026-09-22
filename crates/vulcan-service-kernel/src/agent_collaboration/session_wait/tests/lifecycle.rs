//! Archived collaboration settlement and deletion use actual SQLite and published session files.
//! 归档协作结算与删除使用实际 SQLite 和已发布会话文件。

use super::*;
use crate::session_v1_lifecycle::{
    SessionV1LifecycleError, archive_session_lifecycle_v1, delete_session_lifecycle_v1,
};
use vulcan_session_core::v1::document::SessionDocument;
use vulcan_session_core::v1::event::TurnOutcome;

/// Deleting the target first must preserve missing source acknowledgement and allow settled cleanup.
/// 先删目标必须保留缺失的来源确认，并允许已结算清理。
#[tokio::test]
async fn archived_target_deletion_waits_for_source_settlement_acknowledgement() {
    run_archived_settlement_deletion(false).await;
}

/// A source deleted after acknowledgement must not permanently prevent deletion of its settled target.
/// 确认后删除的来源不能永久阻止删除已结算目标。
#[tokio::test]
async fn archived_target_deletion_accepts_already_deleted_settled_source() {
    run_archived_settlement_deletion(true).await;
}

/// Publishes a source cancellation and target settlement, withholding only the source acknowledgement.
/// 发布来源撤销和目标结算，仅保留来源确认尚未写入。
/// Returns isolated storage, invocation, target, question identity and the original cancellation operation.
/// 返回隔离存储、调用、目标、问题身份及原撤销操作。
async fn unacknowledged_settlement_fixture() -> (
    TestHostRuntimeRoot,
    ValidatedInvocation,
    Arc<SessionHandle>,
    CollaborationQuestionId,
    String,
) {
    // Both indexed roots and every physical write live under the actual isolated host fixture.
    // 双方索引根及全部物理写入均位于实际隔离宿主夹具中。
    let (storage, invocation, target) = fixture(false).await;
    // Retain the immutable source and project authority from the production locator.
    // 从生产定位器保留不可变来源及项目权威。
    let source = &invocation.source_handle;
    // The source owns a real business turn before issuing its question.
    // 来源在发出问题前持有真实业务回合。
    let source_turn = source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("archived-source").unwrap(),
                "Ask for direction",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value;
    // The destination independently owns the receiving business turn.
    // 目标独立持有接收业务回合。
    let target_turn = target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("archived-target").unwrap(),
                "Review the question",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value;
    // Participant coordinates are captured from the actual durable documents.
    // 参与者坐标从实际可靠文档捕获。
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
    // Destination coordinates preserve their own independent generation and turn.
    // 目标坐标保留其独立代次与回合。
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
    // The service-issued namespace supports exact source lookup without scanning.
    // 服务签发命名空间支持不扫描的精确来源定位。
    let question_id = CollaborationQuestionId::try_new(format!(
        "cq1/v1:{}:archive-settlement",
        source.session_id()
    ))
    .unwrap();
    // Stable delivery identity binds both durable records.
    // 稳定投递身份绑定双方可靠记录。
    let delivery_id = CollaborationDeliveryId::try_new("archive-settlement-delivery").unwrap();
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.create_question(CreateQuestionInput {
                    operation_id: CollaborationOperationId::try_new("archive-question-create")
                        .unwrap(),
                    question_id: question_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: "Which direction?".into(),
                    execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                    created_at_ms: 3,
                })
            },
        )
        .unwrap();
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.accept_delivery(AcceptDeliveryInput {
                    operation_id: CollaborationOperationId::try_new("archive-question-accept")
                        .unwrap(),
                    question_id: question_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant,
                    bound_turn_id: Some(target_turn),
                    execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                    accepted_at_ms: 4,
                })
            },
        )
        .unwrap();
    for handle in [source, &target] {
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::TurnCompleted,
                |document| document.close_turn(TurnOutcome::Completed, 1, 5),
            )
            .unwrap();
    }
    // Commit exactly the first two cancellation writes, simulating process loss before the source acknowledgement.
    // 精确提交撤销的前两次写入，模拟来源确认前进程丢失。
    let operation = format!("user-question-cancel:v1:{}", Uuid::new_v4());
    // This is the actual source settlement receipt, not an invented event sequence.
    // 这是实际来源结算回执，不是人工构造的事件序号。
    let cancelled = source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.cancel_question(CancelQuestionInput {
                    operation_id: CollaborationOperationId::try_new(&operation).unwrap(),
                    question_id: question_id.clone(),
                    requester: source_participant,
                    reason: "No longer required".into(),
                    cancelled_at_ms: 6,
                })
            },
        )
        .unwrap()
        .value
        .into_value();
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.settle_inbound_delivery(SettleInboundDeliveryInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "{operation}:target_settlement"
                    ))
                    .unwrap(),
                    delivery_id: delivery_id.clone(),
                    question_id: question_id.clone(),
                    source_operation_id: CollaborationOperationId::try_new(&operation).unwrap(),
                    settlement: CollaborationTargetSettlementState::Cancelled,
                    source_event_seq: cancelled.event_seq,
                    acknowledged_at_ms: 7,
                })
            },
        )
        .unwrap();
    assert!(
        source
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    assert!(
        !target
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    (storage, invocation, target, question_id, operation)
}

/// Replays trusted cancellation after cold archive and deletes both roots in `source_first` order.
/// 冷态归档后重放受信任撤销，并按 `source_first` 顺序删除双方根。
/// Returns after asserting both legal deletion orders and rejection of late operations.
/// 断言两个合法删除顺序及迟到操作拒绝后返回。
async fn run_archived_settlement_deletion(source_first: bool) {
    // Reuse the exact durable gap exercised by generation replacement tests.
    // 复用代次替换测试验证的精确可靠间隙。
    let (_storage, invocation, target, question_id, operation) =
        unacknowledged_settlement_fixture().await;
    // The earlier owner is retained only for immutable identity and locator assertions.
    // 先前所有者仅用于不可变身份及定位断言。
    let source = &invocation.source_handle;
    for handle in [source, &target] {
        archive_session_lifecycle_v1(
            &invocation.controller_runtime,
            &invocation.registry,
            handle.session_id(),
            8,
        )
        .await
        .unwrap();
    }
    // Replace the registry to exclude resident ownership as evidence of a durable acknowledgement.
    // 替换注册表，排除以驻留归属冒充可靠确认的可能。
    let cold = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = cold.clone();
    // Deletion must retain the only target receipt needed to complete source-side recovery.
    // 删除必须保留来源恢复所需的唯一目标回执。
    let rejected =
        delete_session_lifecycle_v1(&invocation.controller_runtime, &cold, target.session_id())
            .await;
    assert!(
        matches!(rejected, Err(SessionV1LifecycleError::UnsettledCollaboration(id)) if id == target.session_id()),
        "Deletion removed an unacknowledged peer receipt: {rejected:?}"
    );
    service_cancel_collaboration_question(
        invocation.state.clone(),
        question_id.as_str().to_owned(),
        source.session_id().to_string(),
        operation.clone(),
        "No longer required".into(),
    )
    .await
    .unwrap();
    for handle in [source, &target] {
        // Read the current cold owner, since archive deliberately retired the earlier handle.
        // 读取当前冷态所有者，因为归档已明确退出先前句柄。
        let current = cold.get(handle.session_id()).unwrap();
        assert!(
            !current
                .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
                .unwrap()
        );
        assert!(
            current
                .read(|document| document.metadata.archived_at_ms.is_some())
                .unwrap()
        );
    }
    // Exercise both legal deletion orders, including an absent source SQLite row during target admission.
    // 验证两个合法删除顺序，包括目标准入时来源 SQLite 行已经缺失的情况。
    let deletion_order = if source_first {
        [source.session_id(), target.session_id()]
    } else {
        [target.session_id(), source.session_id()]
    };
    for id in deletion_order {
        delete_session_lifecycle_v1(&invocation.controller_runtime, &cold, id)
            .await
            .unwrap();
    }
    // A late trusted cancellation returns the exact missing-root state and never recreates deleted files.
    // 迟到的受信任撤销返回精确根缺失状态，绝不重建已删除文件。
    let late = service_cancel_collaboration_question(
        invocation.state.clone(),
        question_id.as_str().to_owned(),
        source.session_id().to_string(),
        operation,
        "No longer required".into(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        late,
        "user cancellation source root is not an indexed session"
    );
}

/// Replacing target history must not discard the terminal receipt still needed by its source.
/// 替换目标历史不得丢弃来源仍需要的终态回执。
/// Uses actual session files and the same generation publisher called by the production edit service.
/// 使用实际会话文件，以及生产编辑服务调用的同一代次发布器。
#[tokio::test]
async fn history_edit_preserves_unacknowledged_target_settlement() {
    // Both canonical documents contain the committed settlement gap, with no active business turn.
    // 双方规范文档包含已提交结算间隙，且没有活动业务回合。
    let (_storage, _invocation, target, _question_id, _operation) =
        unacknowledged_settlement_fixture().await;
    // Capture the real root event and generation instead of assuming allocator values.
    // 捕获真实根事件和代次，不假定分配器值。
    let (generation, target_seq) = target
        .read(|document| {
            (
                document.segment_generation(),
                document
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        vulcan_session_core::v1::event::EventBody::UserInput(input)
                            if input.input_source
                                == vulcan_session_core::v1::event::UserInputSource::Root =>
                        {
                            Some(event.seq)
                        }
                        _ => None,
                    })
                    .unwrap(),
            )
        })
        .unwrap();
    // The existing service calls this exact publisher without a cross-session receipt guard.
    // 既有服务直接调用此精确发布器，没有跨会话回执保护。
    let result = target.publish_edit_generation(
        generation,
        vulcan_session_core::v1::document::EditGenerationInput {
            target_seq,
            replacement: vulcan_session_core::v1::document::RootInputReplacement::new(
                "Edited target",
                vec![],
            ),
            now_ms: 9,
        },
    );
    assert!(
        result.is_err(),
        "History edit discarded the peer settlement receipt: {result:?}"
    );
    assert_eq!(
        target.read(SessionDocument::segment_generation).unwrap(),
        generation
    );
}
