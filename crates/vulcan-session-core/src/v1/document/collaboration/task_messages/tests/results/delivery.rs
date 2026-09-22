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
