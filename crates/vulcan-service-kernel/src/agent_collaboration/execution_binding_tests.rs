//! Exact tool-API ownership for in-turn replies and dedicated question executions.
//! 回合内回复与专用答疑执行的精确工具 API 归属。

use super::*;
use vulcan_session_core::v1::collaboration::{
    BeginQuestionExecutionInput, CollaborationExecutionId,
};
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::{PrepareAndStartApiInput, SessionDocument};
use vulcan_session_core::v1::event::{ApiRequestSnapshot, ProviderProjectionProof};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission};

/// Builds an accepted in-turn question using a real target root and delivery reducer.
/// 使用真实目标根回合与投递归约器构建已接纳的回合内问题。
/// Returns the target document and exact source-owned question snapshot for proof checks.
/// 返回目标文档及用于归属校验的精确来源问题快照。
fn accepted_question() -> (
    SessionDocument,
    vulcan_session_core::v1::collaboration::CollaborationQuestionRecord,
) {
    // document retains the original business task throughout every tested reply.
    // document 在所有被测回复期间保留原始业务任务。
    let mut document = SessionDocument::new("target", "project", "Target", 1);
    let turn_id = document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("root-action").unwrap(),
                "Original business task",
                vec![],
            ),
            2,
        )
        .unwrap()
        .into_value();
    document
        .set_collaboration_run_control(vulcan_session_core::v1::collaboration::SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 3,
        })
        .unwrap();
    // question uses the exact target turn admitted above and a separate source owner.
    // question 使用前面准入的精确目标回合以及独立的来源所有者。
    let question = vulcan_session_core::v1::collaboration::CollaborationQuestionRecord {
        operation_id: CollaborationOperationId::try_new("question-operation").unwrap(),
        question_id: CollaborationQuestionId::try_new("question").unwrap(),
        delivery_id: CollaborationDeliveryId::try_new("delivery").unwrap(),
        source: CollaborationParticipant::new("source", "source", "project", 1, None, None)
            .unwrap(),
        target: CollaborationParticipant::new(
            "target",
            "target",
            "project",
            1,
            Some(turn_id),
            None,
        )
        .unwrap(),
        body: String::from("Which function owns this behavior?"),
        execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
        state: CollaborationQuestionState::Pending,
        answer: None,
        cancellation: None,
        created_at_ms: 3,
    };
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            question_id: question.question_id.clone(),
            delivery_id: question.delivery_id.clone(),
            source: question.source.clone(),
            target: question.target.clone(),
            bound_turn_id: Some(turn_id),
            execution_purpose: question.execution_purpose,
            accepted_at_ms: 4,
        })
        .unwrap();
    (document, question)
}

/// Starts one real physical request from the document's current projection frontier.
/// 从文档当前投影前沿启动一次真实物理请求。
/// Parameters select the document and declared purpose; returns the admitted API identity.
/// 参数选择文档及声明用途；返回准入的 API 身份。
fn start_api(document: &mut SessionDocument, purpose: ApiPurposeKind) -> ApiCallId {
    // through_seq fixes the proof to the last authoritative fact before this request.
    // through_seq 将证明固定到本次请求之前的最后一条权威事实。
    let through_seq = document.events().last().unwrap().seq;
    document
        .prepare_and_start_api(PrepareAndStartApiInput {
            purpose,
            request: ApiRequestSnapshot::new("openai", "model"),
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection: ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .unwrap(),
            },
            locked_baseline: LockedBaselineProjection::empty(),
            continuation_action_id: None,
            now_ms: 6,
        })
        .unwrap()
        .into_value()
        .api_call_id
}

/// A running business API can answer its projected question without becoming an auxiliary turn.
/// 正在运行的业务 API 可回答已投影问题，无需变成辅助回合。
#[test]
fn business_reply_uses_the_original_turn_and_projection_record() {
    for purpose in [
        ApiPurposeKind::MainGeneration,
        ApiPurposeKind::RetryGeneration,
        ApiPurposeKind::Subagent,
    ] {
        // Each purpose owns an isolated document, preventing one API from masking another.
        // 每种用途持有隔离文档，防止一个 API 掩盖另一个。
        let (mut document, question) = accepted_question();
        document
            .project_question_before_api(question.clone(), 5)
            .unwrap();
        let api_id = start_api(&mut document, purpose);
        assert_eq!(document.collaboration_state().question_executions.len(), 1);
        assert_eq!(document.active_turn().unwrap().origin, TurnOrigin::UserRoot);
        assert!(
            prove_active_question_execution_binding(
                &document,
                &question,
                &question.target,
                question.target.turn_id.unwrap(),
                api_id
            )
            .is_ok()
        );
    }
}

/// Physical and logical identities remain mandatory even for ordinary in-turn replies.
/// 即使是普通回合内回复，物理与逻辑身份也必须精确匹配。
#[test]
fn reply_rejects_stale_api_and_wrong_participant_without_mutation() {
    let (mut document, question) = accepted_question();
    document
        .project_question_before_api(question.clone(), 5)
        .unwrap();
    let api_id = start_api(&mut document, ApiPurposeKind::MainGeneration);
    // before retains the authoritative history and active API for rejection checks.
    // before 保留权威历史及活动 API，用于拒绝检查。
    let before = (
        document.memory_revision(),
        document.events().to_vec(),
        document.active_api().cloned(),
    );
    assert!(
        prove_active_question_execution_binding(
            &document,
            &question,
            &question.target,
            question.target.turn_id.unwrap(),
            ApiCallId::try_new(api_id.value() + 100).unwrap()
        )
        .is_err()
    );
    assert!(
        prove_active_question_execution_binding(
            &document,
            &question,
            &question.source,
            question.target.turn_id.unwrap(),
            api_id
        )
        .is_err()
    );
    assert!(
        prove_active_question_execution_binding(
            &document,
            &question,
            &question.target,
            TurnId::try_new(999).unwrap(),
            api_id
        )
        .is_err()
    );
    assert_eq!(
        before,
        (
            document.memory_revision(),
            document.events().to_vec(),
            document.active_api().cloned()
        )
    );
}

/// An API gap or compaction never grants authority to reply on a retained business turn.
/// API 空档或压缩不会授予保留业务回合上的回复权限。
#[test]
fn reply_rejects_missing_api_and_compaction() {
    // document has a legitimate projected question but initially no active physical API.
    // document 拥有合法的已投影问题，但起初没有活动物理 API。
    let (mut document, question) = accepted_question();
    document
        .project_question_before_api(question.clone(), 5)
        .unwrap();
    assert!(
        prove_active_question_execution_binding(
            &document,
            &question,
            &question.target,
            question.target.turn_id.unwrap(),
            ApiCallId::try_new(999).unwrap(),
        )
        .is_err()
    );
    // api_id belongs to a real compaction group, not an invented stale identifier.
    // api_id 属于真实压缩组，并非人为构造的过期标识。
    let api_id = start_api(&mut document, ApiPurposeKind::Compaction);
    assert!(
        prove_active_question_execution_binding(
            &document,
            &question,
            &question.target,
            question.target.turn_id.unwrap(),
            api_id,
        )
        .is_err()
    );
}

/// Source settlement is queryable before the target's original business execution finishes.
/// 来源结算可以在目标原始业务执行结束前查询。
#[test]
fn settled_question_query_does_not_wait_for_business_completion() {
    let (mut document, mut question) = accepted_question();
    document
        .project_question_before_api(question.clone(), 5)
        .unwrap();
    start_api(&mut document, ApiPurposeKind::MainGeneration);
    question.state = CollaborationQuestionState::Cancelled;
    question.cancellation = Some(
        vulcan_session_core::v1::collaboration::CollaborationCancellationRecord {
            operation_id: CollaborationOperationId::try_new("cancel").unwrap(),
            requester: question.source.clone(),
            reason: String::from("No longer needed"),
            cancelled_at_ms: 7,
        },
    );
    assert!(
        document
            .collaboration_state()
            .question_executions
            .values()
            .all(|execution| execution.state == CollaborationQuestionExecutionState::Running)
    );
    assert!(prove_settled_question_delivery(&document, &question).is_ok());
    assert!(document.active_api().is_some());
    assert!(!document.active_turn().unwrap().closed);
}

/// A settled query cannot borrow another delivery's acceptance proof.
/// 已结算查询不能借用其他投递的接纳证明。
#[test]
fn settled_query_requires_exact_delivery_identity() {
    let (document, mut question) = accepted_question();
    assert!(prove_settled_question_delivery(&document, &question).is_err());
    question.state = CollaborationQuestionState::Cancelled;
    question.cancellation = Some(
        vulcan_session_core::v1::collaboration::CollaborationCancellationRecord {
            operation_id: CollaborationOperationId::try_new("cancel").unwrap(),
            requester: question.source.clone(),
            reason: String::from("No longer needed"),
            cancelled_at_ms: 7,
        },
    );
    // Cancellation before projection is readable without ever starting a question execution.
    // 投影前撤销的问题可直接查询，无须启动任何问题处理执行。
    assert!(
        document
            .collaboration_state()
            .question_executions
            .is_empty()
    );
    assert!(prove_settled_question_delivery(&document, &question).is_ok());
    question.delivery_id = CollaborationDeliveryId::try_new("different-delivery").unwrap();
    assert!(prove_settled_question_delivery(&document, &question).is_err());
}

/// A dedicated QA API still requires its exact running execution record.
/// 专用答疑 API 仍要求精确的运行中执行记录。
#[test]
fn auxiliary_reply_requires_the_bound_execution() {
    let (mut document, question) = accepted_question();
    document
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("execution-start").unwrap(),
            execution_id: CollaborationExecutionId::try_new("execution").unwrap(),
            question_id: question.question_id.clone(),
            participant: question.target.clone(),
            purpose: question.execution_purpose,
            parent_turn_id: question.target.turn_id,
            started_at_ms: 5,
        })
        .unwrap();
    let api_id = start_api(&mut document, ApiPurposeKind::QuestionExecution);
    assert!(
        prove_active_question_execution_binding(
            &document,
            &question,
            &question.target,
            question.target.turn_id.unwrap(),
            api_id
        )
        .is_ok()
    );
    // A second accepted question shares the parent turn but not this physical execution's ownership.
    // 第二个已接纳问题共享父回合，但不共享本次物理执行的归属。
    let mut other = question;
    other.question_id = CollaborationQuestionId::try_new("other-question").unwrap();
    other.delivery_id = CollaborationDeliveryId::try_new("other-delivery").unwrap();
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("other-accept").unwrap(),
            question_id: other.question_id.clone(),
            delivery_id: other.delivery_id.clone(),
            source: other.source.clone(),
            target: other.target.clone(),
            bound_turn_id: other.target.turn_id,
            execution_purpose: other.execution_purpose,
            accepted_at_ms: 7,
        })
        .unwrap();
    assert!(
        prove_active_question_execution_binding(
            &document,
            &other,
            &other.target,
            other.target.turn_id.unwrap(),
            api_id
        )
        .is_err()
    );
}
