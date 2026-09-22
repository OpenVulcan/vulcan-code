//! Stop proof and settlement preserve the business turn and exact physical interruption.
//! 停止证明与结算保留业务回合及精确物理中断。

use super::*;
use vulcan_session_core::v1::collaboration::*;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::PrepareAndStartApiInput;
use vulcan_session_core::v1::event::ApiRequestSnapshot;
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::*;

/// Creates an actual running auxiliary execution attached to either a retained parent or its own turn.
/// 创建附着保留父回合或独立回合的真实运行中辅助执行。
/// `in_parent` selects ownership and `api` optionally starts a physical request. Returns its document and ID.
/// `in_parent` 选择归属，`api` 可选启动物理请求。返回文档及执行 ID。
fn stop_fixture(
    in_parent: bool,
    api: Option<ApiPurposeKind>,
) -> (SessionDocument, CollaborationExecutionId) {
    // document is a non-user isolated domain fixture.
    // document 是不涉及用户数据的隔离领域夹具。
    let mut document = SessionDocument::new("target", "project", "Question stop", 1);
    // parent_turn exists only for an in-turn question.
    // parent_turn 仅在回合内问题时存在。
    let parent_turn = if in_parent {
        let turn = document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root").unwrap(),
                    "Original business",
                    vec![],
                ),
                2,
            )
            .unwrap()
            .into_value();
        document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("running").unwrap(),
                state: CollaborationRunControl::Running,
                changed_at_ms: 3,
            })
            .unwrap();
        Some(turn)
    } else {
        None
    };
    // purpose and target carry the exact reducer-proven turn binding.
    // purpose 与 target 携带归约器证明的精确回合绑定。
    let purpose = if in_parent {
        CollaborationExecutionPurpose::InTurnQuestion
    } else {
        CollaborationExecutionPurpose::NaturallyIdleQuestion
    };
    let target =
        CollaborationParticipant::new("target", "target", "project", 1, parent_turn, None).unwrap();
    let question_id = CollaborationQuestionId::try_new("question").unwrap();
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            delivery_id: CollaborationDeliveryId::try_new("delivery").unwrap(),
            question_id: question_id.clone(),
            source: CollaborationParticipant::new("source", "source", "project", 1, None, None)
                .unwrap(),
            target: target.clone(),
            bound_turn_id: parent_turn,
            execution_purpose: purpose,
            accepted_at_ms: 4,
        })
        .unwrap();
    // execution_id is the same identity later captured from the kernel.
    // execution_id 是随后从内核捕获的相同身份。
    let execution_id = CollaborationExecutionId::try_new("execution").unwrap();
    document
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin").unwrap(),
            execution_id: execution_id.clone(),
            question_id,
            participant: target,
            purpose,
            parent_turn_id: parent_turn,
            started_at_ms: 5,
        })
        .unwrap();
    if let Some(purpose) = api {
        // through_seq fixes the real projection before the physical API starts.
        // through_seq 固定物理 API 启动前的真实投影。
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
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 6,
            })
            .unwrap();
    }
    (document, execution_id)
}

/// Both preparation and streaming stops settle the question without finishing its retained parent.
/// 准备期与流执行期停止均结算问题，但不结束保留父回合。
#[test]
fn question_stop_settles_exact_auxiliary_and_preserves_parent() {
    for in_parent in [false, true] {
        for with_api in [false, true] {
            let (mut document, execution_id) = stop_fixture(
                in_parent,
                with_api.then_some(ApiPurposeKind::QuestionExecution),
            );
            let scope = question_stop_scope(&document, &execution_id).unwrap();
            let mode = document.metadata.mode.clone();
            // User stop freezes control before publishing the auxiliary terminal intent.
            // 用户停止在发布辅助终态意图之前冻结控制状态。
            document
                .set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("stop-control").unwrap(),
                    state: CollaborationRunControl::MainOnly,
                    changed_at_ms: 7,
                })
                .unwrap();
            let record = TerminalSettlementRecord {
                settlement_id: String::from("stop-settlement"),
                session_id: String::from("target"),
                turn_id: document.active_turn().unwrap().turn_id,
                scope,
                owner_generation: String::from("run-stop:1"),
                state: TerminalSettlementState::Pending,
                reason_code: String::from("question_execution_stopped"),
                updated_at_ms: 7,
            };
            document
                .begin_terminal_settlement(record.clone(), 7)
                .unwrap();
            // The durable intent precedes interruption, matching production failure-recovery order.
            // 可靠意图先于中断，符合生产失败恢复顺序。
            if with_api {
                document.interrupt_active_api(8).unwrap();
            }
            document
                .fail_question_execution_for_terminal_settlement(&record, None, 9)
                .unwrap();
            assert_eq!(document.metadata.mode, mode);
            assert_eq!(document.active_turn().unwrap().closed, !in_parent);
            assert_eq!(
                document
                    .collaboration_state()
                    .question_executions
                    .get(&execution_id)
                    .unwrap()
                    .outcome,
                Some(CollaborationQuestionExecutionOutcome::Failed)
            );
            if with_api {
                assert!(document.events().iter().any(|event| matches!(&event.event,
                    EventBody::ApiStats(stats) if stats.purpose == ApiPurposeKind::QuestionExecution
                        && stats.status == ApiCallStatus::Interrupted)));
            }
            // Reload only serialized domain authority; no kernel or process-local repair cache is retained.
            // 仅重载序列化领域权威，不保留内核或进程内修复缓存。
            document = SessionDocument::from_parts(
                serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
                document.runtime_lock.clone(),
                document.compactions.clone(),
                document.segment_catalog().to_vec(),
                document.events().to_vec(),
            )
            .unwrap();
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainOnly
            );
            let revision = document.memory_revision();
            assert!(
                !document
                    .fail_question_execution_for_terminal_settlement(&record, None, 10)
                    .unwrap()
            );
            assert_eq!(revision, document.memory_revision());
        }
    }
}

/// A kernel question binding cannot interrupt a different-purpose business API.
/// 内核问题绑定不能中断不同用途的业务 API。
#[test]
fn question_stop_rejects_business_api_before_mutation() {
    let (document, execution_id) = stop_fixture(true, Some(ApiPurposeKind::MainGeneration));
    let before = (
        document.memory_revision(),
        document.events().to_vec(),
        document.active_api().cloned(),
    );
    assert!(question_stop_scope(&document, &execution_id).is_err());
    assert_eq!(
        before,
        (
            document.memory_revision(),
            document.events().to_vec(),
            document.active_api().cloned()
        )
    );
}
