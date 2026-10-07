//! Business result identity, complete text retention and transactional closure tests.
//! 业务结果身份、完整正文保留及事务闭合测试。

use super::*;
use crate::v1::document::ApiCompletion;
use crate::v1::event::{
    ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
    AssistantSemanticStatus, ContinuationEligibility, TurnOutcome,
};
use crate::v1::stats::UsageSnapshot;

mod delivery;

/// Applies one intent of `kind` to its real business turn and returns target plus message.
/// 将一条 `kind` 意图应用到真实业务回合，返回目标及消息。
fn applied_task(kind: CollaborationTaskMessageKind) -> (SessionDocument, CollaborationTaskMessage) {
    let (_, mut target, message) = fixture(kind);
    target
        .accept_task_message(
            CollaborationOperationId::try_new("accept").unwrap(),
            message.clone(),
            3,
        )
        .unwrap();
    if kind == CollaborationTaskMessageKind::TaskStart {
        target.close_turn(TurnOutcome::Completed, 1, 4).unwrap();
        assert!(target.collaboration_state().task_results.is_empty());
        target
            .claim_task_message_before_api(
                CollaborationOperationId::try_new("apply").unwrap(),
                message.delivery_id.clone(),
                5,
            )
            .unwrap();
    } else {
        target
            .apply_task_steering_before_api(
                CollaborationOperationId::try_new("apply").unwrap(),
                message.delivery_id.clone(),
                5,
            )
            .unwrap();
    }
    (target, message)
}

/// Completes one actual API with the full `text` snapshot; returns its API identity.
/// 以完整 `text` 快照完成真实 API；返回 API 身份。
fn finish_response(target: &mut SessionDocument, text: &str) -> crate::v1::ApiCallId {
    let request = ApiRequestSnapshot::new("openai", "gpt-5");
    let api = target
        .start_api(ApiPurposeKind::MainGeneration, request.clone(), 6)
        .unwrap();
    target
        .append_assistant_text(api, text, AssistantSemanticStatus::Completed, 7)
        .unwrap();
    target
        .complete_api_group(
            api,
            ApiCompletion {
                request,
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: 8,
            },
        )
        .unwrap();
    api
}

/// Successful task results preserve the full large text, exact ownership and one closure revision.
/// 成功任务结果保留完整大正文、精确归属及单次闭合修订。
#[test]
fn task_result_retains_full_response_and_original_turn_after_successor() {
    for kind in [
        CollaborationTaskMessageKind::TaskStart,
        CollaborationTaskMessageKind::TaskSteer,
    ] {
        let (mut target, message) = applied_task(kind);
        let turn = target.active_turn().unwrap().clone();
        let text = "完整回复🙂".repeat(9000);
        let api = finish_response(&mut target, &text);
        let prefix = target.events().to_vec();
        let revision = target.memory_revision();
        target.close_turn(TurnOutcome::Completed, 4, 9).unwrap();
        let result = target.collaboration_state().task_results[&message.delivery_id].clone();
        assert_eq!(target.memory_revision(), revision + 1);
        assert_eq!(result.target_revision, target.memory_revision());
        assert_eq!(&target.events()[..prefix.len()], prefix);
        assert_eq!(result.acceptance.message, message);
        let terminal = result.terminal.as_ref().unwrap();
        assert_eq!(terminal.turn_id, turn.turn_id);
        assert_eq!(terminal.turn_start_seq, turn.start_seq);
        let assistant = terminal.final_assistant.as_ref().unwrap();
        assert_eq!(assistant.api_call_id, api);
        assert_eq!(assistant.source_text.text, text);
        let mut restored = reload(&target);
        restored
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("next-task").unwrap(),
                    "New unrelated work",
                    vec![],
                ),
                10,
            )
            .unwrap();
        assert_ne!(restored.active_turn().unwrap().turn_id, turn.turn_id);
        assert_eq!(
            reload(&restored).collaboration_state().task_results[&message.delivery_id],
            result
        );
    }
}

/// Stopped and abandoned turns retain host outcomes without inventing assistant answers.
/// 停止与放弃回合保留宿主结果，不伪造助手回答。
#[test]
fn task_result_non_success_has_no_fabricated_response() {
    for outcome in [TurnOutcome::Stopped, TurnOutcome::Abandoned] {
        let (mut target, message) = applied_task(CollaborationTaskMessageKind::TaskStart);
        let revision = target.memory_revision();
        target.close_turn(outcome, 1, 9).unwrap();
        let result = &target.collaboration_state().task_results[&message.delivery_id];
        assert_eq!(result.terminal.as_ref().unwrap().outcome, outcome);
        assert!(result.terminal.as_ref().unwrap().final_assistant.is_none());
        assert_eq!(target.memory_revision(), revision + 1);
        assert_eq!(
            reload(&target).collaboration_state().task_results[&message.delivery_id],
            *result
        );
    }
}

/// Missing final API proof or a late operation collision rolls back the complete closure.
/// 缺少最终 API 凭据或末段操作冲突会回滚整个闭合。
#[test]
fn task_result_failure_rolls_back_business_closure() {
    let (mut target, message) = applied_task(CollaborationTaskMessageKind::TaskStart);
    let before = target.clone();
    assert!(target.close_turn(TurnOutcome::Completed, 1, 9).is_err());
    assert_eq!(target, before);
    finish_response(&mut target, "Actual final");
    let effect = &target.collaboration_state().task_message_effects[&message.delivery_id];
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "task-result:{}",
                effect.event_seq.value()
            ))
            .unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 9,
        })
        .unwrap();
    let before = target.clone();
    assert!(target.close_turn(TurnOutcome::Completed, 1, 10).is_err());
    assert_eq!(target, before);
}

/// Editing the source Assistant while retaining result receipts must fail cold reload.
/// 修改来源 Assistant 而保留结果回执时，冷重载必须失败。
#[test]
fn task_result_reload_rejects_divergent_original_assistant() {
    let (mut target, message) = applied_task(CollaborationTaskMessageKind::TaskSteer);
    finish_response(&mut target, "Original final");
    target.close_turn(TurnOutcome::Completed, 1, 9).unwrap();
    let result = &target.collaboration_state().task_results[&message.delivery_id];
    let text_seq = result
        .terminal
        .as_ref()
        .unwrap()
        .final_assistant
        .as_ref()
        .unwrap()
        .source_text
        .event_seq;
    let mut events = target.events().to_vec();
    let EventBody::AssistantText(text) = &mut events
        .iter_mut()
        .find(|event| event.seq == text_seq)
        .unwrap()
        .event
    else {
        panic!("assistant source missing")
    };
    text.text = "Altered final".into();
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

/// Closed effects require results independently of old event residency; active effects cannot finish early.
/// 闭合效果必须拥有结果，不依赖旧事件驻留；活动效果不能提前结束。
#[test]
fn task_result_lifetime_requires_closed_turn_and_complete_snapshot() {
    let (mut target, message) = applied_task(CollaborationTaskMessageKind::TaskStart);
    let active = target.clone();
    target.close_turn(TurnOutcome::Stopped, 1, 9).unwrap();
    let result = target.collaboration_state().task_results[&message.delivery_id].clone();
    let mut premature = active;
    premature
        .metadata
        .collaboration
        .state
        .task_results
        .insert(message.delivery_id, result.clone());
    assert!(matches!(
        premature.validate_task_result_facts(),
        Err(DomainError::CollaborationStateViolation {
            field: "collaboration.task_result.business_lifetime"
        })
    ));
    let mut missing = target;
    missing.metadata.collaboration.state.task_results.clear();
    missing
        .metadata
        .collaboration
        .state
        .operations
        .remove(&result.operation_id);
    assert!(matches!(
        missing.validate_task_result_facts(),
        Err(DomainError::CollaborationStateViolation {
            field: "collaboration.task_result.business_lifetime"
        })
    ));
    let mut wrong_start = result;
    wrong_start.terminal.as_mut().unwrap().turn_start_seq = Seq::try_new(1).unwrap();
    assert!(matches!(
        wrong_start.validate(),
        Err(DomainError::CollaborationStateViolation {
            field: "collaboration.task_result.binding"
        })
    ));
}

/// Steering applied after an old response cannot claim that old text as its final result.
/// 旧回复之后应用的方向消息不能把旧正文认领为自己的最终结果。
#[test]
fn task_result_cannot_reuse_response_that_precedes_steering() {
    let (_, mut target, message) = fixture(CollaborationTaskMessageKind::TaskSteer);
    finish_response(&mut target, "Final before guidance");
    target
        .accept_task_message(
            CollaborationOperationId::try_new("late-accept").unwrap(),
            message.clone(),
            9,
        )
        .unwrap();
    target
        .apply_task_steering_before_api(
            CollaborationOperationId::try_new("late-apply").unwrap(),
            message.delivery_id,
            10,
        )
        .unwrap();
    let before = target.clone();
    assert!(matches!(
        target.close_turn(TurnOutcome::Completed, 1, 11),
        Err(DomainError::CollaborationStateViolation {
            field: "collaboration.task_result.final_assistant"
        })
    ));
    assert_eq!(target, before);
}

/// UTF-8 chunks reconstruct the exact long result and keep routing, size and input keys stable.
/// UTF-8 分块重建精确长结果，保持路由、大小及输入键稳定。
#[test]
fn task_result_envelopes_are_lossless_bounded_and_deterministic() {
    let (mut target, message) = applied_task(CollaborationTaskMessageKind::TaskSteer);
    let text = "中文🙂<&>\n\"quoted\"".repeat(10000);
    finish_response(&mut target, &text);
    target.close_turn(TurnOutcome::Completed, 1, 9).unwrap();
    let mut result = target.collaboration_state().task_results[&message.delivery_id].clone();
    // Maximum-length escaped identities must fit the body but never inflate the input action key.
    // 最大长度转义身份必须容纳于正文，但绝不膨胀输入动作键。
    let long_id =
        CollaborationDeliveryId::try_new("\0".repeat(COLLABORATION_ID_MAX_BYTES)).unwrap();
    result.effect.delivery_id = long_id.clone();
    result.acceptance.message.delivery_id = long_id;
    let receipt_seq = Seq::try_new(crate::v1::ids::JS_SAFE_INTEGER_MAX).unwrap();
    let envelopes = result
        .task_result_envelopes(receipt_seq, CollaborationTaskResultDisclosure::Full)
        .unwrap();
    assert!(envelopes.len() > 1);
    assert_eq!(
        result
            .task_result_envelopes(receipt_seq, CollaborationTaskResultDisclosure::Full)
            .unwrap(),
        envelopes
    );
    let mut restored = String::new();
    for (index, envelope) in envelopes.iter().enumerate() {
        let (header, body) = envelope.body.split_once('\n').unwrap();
        let coordinates: serde_json::Value = serde_json::from_str(header).unwrap();
        assert_eq!(coordinates["version"], 1);
        assert_eq!(coordinates["chunk_index"], index);
        assert_eq!(coordinates["chunk_count"], envelopes.len());
        assert_eq!(coordinates["byte_start"], restored.len());
        restored.push_str(body);
        assert_eq!(coordinates["byte_end"], restored.len());
        assert_eq!(coordinates["total_text_bytes"], text.len());
        assert_eq!(coordinates["outcome"], "completed");
        assert_eq!(envelope.source, "target");
        assert_eq!(envelope.target, "source");
        assert!(envelope.task_id.is_none());
        assert!(envelope.question_id.is_none());
        assert_eq!(
            envelope.kind,
            vulcan_ai_contract::HostMessageKind::TaskResult
        );
        assert!(
            envelope.body.len() <= vulcan_ai_contract::host_message::HOST_MESSAGE_MAX_BODY_BYTES
        );
        ClientActionId::try_new(format!("agent-message:{}", envelope.event_id)).unwrap();
        assert!(envelope.render().unwrap().contains("&lt;&amp;&gt;"));
    }
    assert_eq!(restored, text);
    let other = result
        .task_result_envelopes(
            Seq::try_new(1).unwrap(),
            CollaborationTaskResultDisclosure::Full,
        )
        .unwrap();
    assert_eq!(other.len(), envelopes.len());
    assert!(
        other
            .iter()
            .zip(&envelopes)
            .all(|(left, right)| left.event_id != right.event_id && left.body == right.body)
    );
}

/// Empty success and non-success outcomes retain a single explicit chunk without inventing a reply.
/// 空成功正文与非成功结果保留一个明确块，不编造回复。
#[test]
fn task_result_empty_envelopes_preserve_actual_terminal_kind() {
    for outcome in [
        TurnOutcome::Completed,
        TurnOutcome::Stopped,
        TurnOutcome::Abandoned,
    ] {
        let (mut target, message) = applied_task(CollaborationTaskMessageKind::TaskStart);
        if outcome == TurnOutcome::Completed {
            finish_response(&mut target, "");
        }
        target.close_turn(outcome, 1, 9).unwrap();
        let result = &target.collaboration_state().task_results[&message.delivery_id];
        let envelopes = result
            .task_result_envelopes(
                Seq::try_new(40).unwrap(),
                CollaborationTaskResultDisclosure::Full,
            )
            .unwrap();
        assert_eq!(envelopes.len(), 1);
        let (header, body) = envelopes[0].body.split_once('\n').unwrap();
        let coordinates: serde_json::Value = serde_json::from_str(header).unwrap();
        assert!(body.is_empty());
        assert_eq!(coordinates["chunk_count"], 1);
        assert_eq!(coordinates["byte_start"], 0);
        assert_eq!(coordinates["byte_end"], 0);
        assert_eq!(coordinates["total_text_bytes"], 0);
        assert_eq!(
            coordinates["outcome"],
            serde_json::to_value(outcome).unwrap()
        );
        assert_eq!(
            coordinates["source_text_seq"].is_null(),
            outcome != TurnOutcome::Completed
        );
    }
}
