//! In-turn question records must not prevent the original child-return continuation.
//! 回合内答疑记录不得阻塞原业务的子任务回流续跑。

use super::*;
use vulcan_session_core::v1::collaboration::{
    AcceptDeliveryInput, CollaborationDeliveryId, CollaborationExecutionId,
    CollaborationExecutionPurpose, CollaborationParticipant, CollaborationQuestionExecutionState,
    CollaborationQuestionId, CollaborationTargetSettlementState, CreateQuestionInput,
    ReplyQuestionInput, SettleInboundDeliveryInput,
};

/// Projects a real source question into the parent and optionally records its winning answer.
/// 将真实来源问题投影进父回合，并可选记录已胜出的回答。
fn project_parent_question(parent: &SessionHandle, answered: bool) -> CollaborationExecutionId {
    let target = parent
        .read(|document| {
            CollaborationParticipant::new(
                document.metadata.session_id.clone(),
                document.metadata.session_id.clone(),
                document.metadata.project_tag.clone(),
                document.segment_generation(),
                Some(document.active_turn().unwrap().turn_id),
                None,
            )
        })
        .unwrap()
        .unwrap();
    let mut source = SessionDocument::new("hook-question-source", &target.project_id, "Source", 1);
    let source_turn = source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("hook-question-source-root").unwrap(),
                "Source work",
                Vec::new(),
            ),
            2,
        )
        .unwrap()
        .into_value();
    let source_participant = CollaborationParticipant::new(
        "hook-question-source",
        "hook-question-source",
        &target.project_id,
        1,
        Some(source_turn),
        None,
    )
    .unwrap();
    let question = source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("hook-question-create").unwrap(),
            question_id: CollaborationQuestionId::try_new("hook-question").unwrap(),
            delivery_id: CollaborationDeliveryId::try_new("hook-question-delivery").unwrap(),
            source: source_participant,
            target: target.clone(),
            body: "Question fixture".into(),
            execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
            created_at_ms: 4,
        })
        .unwrap()
        .into_value()
        .question;
    let execution = parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.apply_atomic_transition(|document| {
                    document.accept_delivery(AcceptDeliveryInput {
                        operation_id: CollaborationOperationId::try_new("hook-question-accept")?,
                        question_id: question.question_id.clone(),
                        delivery_id: question.delivery_id.clone(),
                        source: question.source.clone(),
                        target: target.clone(),
                        bound_turn_id: target.turn_id,
                        execution_purpose: question.execution_purpose,
                        accepted_at_ms: 5,
                    })?;
                    document
                        .prepare_question_execution_before_api(question.clone(), 6)
                        .map(|receipt| receipt.into_value().execution.execution_id)
                })
            },
        )
        .unwrap()
        .value;
    if answered {
        let reply_operation = CollaborationOperationId::try_new("hook-question-reply").unwrap();
        let reply = source
            .reply_question(ReplyQuestionInput {
                operation_id: reply_operation.clone(),
                question_id: question.question_id.clone(),
                responder: target,
                body: "Answer fixture".into(),
                answered_at_ms: 7,
            })
            .unwrap()
            .into_value();
        parent
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.settle_inbound_delivery(SettleInboundDeliveryInput {
                        operation_id: CollaborationOperationId::try_new("hook-question-settle")?,
                        delivery_id: question.delivery_id,
                        question_id: question.question_id,
                        source_operation_id: reply_operation,
                        settlement: CollaborationTargetSettlementState::Answered,
                        source_event_seq: reply.event_seq,
                        acknowledged_at_ms: 7,
                    })
                },
            )
            .unwrap();
    }
    execution
}

/// Answered and unanswered in-turn questions retain history without claiming a second API owner.
/// 已答及未答的回合内问题保留历史，但不占用第二个 API 所有者。
#[test]
fn child_return_hook_recovery_with_in_turn_questions() {
    for answered in [false, true] {
        let (_directory, parent) = stored_handle();
        parent
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("question-parent-root").unwrap(),
                    "Parent work",
                    Vec::new(),
                ),
                2,
            )
            .unwrap();
        set_control(&parent, CollaborationRunControl::Running, 3);
        let original_runtime = crate::tests::test_support::test_agent_runtime_bridge();
        let original_hook = prepare_hook(
            &original_runtime,
            parent.session_id(),
            PendingSessionPromptSource::UserFollowup,
            "Parent work",
        );
        crate::session_business_hook::capture_business_hook_before_dispatch(
            &parent,
            &original_runtime,
            parent.session_id(),
            &original_hook,
        )
        .unwrap();
        let execution_id = project_parent_question(&parent, answered);
        let returned = parent
            .append_subagent_return(child_result(), 8)
            .unwrap()
            .value;
        // A cold writer must reproduce the failure, not just an already prepared in-memory Hook.
        // 冷态写入器必须覆盖此故障，不能仅测试内存中已准备的 Hook。
        let locator = parent.storage_locator().unwrap();
        let restored = SessionHandle::new(
            parent.session_id(),
            locator.clone(),
            load_session_v1_full(&locator).unwrap().document,
        );
        let authority =
            super::super::super::pending_prepare::subagent_return_dispatch_authority_from_v1_handle(
                &restored, &returned,
            )
            .unwrap();
        let before = restored
            .read_full_durable(|document| {
                let execution = &document.collaboration_state().question_executions[&execution_id];
                assert_eq!(
                    execution.state,
                    CollaborationQuestionExecutionState::Running
                );
                assert_eq!(execution.parent_turn_id, Some(returned.parent_turn_id));
                assert!(execution.execution_turn_id.is_none());
                serde_json::to_value((&document.metadata, document.events())).unwrap()
            })
            .unwrap();
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        for _ in 0..2 {
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::SubagentReturn(&authority),
            )
            .unwrap();
            assert_eq!(
                prepare_hook(
                    &runtime,
                    restored.session_id(),
                    PendingSessionPromptSource::SubagentReturnContinuation,
                    "Return fixture",
                ),
                original_hook
            );
            assert_eq!(
                restored
                    .read_full_durable(|document| {
                        serde_json::to_value((&document.metadata, document.events())).unwrap()
                    })
                    .unwrap(),
                before
            );
        }
        // Corrupt only detached snapshots: a foreign or standalone owner must still fail closed.
        // 仅破坏分离快照：外来或独立所有者仍必须拒绝恢复。
        for (purpose, parent_turn_id, execution_turn_id) in [
            (
                CollaborationExecutionPurpose::NaturallyIdleQuestion,
                None,
                Some(returned.parent_turn_id),
            ),
            (CollaborationExecutionPurpose::InTurnQuestion, None, None),
            (
                CollaborationExecutionPurpose::InTurnQuestion,
                Some(returned.parent_turn_id),
                Some(returned.parent_turn_id),
            ),
        ] {
            let mut invalid = restored.read_full_durable(Clone::clone).unwrap();
            let execution = invalid
                .metadata
                .collaboration
                .state
                .question_executions
                .get_mut(&execution_id)
                .unwrap();
            execution.purpose = purpose;
            execution.parent_turn_id = parent_turn_id;
            execution.execution_turn_id = execution_turn_id;
            assert!(matches!(
                invalid.main_business_recovery_state(),
                Err(
                    vulcan_session_core::v1::errors::DomainError::CollaborationStateViolation {
                        field: "business_hook.recovery_owner",
                    }
                )
            ));
        }
        // Even an answered question does not waive the physical request's exclusive ownership.
        // 即使问题已经回答，也不能豁免物理请求的排他所有权。
        let request = restored
            .read_full_durable(|document| {
                let through_seq = document.events().last().unwrap().seq;
                PrepareAndStartApiInput {
                    purpose: ApiPurposeKind::QuestionExecution,
                    request: ApiRequestSnapshot::new("openai", "model"),
                    request_budget_evidence:
                        crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                    projection: ProviderProjectionProof {
                        segment_generation: document.segment_generation(),
                        document_revision: document.memory_revision(),
                        retained_from_seq: Seq::try_new(1).unwrap(),
                        through_seq,
                        fingerprint_sha256: provider_visible_fingerprint(
                            document.events(),
                            through_seq,
                        )
                        .unwrap(),
                    },
                    locked_baseline: LockedBaselineProjection::empty(),
                    continuation_action_id: None,
                    now_ms: 9,
                }
            })
            .unwrap();
        restored.prepare_and_start_api_durable(request).unwrap();
        assert!(
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::SubagentReturn(&authority),
            )
            .is_err()
        );
        restored
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| document.interrupt_active_api_for_process(10),
            )
            .unwrap();
        set_control(&restored, CollaborationRunControl::Stopped, 11);
        assert!(
            prepare_internal_continuation_hook_owner(
                &restored,
                &runtime,
                InternalContinuationHookSource::SubagentReturn(&authority),
            )
            .is_err()
        );
    }
}
