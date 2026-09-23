//! Two-owner result transfer, late receipt and immutable replay regression tests.
//! 双方结果投递、迟到回执与不可变重放回归测试。

use super::*;

mod projection;

/// Creates a requester outbox and an actual successful executor result, without source acknowledgement.
/// 创建请求者出站箱及真实成功执行结果，但不确认来源接纳。
/// Returns both documents and the result requiring the ordinary source receipt to be repaired first.
/// 返回双方文档及结果；首先需要修复普通来源回执。
fn completed_pair() -> (SessionDocument, SessionDocument, CollaborationTaskResult) {
    let (mut source, _, message) = fixture(CollaborationTaskMessageKind::TaskSteer);
    source.send_task_message(message.clone(), 3).unwrap();
    let (mut target, _) = applied_task(CollaborationTaskMessageKind::TaskSteer);
    finish_response(&mut target, "Exact task answer");
    target.close_turn(TurnOutcome::Completed, 1, 9).unwrap();
    let result = target.collaboration_state().task_results[&message.delivery_id].clone();
    (source, target, result)
}

/// Repairs source acceptance and returns the first immutable receiver receipt.
/// 修复来源接纳，返回首份不可变接收回执。
/// `source` is the original requester; `result` remains executor authority.
/// `source` 是原请求者；`result` 仍是执行者权威。
fn receive(
    source: &mut SessionDocument,
    result: CollaborationTaskResult,
) -> CollaborationTaskResultAcceptance {
    source
        .acknowledge_task_message(
            CollaborationOperationId::try_new("repair-task-acceptance").unwrap(),
            result.acceptance.clone(),
            10,
        )
        .unwrap();
    source
        .accept_task_result(
            CollaborationOperationId::try_new("receive-result").unwrap(),
            result,
            11,
        )
        .unwrap()
        .into_value()
}

/// An application failure returns through both durable owners without inventing a business terminal.
/// 应用前失败经双方可靠归属回传，不虚构业务终态。
#[test]
fn failed_task_delivery_returns_and_releases_both_history_guards() {
    let (mut source, mut target, message) = fixture(CollaborationTaskMessageKind::TaskSteer);
    source.send_task_message(message.clone(), 3).unwrap();
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept-failed").unwrap(),
            message.clone(),
            4,
        )
        .unwrap();
    target
        .fail_task_message(
            CollaborationOperationId::try_new("fail-application").unwrap(),
            message.delivery_id.clone(),
            CollaborationDeliveryFailureReason::InvalidBinding,
            5,
        )
        .unwrap();
    assert!(target.collaboration_state().requires_edit_or_delete_settlement());
    let original_turn = target.active_turn().unwrap().turn_id;
    let result = target
        .record_failed_task_result(message.delivery_id.clone(), 6)
        .unwrap()
        .into_value();
    assert!(result.terminal.is_none());
    assert_eq!(target.active_turn().unwrap().turn_id, original_turn);
    assert_eq!(
        reload(&target)
            .record_failed_task_result(message.delivery_id.clone(), 7)
            .unwrap()
            .into_value(),
        result
    );
    let receipt = receive(&mut source, result.clone());
    target
        .acknowledge_task_result(
            CollaborationOperationId::try_new("confirm-failed").unwrap(),
            receipt.clone(),
            12,
        )
        .unwrap();
    assert!(!source.collaboration_state().requires_edit_or_delete_settlement());
    assert!(!target.collaboration_state().requires_edit_or_delete_settlement());
    let envelope = &result.task_result_envelopes(receipt.event_seq).unwrap()[0];
    assert_eq!(envelope.kind, vulcan_ai_contract::HostMessageKind::TaskDeliveryFailed);
    assert_eq!(envelope.source, "target");
    assert_eq!(envelope.target, "source");
    let body: serde_json::Value = serde_json::from_str(&envelope.body).unwrap();
    assert_eq!(body["delivery_id"], message.delivery_id.as_str());
    assert_eq!(body["reason"], "invalid_binding");
    assert!(!envelope.render().unwrap().contains("task_result"));
    reload(&source);
    reload(&target);
}

/// Late delivery survives a new requester turn and returns the same receipt after either side reloads.
/// 迟到投递跨越请求者新回合，并在任一方重载后返回相同回执。
#[test]
fn task_result_delivery_preserves_closed_source_and_exact_replay() {
    let (mut source, mut target, result) = completed_pair();
    let before = source.clone();
    assert!(
        source
            .accept_task_result(
                CollaborationOperationId::try_new("receive-result").unwrap(),
                result.clone(),
                10
            )
            .is_err()
    );
    assert_eq!(
        source, before,
        "unacknowledged intent must be repaired explicitly"
    );
    source.close_turn(TurnOutcome::Completed, 1, 10).unwrap();
    source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("new-source-business").unwrap(),
                "Other work",
                vec![],
            ),
            10,
        )
        .unwrap();
    let active = source.active_turn().cloned();
    let acceptance = receive(&mut source, result.clone());
    assert_eq!(source.active_turn(), active.as_ref());
    assert!(source.active_api().is_none());
    assert_eq!(acceptance.receiver_revision, source.memory_revision());
    let mut source = reload(&source);
    let before = source.clone();
    for operation in ["receive-result", "retry-result"] {
        let repeated = source
            .accept_task_result(
                CollaborationOperationId::try_new(operation).unwrap(),
                result.clone(),
                50,
            )
            .unwrap();
        assert!(matches!(repeated, IdempotentMutation::Idempotent(_)));
        assert_eq!(repeated.into_value(), acceptance);
        assert_eq!(source, before);
    }
    let acknowledgement = target
        .acknowledge_task_result(
            CollaborationOperationId::try_new("confirm-result").unwrap(),
            acceptance.clone(),
            12,
        )
        .unwrap()
        .into_value();
    assert_eq!(acknowledgement.acceptance, acceptance);
    let mut target = reload(&target);
    let before = target.clone();
    for operation in ["confirm-result", "retry-confirm"] {
        let repeated = target
            .acknowledge_task_result(
                CollaborationOperationId::try_new(operation).unwrap(),
                acceptance.clone(),
                51,
            )
            .unwrap();
        assert!(matches!(repeated, IdempotentMutation::Idempotent(_)));
        assert_eq!(repeated.into_value(), acknowledgement);
        assert_eq!(target, before);
    }
}

/// A completed task clears local edit guards only after each owner has its required terminal receipt.
/// 已完成任务只有在双方各自取得所需终态回执后才解除本地编辑保护。
/// The service still checks the executor acknowledgement before replacing requester history.
/// 服务层在替换请求者历史前还会核对执行者确认。
#[test]
fn completed_task_result_clears_local_edit_guards_after_receipts() {
    let (mut source, mut target, result) = completed_pair();
    assert!(source.has_unsettled_collaboration_for_edit_or_delete());
    assert!(target.has_unsettled_collaboration_for_edit_or_delete());

    let acceptance = receive(&mut source, result);
    assert!(
        !source.has_unsettled_collaboration_for_edit_or_delete(),
        "a durable requester result is locally complete"
    );
    assert!(
        target.has_unsettled_collaboration_for_edit_or_delete(),
        "the executor must retain its result until requester acceptance is acknowledged"
    );

    target
        .acknowledge_task_result(
            CollaborationOperationId::try_new("confirm-completed-edit-guard").unwrap(),
            acceptance,
            12,
        )
        .unwrap();
    assert!(
        !target.has_unsettled_collaboration_for_edit_or_delete(),
        "both terminal receipts permit a settled task history to be edited"
    );
}

/// Conflicting receipt bodies, receiver coordinates and occupied operation keys cannot alter either owner.
/// 冲突回执正文、接收坐标与被占用操作键不能改变任一所有者。
#[test]
fn task_result_delivery_rejects_conflicts_without_partial_writes() {
    let (mut source, mut target, result) = completed_pair();
    let acceptance = receive(&mut source, result.clone());
    let before = source.clone();
    let mut altered = result;
    altered
        .terminal
        .as_mut()
        .unwrap()
        .final_assistant
        .as_mut()
        .unwrap()
        .source_text
        .text = "Different answer".into();
    for operation in ["receive-result", "other-receive"] {
        assert!(
            source
                .accept_task_result(
                    CollaborationOperationId::try_new(operation).unwrap(),
                    altered.clone(),
                    20
                )
                .is_err()
        );
        assert_eq!(source, before);
    }
    let before = target.clone();
    let mut false_receipt = acceptance.clone();
    false_receipt.result = altered;
    assert!(
        target
            .acknowledge_task_result(
                CollaborationOperationId::try_new("confirm-result").unwrap(),
                false_receipt,
                20
            )
            .is_err()
    );
    assert_eq!(target, before);
    target
        .acknowledge_task_result(
            CollaborationOperationId::try_new("confirm-result").unwrap(),
            acceptance.clone(),
            20,
        )
        .unwrap();
    let before = target.clone();
    let mut other_receipt = acceptance;
    other_receipt.event_seq = Seq::try_new(other_receipt.event_seq.value() + 1).unwrap();
    assert!(
        target
            .acknowledge_task_result(
                CollaborationOperationId::try_new("other-confirm").unwrap(),
                other_receipt,
                21
            )
            .is_err()
    );
    assert_eq!(target, before);
}

/// Metadata-only checks retain exact transfer operations, and hydration rejects foreign owners and edited events.
/// 仅元数据校验保留精确投递操作，水合拒绝外来所有者与篡改事件。
#[test]
fn task_result_delivery_reload_rejects_missing_and_forged_receipts() {
    let (mut source, mut target, result) = completed_pair();
    let acceptance = receive(&mut source, result);
    let acknowledgement = target
        .acknowledge_task_result(
            CollaborationOperationId::try_new("confirm-result").unwrap(),
            acceptance.clone(),
            12,
        )
        .unwrap()
        .into_value();
    for (document, operation_id) in [
        (&source, &acceptance.operation_id),
        (&target, &acknowledgement.operation_id),
    ] {
        let mut metadata = document.metadata.clone();
        metadata.collaboration.state.operations.remove(operation_id);
        assert!(metadata.collaboration.state.validate().is_err());
        let mut metadata = document.metadata.clone();
        metadata.session_id = "foreign-session".into();
        assert!(
            SessionDocument::from_parts(
                metadata,
                document.runtime_lock.clone(),
                document.compactions.clone(),
                document.segment_catalog().to_vec(),
                document.events().to_vec()
            )
            .is_err()
        );
    }
    let mut events = source.events().to_vec();
    let host = events
        .iter_mut()
        .find(|event| event.seq == acceptance.event_seq)
        .unwrap();
    host.visibility = EventVisibility::Provider;
    assert!(
        SessionDocument::from_parts(
            source.metadata.clone(),
            source.runtime_lock.clone(),
            source.compactions.clone(),
            source.segment_catalog().to_vec(),
            events
        )
        .is_err()
    );
    let mut events = target.events().to_vec();
    let host = events
        .iter_mut()
        .find(|event| event.seq == acknowledgement.event_seq)
        .unwrap();
    host.created_at_ms += 1;
    assert!(
        SessionDocument::from_parts(
            target.metadata.clone(),
            target.runtime_lock.clone(),
            target.compactions.clone(),
            target.segment_catalog().to_vec(),
            events
        )
        .is_err()
    );
}
