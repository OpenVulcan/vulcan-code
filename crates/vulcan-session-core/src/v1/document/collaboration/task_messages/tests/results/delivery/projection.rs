//! Original-turn result projection and recovery invariants.
//! 原回合结果投影及恢复不变量。

use super::*;

/// Unprojected acceptance and projected chunks require a business request; compaction cannot consume them.
/// 未投影接纳与已投影分块均要求业务请求；压缩不能消费它们。
#[test]
fn collaboration_continuation_tracks_result_consumption_and_original_turn() {
    let (mut source, acceptance) = accepted_long_result();
    let authority = source.pending_collaboration_continuation().unwrap();
    assert_eq!(authority.acceptance_seq, acceptance.event_seq);
    // An API starting before projection has no result in its context, despite its later sequence.
    // 投影前启动的 API 即使序号较晚，其上下文也没有结果。
    finish_response(&mut source, "Old context response");
    assert_eq!(
        source.pending_collaboration_continuation(),
        Some(authority.clone())
    );
    let before = source.clone();
    assert!(
        source
            .complete_turn_with_lifetime_usage(authority.turn_id, 11, None)
            .is_err()
    );
    assert_eq!(source, before);
    source
        .project_task_result_before_api(
            acceptance.result.effect.delivery_id,
            CollaborationTaskResultDisclosure::Full,
            12,
        )
        .unwrap();
    assert_eq!(
        reload(&source).pending_collaboration_continuation(),
        Some(authority.clone())
    );
    let api = source
        .start_api(
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            13,
        )
        .unwrap();
    assert_eq!(
        source.pending_collaboration_continuation(),
        Some(authority.clone())
    );
    source.interrupt_active_api(14).unwrap();
    assert!(source.events().iter().any(
        |event| matches!(&event.event, EventBody::ApiStats(stats) if stats.api_call_id == api)
    ));
    source
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            15,
        )
        .unwrap();
    assert!(source.pending_collaboration_continuation().is_none());
    source.interrupt_active_api(16).unwrap();
    source.close_turn(TurnOutcome::Stopped, 1, 17).unwrap();
    source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("other-result-task").unwrap(),
                "New business",
                vec![],
            ),
            18,
        )
        .unwrap();
    assert!(source.pending_collaboration_continuation().is_none());
}

/// Exact steering remains pending across projection, and explicit stop removes automatic continuation authority.
/// 精确方向在投影前后保持待处理，明确停止会移除自动续跑权威。
#[test]
fn collaboration_continuation_tracks_steering_and_stop_control() {
    let (_, mut target, message) = fixture(CollaborationTaskMessageKind::TaskSteer);
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("target-running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 3,
        })
        .unwrap();
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept-steer").unwrap(),
            message.clone(),
            4,
        )
        .unwrap();
    let authority = target.pending_collaboration_continuation().unwrap();
    assert!(matches!(
        authority.source,
        CollaborationContinuationSource::Steering { .. }
    ));
    target
        .apply_task_steering_before_api(
            CollaborationOperationId::try_new("apply-steer").unwrap(),
            message.delivery_id.clone(),
            5,
        )
        .unwrap();
    assert_eq!(
        target.pending_collaboration_continuation(),
        Some(authority.clone())
    );
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("target-stop").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: 6,
        })
        .unwrap();
    assert!(target.pending_collaboration_continuation().is_none());
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("target-resume").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 7,
        })
        .unwrap();
    assert_eq!(
        reload(&target).pending_collaboration_continuation(),
        Some(authority)
    );
    target
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            8,
        )
        .unwrap();
    assert!(target.pending_collaboration_continuation().is_none());
}

/// A conflict in the second chunk rolls back the first chunk and its allocated identities.
/// 第二份分块冲突时，回滚第一份分块及其分配身份。
#[test]
fn task_result_projection_late_chunk_conflict_rolls_back_complete_batch() {
    let (mut source, acceptance) = accepted_long_result();
    let envelopes = acceptance
        .result
        .task_result_envelopes(
            acceptance.event_seq,
            CollaborationTaskResultDisclosure::Full,
        )
        .unwrap();
    source
        .append_in_turn_guidance_before_api(
            ClientActionId::try_new(format!("agent-message:{}", envelopes[1].event_id)).unwrap(),
            String::from("Conflicting host input"),
            12,
        )
        .unwrap();
    let before = source.clone();
    assert!(
        source
            .project_task_result_before_api(
                acceptance.result.effect.delivery_id,
                CollaborationTaskResultDisclosure::Full,
                13
            )
            .is_err()
    );
    assert_eq!(source, before);
    reload(&source);
}

/// Builds an accepted multichunk result from an actual completed target and starts requester control.
/// 从实际完成目标构建已接纳多分块结果，并启动请求者运行控制。
/// Returns the requester and immutable local acceptance used by every projection test.
/// 返回请求者及每项投影验证使用的不可变本地接纳。
fn accepted_long_result() -> (SessionDocument, CollaborationTaskResultAcceptance) {
    let (mut source, _, message) = fixture(CollaborationTaskMessageKind::TaskSteer);
    source.send_task_message(message.clone(), 3).unwrap();
    source
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("source-running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 3,
        })
        .unwrap();
    let (mut target, _) = applied_task(CollaborationTaskMessageKind::TaskSteer);
    finish_response(&mut target, &"完整结果🦀\n".repeat(15000));
    target.close_turn(TurnOutcome::Completed, 1, 9).unwrap();
    let result = target.collaboration_state().task_results[&message.delivery_id].clone();
    let acceptance = receive(&mut source, result);
    (source, acceptance)
}

/// All chunks commit in one revision and reloading/replaying cannot duplicate any model input.
/// 全部分块在一次修订提交，重载及重放不得重复任何模型输入。
#[test]
fn task_result_projection_is_atomic_complete_and_idempotent() {
    let (mut source, acceptance) = accepted_long_result();
    let before_revision = source.memory_revision();
    let turn = source.active_turn().unwrap().turn_id;
    let delivery = acceptance.result.effect.delivery_id.clone();
    let receipt = source
        .project_task_result_before_api(
            delivery.clone(),
            CollaborationTaskResultDisclosure::Full,
            12,
        )
        .unwrap()
        .into_value();
    assert_eq!(source.memory_revision(), before_revision + 1);
    assert_eq!(receipt.receiver_revision, source.memory_revision());
    assert_eq!(receipt.turn_id, turn);
    assert!(receipt.inputs.len() > 2);
    let envelopes = acceptance
        .result
        .task_result_envelopes(
            acceptance.event_seq,
            CollaborationTaskResultDisclosure::Full,
        )
        .unwrap();
    for (input, expected) in receipt.inputs.iter().zip(envelopes) {
        let event = source
            .events()
            .iter()
            .find(|event| event.seq == input.input_seq)
            .unwrap();
        let EventBody::UserInput(actual) = &event.event else {
            panic!("missing User chunk")
        };
        assert_eq!(actual.text, expected.render().unwrap());
        assert_eq!(event.turn_id, Some(turn));
    }
    let mut source = reload(&source);
    let before = source.clone();
    assert_eq!(
        source
            .project_task_result_before_api(
                delivery.clone(),
                CollaborationTaskResultDisclosure::Full,
                99
            )
            .unwrap(),
        IdempotentMutation::Idempotent(receipt.clone())
    );
    assert_eq!(source, before);
    source.close_turn(TurnOutcome::Stopped, 1, 100).unwrap();
    let before = source.clone();
    assert_eq!(
        source
            .project_task_result_before_api(delivery, CollaborationTaskResultDisclosure::Full, 101)
            .unwrap(),
        IdempotentMutation::Idempotent(receipt)
    );
    assert_eq!(source, before);
}

/// Active physical requests, stopped control and closed turns cannot accept fresh result chunks.
/// 活动物理请求、已停止控制以及已关闭回合不得接纳新的结果分块。
#[test]
fn task_result_projection_rejects_illegal_boundaries_without_writes() {
    let (base, acceptance) = accepted_long_result();
    let delivery = acceptance.result.effect.delivery_id;
    for scenario in 0..3 {
        let mut source = base.clone();
        match scenario {
            0 => {
                source
                    .start_api(
                        ApiPurposeKind::MainGeneration,
                        ApiRequestSnapshot::new("openai", "gpt-5"),
                        12,
                    )
                    .unwrap();
            }
            1 => {
                source
                    .set_collaboration_run_control(SetRunControlInput {
                        operation_id: CollaborationOperationId::try_new("stop-source").unwrap(),
                        state: CollaborationRunControl::Stopped,
                        changed_at_ms: 12,
                    })
                    .unwrap();
            }
            _ => {
                source.close_turn(TurnOutcome::Stopped, 1, 12).unwrap();
            }
        }
        let before = source.clone();
        assert!(
            source
                .project_task_result_before_api(
                    delivery.clone(),
                    CollaborationTaskResultDisclosure::Full,
                    14
                )
                .is_err()
        );
        assert_eq!(source, before);
    }
}

/// A later receiving turn consumes an unprojected result exactly once without rewriting its original sender.
/// 后续接收回合仅消费一次未投影结果，不改写原发送方。
#[test]
fn task_result_projection_delivers_late_result_to_current_turn() {
    let (mut source, acceptance) = accepted_long_result();
    let original_source = acceptance.result.acceptance.message.source.clone();
    source.close_turn(TurnOutcome::Stopped, 1, 12).unwrap();
    let current = source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("later-wait").unwrap(),
                "Collect pending replies",
                vec![],
            ),
            13,
        )
        .unwrap()
        .into_value();
    assert_eq!(
        source.pending_collaboration_continuation().unwrap().turn_id,
        current
    );
    let projection = source
        .project_task_result_before_api(
            acceptance.result.effect.delivery_id.clone(),
            CollaborationTaskResultDisclosure::Full,
            14,
        )
        .unwrap()
        .into_value();
    assert_eq!(projection.turn_id, current);
    assert_eq!(
        source.collaboration_state().received_task_results[&acceptance.result.effect.delivery_id]
            .result
            .acceptance
            .message
            .source,
        original_source
    );
    assert!(
        reload(&source)
            .pending_collaboration_continuation()
            .is_some()
    );
    source
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            15,
        )
        .unwrap();
    assert!(source.pending_collaboration_continuation().is_none());
}

/// Recovery rejects edited input text, wrong turn, missing host proof and orphaned projection operations.
/// 恢复拒绝被编辑输入正文、错误回合、缺失宿主凭据以及孤立投影操作。
#[test]
fn task_result_projection_reload_rejects_forged_input_proofs() {
    let (mut source, acceptance) = accepted_long_result();
    let delivery = acceptance.result.effect.delivery_id;
    let receipt = source
        .project_task_result_before_api(
            delivery.clone(),
            CollaborationTaskResultDisclosure::Full,
            12,
        )
        .unwrap()
        .into_value();
    for scenario in 0..4 {
        let mut events = source.events().to_vec();
        let mut metadata = source.metadata.clone();
        match scenario {
            0 => {
                let event = events
                    .iter_mut()
                    .find(|event| event.seq == receipt.inputs[1].input_seq)
                    .unwrap();
                let EventBody::UserInput(input) = &mut event.event else {
                    unreachable!()
                };
                input.text.push_str("forged");
            }
            1 => {
                events
                    .iter_mut()
                    .find(|event| event.seq == receipt.inputs[0].input_seq)
                    .unwrap()
                    .turn_id = None;
            }
            2 => {
                events.retain(|event| event.seq != receipt.event_seq);
            }
            _ => {
                metadata
                    .collaboration
                    .state
                    .task_result_projections
                    .remove(&delivery);
            }
        }
        assert!(
            SessionDocument::from_parts(
                metadata,
                source.runtime_lock.clone(),
                source.compactions.clone(),
                source.segment_catalog().to_vec(),
                events
            )
            .is_err()
        );
    }
}
