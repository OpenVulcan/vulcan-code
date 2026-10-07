//! Adversarial regression coverage for source and target collaboration state.
//! 来源与目标协作状态的对抗式回归覆盖。

use super::*;
use crate::v1::document::ClientInputSubmission;
use crate::v1::event::{EventBody, TurnOutcome};
use crate::v1::ids::{ClientActionId, JS_SAFE_INTEGER_MAX, PendingInputId};

/// Fixed semantic time used by every collaboration fixture.
/// 所有协作 fixture 使用的固定语义时间。
const T0: i64 = 1_700_000_000_000;

/// Business failure closes in-turn attempts and publishes idle control without undoing explicit stop.
/// 业务失败闭合回合内处理尝试并发布空闲控制，不撤销明确停止。
#[test]
fn business_failure_settles_question_attempts_and_preserves_stop_control() {
    for control in [
        CollaborationRunControl::Running,
        CollaborationRunControl::MainOnly,
        CollaborationRunControl::Stopped,
        CollaborationRunControl::MainAndSubagents,
    ] {
        for with_api in [false, true] {
            // Each case owns a real projected optional question and independent business turn.
            // 每个场景拥有真实已投影可选问题与独立业务回合。
            let (mut target, question) = question_preparation_fixture(true);
            let execution = target
                .prepare_question_execution_before_api(question.clone(), T0 + 3)
                .unwrap()
                .into_value()
                .execution;
            if with_api {
                target
                    .start_api(
                        crate::v1::event::ApiPurposeKind::MainGeneration,
                        crate::v1::event::ApiRequestSnapshot::new("provider", "model"),
                        T0 + 4,
                    )
                    .unwrap();
            }
            target
                .set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("failure-control").unwrap(),
                    state: control,
                    changed_at_ms: T0 + 5,
                })
                .unwrap();
            let revision = target.memory_revision();
            assert!(
                target
                    .fail_execution_boundary_and_close_turn("business_failure", None, T0 + 6)
                    .unwrap()
            );
            assert_eq!(target.memory_revision(), revision + 1);
            assert!(target.active_api().is_none());
            assert!(target.active_turn().unwrap().closed);
            assert_eq!(
                target.collaboration_run_control(),
                if control == CollaborationRunControl::Running {
                    CollaborationRunControl::NaturallyIdle
                } else {
                    control
                }
            );
            let ended = &target.collaboration_state().question_executions[&execution.execution_id];
            assert_eq!(ended.state, CollaborationQuestionExecutionState::Completed);
            assert_eq!(
                ended.outcome,
                Some(CollaborationQuestionExecutionOutcome::Unanswered)
            );
            assert!(
                target.collaboration_state().inbound_deliveries[&question.delivery_id]
                    .settlement
                    .is_none()
            );
            assert_eq!(question.state, CollaborationQuestionState::Pending);
            // Reload validates embedded receipt revisions as well as events and derived ownership.
            // 重载同时校验回执内修订、事件及派生所有权。
            let reloaded = SessionDocument::from_parts(
                serde_json::from_str(&serde_json::to_string(&target.metadata).unwrap()).unwrap(),
                target.runtime_lock.clone(),
                target.compactions.clone(),
                target.segment_catalog().to_vec(),
                target.events().to_vec(),
            )
            .unwrap();
            assert!(reloaded.active_turn().is_none());
            assert_eq!(reloaded.collaboration_state(), target.collaboration_state());
            let before = target.clone();
            assert!(
                !target
                    .fail_execution_boundary_and_close_turn("business_failure", None, T0 + 7)
                    .unwrap()
            );
            assert_eq!(target, before);
        }
    }
}

/// Process interruption retains both the business and question attempt for explicit Continue.
/// 进程中断为显式 Continue 同时保留业务与问题处理尝试。
#[test]
fn business_failure_after_process_interruption_preserves_question_attempt() {
    let (mut target, question) = question_preparation_fixture(true);
    target
        .prepare_question_execution_before_api(question, T0 + 3)
        .unwrap();
    target
        .start_api(
            crate::v1::event::ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("provider", "model"),
            T0 + 4,
        )
        .unwrap();
    target.interrupt_active_api_for_process(T0 + 5).unwrap();
    let before = target.clone();
    assert!(
        !target
            .fail_execution_boundary_and_close_turn("business_failure", None, T0 + 6)
            .unwrap()
    );
    assert_eq!(target, before);
    assert_eq!(
        target.collaboration_run_control(),
        CollaborationRunControl::Running
    );
    assert!(!target.active_turn().unwrap().closed);
}

/// A late idle-control identity conflict rolls back API, question and business closure together.
/// 迟到空闲控制身份冲突同时回滚 API、问题处理尝试及业务闭合。
#[test]
fn business_failure_rolls_back_late_collaboration_control_conflict() {
    let (mut target, question) = question_preparation_fixture(true);
    target
        .prepare_question_execution_before_api(question, T0 + 3)
        .unwrap();
    let turn_id = target.active_turn().unwrap().turn_id;
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "business-failure-idle:{}:{}",
                target.segment_generation(),
                turn_id.value()
            ))
            .unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: T0 + 4,
        })
        .unwrap();
    target
        .start_api(
            crate::v1::event::ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("provider", "model"),
            T0 + 5,
        )
        .unwrap();
    let before = target.clone();
    assert!(matches!(
        target.fail_execution_boundary_and_close_turn("business_failure", None, T0 + 6),
        Err(DomainError::CollaborationConflict { .. })
    ));
    assert_eq!(target, before);
}

/// Builds an accepted question with a real business turn or an idle target for atomic launch tests.
/// 为原子启动测试创建已接纳问题，目标绑定真实业务回合或自然空闲状态。
/// `in_turn` selects the purpose; returns the target document and authoritative source question.
/// `in_turn` 选择用途；返回目标文档与来源权威问题。
fn question_preparation_fixture(in_turn: bool) -> (SessionDocument, CollaborationQuestionRecord) {
    // Keep both participants backed by actual documents and allocated turn identities.
    // 双方参与者均来自实际文档与已分配回合身份。
    let mut source = SessionDocument::new("prepare-source", "project-1", "source", T0);
    let source_turn = open_root_turn(&mut source, "prepare-source-input", "source work");
    let mut target = SessionDocument::new("prepare-target", "project-1", "target", T0);
    let purpose = if in_turn {
        CollaborationExecutionPurpose::InTurnQuestion
    } else {
        CollaborationExecutionPurpose::NaturallyIdleQuestion
    };
    let target_identity = if in_turn {
        let turn = open_root_turn(&mut target, "prepare-target-input", "target work");
        target
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("prepare-running").unwrap(),
                state: CollaborationRunControl::Running,
                changed_at_ms: T0,
            })
            .unwrap();
        source_participant(&target, turn)
    } else {
        target_participant(&target)
    };
    let source_identity = source_participant(&source, source_turn);
    let question_id = CollaborationQuestionId::try_new("prepare-question").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("prepare-delivery").unwrap();
    source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("prepare-create").unwrap(),
            question_id: question_id.clone(),
            delivery_id: delivery_id.clone(),
            source: source_identity.clone(),
            target: target_identity.clone(),
            body: String::from("Optional progress question."),
            execution_purpose: purpose,
            created_at_ms: T0 + 1,
        })
        .unwrap();
    target
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("prepare-accept").unwrap(),
            question_id: question_id.clone(),
            delivery_id,
            source: source_identity,
            bound_turn_id: target_identity.turn_id,
            target: target_identity,
            execution_purpose: purpose,
            accepted_at_ms: T0 + 2,
        })
        .unwrap();
    (
        target,
        source.question_snapshot(&question_id).unwrap().clone(),
    )
}

/// Explicit stop must return a failed delivery for an accepted question not yet projected.
/// 明确停止后，尚未投影的已接纳问题必须得到失败投递回执。
#[test]
fn stopped_target_fails_accepted_unprojected_question() {
    for control in [
        CollaborationRunControl::Stopped,
        CollaborationRunControl::MainAndSubagents,
    ] {
        let (mut target, question) = question_preparation_fixture(true);
        target
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("stop-accepted-question").unwrap(),
                state: control,
                changed_at_ms: T0 + 3,
            })
            .unwrap();
        let inbound = target
            .inbound_delivery_snapshot(&question.delivery_id)
            .unwrap();
        assert_eq!(inbound.state, CollaborationDeliveryState::Failed);
        assert_eq!(
            inbound.failure,
            Some(CollaborationDeliveryFailureReason::TargetStopped)
        );
    }
    let (mut parent, question) = question_preparation_fixture(true);
    parent
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("main-only-accepted-question").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: T0 + 3,
        })
        .unwrap();
    assert_eq!(
        parent
            .inbound_delivery_snapshot(&question.delivery_id)
            .unwrap()
            .state,
        CollaborationDeliveryState::Accepted
    );
}

/// Atomic preparation publishes all three facts once and replays after persistent reconstruction.
/// 原子准备只发布一次三个事实，并在持久化重建后幂等重放。
#[test]
fn question_preparation_publishes_once_and_reloads() {
    for in_turn in [false, true] {
        // Revision assertions include the execution start, User envelope and projection receipt.
        // 修订断言包含执行启动、User 信封与投影回执。
        let (mut target, question) = question_preparation_fixture(in_turn);
        let before_revision = target.memory_revision();
        let receipt = target
            .prepare_question_execution_before_api(question.clone(), T0 + 3)
            .unwrap()
            .into_value();
        assert_eq!(target.memory_revision(), before_revision + 1);
        assert_eq!(
            target
                .memory_revision_receipt(target.memory_revision())
                .unwrap()
                .changed_event_seqs
                .len(),
            3
        );
        assert_eq!(
            target
                .inbound_delivery_snapshot(&question.delivery_id)
                .unwrap()
                .target_revision,
            target.memory_revision()
        );
        assert_eq!(receipt.execution.parent_turn_id.is_some(), in_turn);
        assert_eq!(receipt.execution.execution_turn_id.is_some(), !in_turn);
        assert!(target.events().iter().any(|event| matches!(&event.event,
            EventBody::UserInput(input) if input.text.contains("Optional progress question.")
                && input.input_source == crate::v1::event::UserInputSource::AgentMessage)));
        // Reload the serialized authority rather than relying on a process-local replay cache.
        // 重载序列化权威，不能依赖进程内重放缓存。
        let mut reloaded = SessionDocument::from_parts(
            serde_json::from_str(&serde_json::to_string(&target.metadata).unwrap()).unwrap(),
            target.runtime_lock.clone(),
            target.compactions.clone(),
            target.segment_catalog().to_vec(),
            target.events().to_vec(),
        )
        .unwrap();
        let before = reloaded.clone();
        let replay = reloaded
            .prepare_question_execution_before_api(question, T0 + 4)
            .unwrap();
        assert!(!replay.changed());
        assert_eq!(replay.into_value(), receipt);
        assert_eq!(reloaded, before);
    }
}

/// A failure after staging the execution must preserve the entire live document.
/// 暂存执行后发生错误时必须保持完整当前文档不变。
#[test]
fn question_preparation_rolls_back_late_projection_failure() {
    for in_turn in [false, true] {
        // A forged source passes start ownership but fails the exact delivery projection proof.
        // 伪造来源通过启动所有权检查，但无法通过精确投递投影凭据。
        let (mut target, mut question) = question_preparation_fixture(in_turn);
        question.source.session_id = String::from("forged-source");
        let before = target.clone();
        assert!(
            target
                .prepare_question_execution_before_api(question, T0 + 3)
                .is_err()
        );
        assert_eq!(target, before);
    }
}

/// Launch admission rejects historical successful receipts after stop or execution completion.
/// 启动准入拒绝停止或执行结束之后的历史成功回执。
#[test]
fn question_preparation_rejects_stopped_and_completed_replays() {
    for in_turn in [false, true] {
        // Preserve an active snapshot to check stop independently of natural completion.
        // 保留活动快照，独立于自然完成检查停止。
        let (mut target, question) = question_preparation_fixture(in_turn);
        let receipt = target
            .prepare_question_execution_before_api(question.clone(), T0 + 3)
            .unwrap()
            .into_value();
        let mut stopped = target.clone();
        stopped
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("prepare-stop").unwrap(),
                state: CollaborationRunControl::Stopped,
                changed_at_ms: T0 + 4,
            })
            .unwrap();
        let stopped_before = stopped.clone();
        assert!(
            stopped
                .prepare_question_execution_before_api(question.clone(), T0 + 5)
                .is_err()
        );
        assert_eq!(stopped, stopped_before);
        target
            .complete_question_execution(CompleteQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("prepare-complete").unwrap(),
                execution_id: receipt.execution.execution_id,
                outcome: CollaborationQuestionExecutionOutcome::Unanswered,
                completed_at_ms: T0 + 4,
            })
            .unwrap();
        let completed_before = target.clone();
        assert!(
            target
                .prepare_question_execution_before_api(question, T0 + 5)
                .is_err()
        );
        assert_eq!(target, completed_before);
    }
}

/// Composite collaboration drafts roll back late errors and reject premature writer publication.
/// 协作复合草稿回滚后续错误，并拒绝过早向写入器发布。
#[test]
fn atomic_collaboration_draft_preserves_owner_on_late_failure() {
    // A clean owner allows exact equality checks across failed transactions.
    // 干净所有者允许对失败事务前后进行精确相等检查。
    let mut document = SessionDocument::new("atomic-owner", "project-1", "owner", T0);
    // The same operation identity cannot denote both Running and Stopped.
    // 同一操作身份不能同时表示 Running 和 Stopped。
    let input = SetRunControlInput {
        operation_id: CollaborationOperationId::try_new("atomic-control").unwrap(),
        state: CollaborationRunControl::Running,
        changed_at_ms: T0 + 1,
    };
    // Snapshot includes allocator, receipts, dirty files and the transient revision reservation.
    // 快照包含分配器、回执、脏文件以及临时修订预留。
    let before = document.clone();
    assert!(
        document
            .apply_atomic_transition(|staged| {
                staged.set_collaboration_run_control(input.clone())?;
                staged.set_collaboration_run_control(SetRunControlInput {
                    state: CollaborationRunControl::Stopped,
                    ..input.clone()
                })
            })
            .is_err()
    );
    assert_eq!(document, before);
    assert!(
        document
            .apply_atomic_transition(|staged| {
                staged.set_collaboration_run_control(input.clone())?;
                let _ = staged.prepare_flush_candidates();
                Ok(())
            })
            .is_err()
    );
    assert_eq!(document, before);
    document
        .apply_atomic_transition(|staged| {
            staged.set_collaboration_run_control(input)?;
            staged.apply_atomic_transition(|nested| {
                nested.set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("nested-control").unwrap(),
                    state: CollaborationRunControl::Stopped,
                    changed_at_ms: T0 + 2,
                })
            })
        })
        .unwrap();
    assert_eq!(document.memory_revision(), before.memory_revision() + 1);
    assert_eq!(
        document
            .memory_revision_receipt(document.memory_revision())
            .unwrap()
            .changed_event_seqs
            .len(),
        2
    );
    assert!(document.staged_commit_revision.is_none());
}

/// An existing failure reducer remains atomic when nested after another collaboration mutation.
/// 既有失败归约器嵌套在另一协作修改之后时仍保持原子性。
#[test]
fn nested_execution_failure_uses_the_reserved_publication_revision() {
    use crate::v1::event::{ApiPurposeKind, ApiRequestSnapshot};
    for write_control_first in [false, true] {
        // Each case owns a real open turn and physical API before the composite failure.
        // 每个场景在复合失败前均持有真实开放回合和物理 API。
        let mut document = SessionDocument::new("nested-failure", "project-1", "failure", T0);
        open_root_turn(&mut document, "nested-root", "work");
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 1,
            )
            .unwrap();
        // Capture the public revision before either nested transition runs.
        // 在任一嵌套转换运行前捕获公开修订。
        let revision = document.memory_revision();
        document
            .apply_atomic_transition(|staged| {
                if write_control_first {
                    staged.set_collaboration_run_control(SetRunControlInput {
                        operation_id: CollaborationOperationId::try_new("failure-control").unwrap(),
                        state: CollaborationRunControl::Stopped,
                        changed_at_ms: T0 + 2,
                    })?;
                }
                staged.fail_execution_boundary_and_close_turn(
                    "test_failure",
                    Some("Failed stream."),
                    T0 + 3,
                )
            })
            .unwrap();
        assert_eq!(document.memory_revision(), revision + 1);
        assert!(document.active_api().is_none());
        assert!(document.active_turn().unwrap().closed);
        assert!(document.staged_commit_revision.is_none());
        assert_eq!(
            document
                .memory_revision_receipt(revision + 1)
                .unwrap()
                .changed_event_seqs
                .len(),
            4 + usize::from(write_control_first)
        );
    }
}

/// Auxiliary completion preserves TTL state and immutable context bodies through reload.
/// 辅助完成在重载前后均保留 TTL 状态及不可变上下文正文。
#[test]
fn auxiliary_question_completion_does_not_consume_business_context_lifetime() {
    use crate::v1::dynamic_context::DynamicContextInput;
    use crate::v1::event::{SystemContextOperation, SystemContextScope, TurnBoundaryPurpose};
    use crate::v1::projection::derive_active_system_state;
    for outcome in [
        CollaborationQuestionExecutionOutcome::Unanswered,
        CollaborationQuestionExecutionOutcome::Failed,
    ] {
        // A real accepted question owns the auxiliary turn; it is not a synthetic user root.
        // 真实已接纳问题拥有辅助回合，而非合成用户根回合。
        let (_, mut target, source_identity, target_identity, question_id, delivery_id, _) =
            create_question_fixture("ttl-source", "ttl", "ttl");
        target
            .accept_delivery(acceptance_input(
                "accept-ttl",
                delivery_id,
                question_id.clone(),
                source_identity,
                target_identity.clone(),
            ))
            .unwrap();
        target
            .synchronize_dynamic_context(
                vec![
                    DynamicContextInput::new(
                        "ttl-one",
                        Some("One business turn.".into()),
                        SystemContextScope::Session,
                        Some(1),
                    ),
                    DynamicContextInput::new(
                        "ttl-two",
                        Some("Two business turns.".into()),
                        SystemContextScope::Session,
                        Some(2),
                    ),
                ],
                T0 + 3,
            )
            .unwrap();
        // Prefix and metadata must survive both an unanswered attempt and a failed attempt.
        // 历史前缀与 metadata 必须跨越无回答和失败两种尝试保持不变。
        let prefix = target.events().to_vec();
        // The producer map is the durable accelerator whose TTL generation must remain valid.
        // 生产者映射是可靠加速索引，其 TTL 代际必须继续有效。
        let dynamic_before = target.metadata.dynamic_context.clone();
        // Stable execution identity is shared by the start and completion operations.
        // 稳定执行身份由开始和完成操作共享。
        let execution_id = CollaborationExecutionId::try_new("ttl-execution").unwrap();
        target
            .begin_question_execution(BeginQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("begin-ttl").unwrap(),
                execution_id: execution_id.clone(),
                question_id,
                participant: target_identity,
                purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
                parent_turn_id: None,
                started_at_ms: T0 + 4,
            })
            .unwrap();
        target
            .complete_question_execution(CompleteQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("complete-ttl").unwrap(),
                execution_id,
                outcome,
                completed_at_ms: T0 + 5,
            })
            .unwrap();
        assert_eq!(&target.events()[..prefix.len()], prefix.as_slice());
        assert_eq!(target.metadata.dynamic_context, dynamic_before);
        assert!(target.events().iter().any(|event| matches!(&event.event, EventBody::TurnBoundary(boundary) if boundary.purpose == TurnBoundaryPurpose::QuestionExecution)));
        assert!(!target.events().iter().any(|event| matches!(&event.event, EventBody::SystemContext(context) if context.operation == SystemContextOperation::Remove)));
        // Replay from serialized durable state must preserve remaining counts, including one-turn TTL.
        // 从序列化可靠状态回放必须保留剩余次数，包括一次回合 TTL。
        let mut reloaded = SessionDocument::from_parts(
            serde_json::from_str(&serde_json::to_string(&target.metadata).unwrap()).unwrap(),
            target.runtime_lock.clone(),
            target.compactions.clone(),
            target.segment_catalog().to_vec(),
            target.events().to_vec(),
        )
        .unwrap();
        // A mismatched boundary purpose cannot be accepted when its opening fact is resident.
        // 当起始事实常驻时，不能接纳用途不匹配的闭合边界。
        let mut forged = reloaded.clone();
        for event in forged.events.iter_mut() {
            if let EventBody::TurnBoundary(boundary) = &mut event.event {
                boundary.purpose = TurnBoundaryPurpose::Business;
            }
        }
        assert!(forged.rebuild_derived_state().is_err());
        // Authoritative replay is checked before and after a subsequent real business closure.
        // 在后续真实业务闭合之前和之后检查权威回放。
        let states =
            derive_active_system_state(reloaded.events(), reloaded.events().last().unwrap().seq)
                .unwrap();
        assert_eq!(
            states
                .iter()
                .map(|state| state.remaining_turns)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(2)]
        );
        open_root_turn(&mut reloaded, "after-qa", "Business work.");
        reloaded
            .close_turn(TurnOutcome::Completed, 1, T0 + 7)
            .unwrap();
        // Only the business closure removes the one-turn generation and decrements the other.
        // 只有业务闭合才移除一次回合代际并递减另一代际。
        let states =
            derive_active_system_state(reloaded.events(), reloaded.events().last().unwrap().seq)
                .unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].context_key, "ttl-two");
        assert_eq!(states[0].remaining_turns, Some(1));
        assert!(!reloaded.metadata.dynamic_context.contains_key("ttl-one"));
    }
}

/// Projects an accepted idle question only after its real auxiliary turn exists, then reloads it.
/// 只在真实辅助回合存在后投影已接纳空闲问题，再验证重载。
#[test]
fn question_projection_is_atomic_idempotent_and_typed() {
    let (source, mut target, source_identity, target_identity, question_id, delivery_id, _) =
        create_question_fixture("source-projection", "projection", "projection");
    target
        .accept_delivery(acceptance_input(
            "accept-projection",
            delivery_id.clone(),
            question_id.clone(),
            source_identity,
            target_identity.clone(),
        ))
        .unwrap();
    let question = source.question_snapshot(&question_id).unwrap().clone();
    let before = target.clone();
    assert!(
        target
            .project_question_before_api(question.clone(), T0 + 3)
            .is_err()
    );
    assert_eq!(
        target, before,
        "a missing execution owner must not append any input or mutate IDs"
    );
    target
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin-projection").unwrap(),
            execution_id: CollaborationExecutionId::try_new("execution-projection").unwrap(),
            question_id,
            participant: target_identity,
            purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            parent_turn_id: None,
            started_at_ms: T0 + 4,
        })
        .unwrap();
    // Projection and its embedded target revision must share the actual publication boundary.
    // 投影及内嵌目标修订必须共享实际发布边界。
    let before_projection_revision = target.memory_revision();
    let receipt = target
        .project_question_before_api(question.clone(), T0 + 5)
        .unwrap()
        .into_value();
    assert_eq!(target.memory_revision(), before_projection_revision + 1);
    assert_eq!(receipt.revision, target.memory_revision());
    assert_eq!(
        target
            .memory_revision_receipt(target.memory_revision())
            .unwrap()
            .changed_event_seqs
            .len(),
        2
    );
    let projected = target.collaboration_state().inbound_deliveries[&delivery_id]
        .projected_seq
        .unwrap();
    assert!(target.events().iter().any(|event| event.seq == projected && matches!(&event.event,
        EventBody::UserInput(input) if input.input_source == crate::v1::event::UserInputSource::AgentMessage
            && input.text.contains("kind=\"question\"") && input.text.contains("source-projection")
    )));
    let after = target.clone();
    let retry = target
        .project_question_before_api(question, T0 + 6)
        .unwrap();
    assert!(matches!(retry, IdempotentMutation::Idempotent(_)));
    assert_eq!(retry.into_value(), receipt);
    assert_eq!(target, after);
    let serialized = serde_json::to_string(&target.metadata).unwrap();
    let metadata = serde_json::from_str(&serialized).unwrap();
    let reloaded = SessionDocument::from_parts(
        metadata,
        target.runtime_lock.clone(),
        target.compactions.clone(),
        target.segment_catalog().to_vec(),
        target.events().to_vec(),
    )
    .unwrap();
    assert_eq!(reloaded.collaboration_state(), target.collaboration_state());
}

/// Appends a notification once without altering the existing prefix or creating a question attempt.
/// 恰好一次追加通知，不改变既有前缀，也不创建答疑尝试。
#[test]
fn notification_projection_preserves_prefix_and_does_not_start_question_execution() {
    let mut source = SessionDocument::new("notify-source", "project-1", "source", T0);
    let source_turn = open_root_turn(&mut source, "notify-source-root", "work");
    let source_identity = source_participant(&source, source_turn);
    let mut target = SessionDocument::new("notify-target", "project-1", "target", T0);
    let target_turn = open_root_turn(&mut target, "notify-target-root", "target work");
    let target_identity = source_participant(&target, target_turn);
    let notification_id = CollaborationNotificationId::try_new("stage-one").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("stage-one-delivery").unwrap();
    target
        .accept_notification(AcceptNotificationInput {
            operation_id: CollaborationOperationId::try_new("stage-one-accept").unwrap(),
            notification_id: notification_id.clone(),
            delivery_id,
            source_operation_id: CollaborationOperationId::try_new("stage-one-send").unwrap(),
            source: source_identity,
            target: target_identity,
            body: String::from("Stage one complete."),
            accepted_at_ms: T0 + 1,
        })
        .unwrap();
    let prefix = target.events().to_vec();
    // One publication must satisfy the canonical SessionHandle single-revision contract.
    // 一次发布必须满足规范 SessionHandle 的单修订契约。
    let before_revision = target.memory_revision();
    target
        .project_notification_before_api(notification_id.clone(), T0 + 2)
        .unwrap();
    assert_eq!(target.memory_revision(), before_revision + 1);
    assert_eq!(
        target
            .memory_revision_receipt(target.memory_revision())
            .unwrap()
            .changed_event_seqs
            .len(),
        2
    );
    assert_eq!(&target.events()[..prefix.len()], prefix.as_slice());
    assert!(target.collaboration_state().question_executions.is_empty());
    assert_eq!(target.active_turn().unwrap().turn_id, target_turn);
    let after = target.clone();
    target
        .project_notification_before_api(notification_id, T0 + 3)
        .unwrap();
    assert_eq!(target, after);
}

/// Delivers an answer after the source starts a later turn while retaining its original question owner.
/// 来源启动后续回合后交付答案，同时保留问题的原始归属。
#[test]
fn late_answer_projects_in_new_turn_without_reopening_original_task() {
    let (mut source, _, source_identity, target_identity, question_id, _, _) =
        create_question_fixture("late-answer-source", "late-answer", "late-answer");
    let original_turn = source_identity.turn_id.unwrap();
    source
        .close_turn(TurnOutcome::Completed, 1, T0 + 3)
        .unwrap();
    source
        .reply_question(ReplyQuestionInput {
            operation_id: CollaborationOperationId::try_new("late-answer-reply").unwrap(),
            question_id: question_id.clone(),
            responder: target_identity,
            body: String::from("Later answer."),
            answered_at_ms: T0 + 4,
        })
        .unwrap();
    let closed = source.clone();
    assert!(
        source
            .project_answer_before_api(question_id.clone(), T0 + 5)
            .is_err()
    );
    assert_eq!(
        source, closed,
        "an answer must not auto-open a business turn"
    );
    let current_turn = open_root_turn(&mut source, "later-task", "new work");
    let prefix = source.events().to_vec();
    source
        .project_answer_before_api(question_id.clone(), T0 + 6)
        .unwrap();
    assert_ne!(current_turn, original_turn);
    assert_eq!(&source.events()[..prefix.len()], prefix.as_slice());
    let question = source.question_snapshot(&question_id).unwrap();
    assert_eq!(question.source.turn_id, Some(original_turn));
    let answer = question.answer.as_ref().unwrap();
    let projected_seq = answer.delivery.projected_seq.unwrap();
    // A late answer quotes its request before the answer without altering either durable body.
    // 迟到答案先引用原问题，再追加答案，不改写任何持久正文。
    let reference = source.events().iter().find(|event| {
        matches!(&event.event, EventBody::UserInput(input) if input.text.contains("kind=\"result_context\""))
    }).expect("late answer must include its original request reference");
    assert!(reference.seq < projected_seq);
    assert_eq!(reference.turn_id, Some(current_turn));
    let EventBody::UserInput(reference_input) = &reference.event else {
        unreachable!()
    };
    assert!(reference_input.text.contains(&question.body));
    assert!(reference_input.text.contains("\"late\":true"));
    assert_eq!(
        source
            .events()
            .iter()
            .find(|event| event.seq == projected_seq)
            .unwrap()
            .turn_id,
        Some(current_turn)
    );
    let after = source.clone();
    source
        .project_answer_before_api(question_id, T0 + 7)
        .unwrap();
    assert_eq!(source, after);
}

/// Late references truncate UTF-8 request text independently while preserving maximum-sized results.
/// 迟到引用独立截取 UTF-8 请求正文，同时保留达到上限的完整结果。
#[test]
fn late_reference_preserves_utf8_and_full_result_budget() {
    use vulcan_ai_contract::{
        HOST_MESSAGE_MAX_BODY_BYTES, HOST_MESSAGE_REFERENCE_MAX_BYTES, HostMessageEnvelope,
        HostMessageKind,
    };
    let (mut source, _, identity, _, _, _, _) = create_question_fixture(
        "reference-source",
        "reference-question",
        "reference-delivery",
    );
    let original_turn = identity.turn_id.unwrap();
    let result = HostMessageEnvelope {
        event_id: "bounded-reference-result".into(),
        kind: HostMessageKind::TaskResult,
        source: "responder".into(),
        target: identity.session_id,
        task_id: None,
        question_id: None,
        scope: "task".into(),
        body: "x".repeat(HOST_MESSAGE_MAX_BODY_BYTES),
    };
    let original = "界".repeat(HOST_MESSAGE_REFERENCE_MAX_BYTES);
    let before = source.clone();
    projection::append_late_result_context(&mut source, &result, original_turn, &original, T0 + 2)
        .unwrap();
    assert_eq!(
        source, before,
        "same-turn results must not duplicate their request"
    );
    source.close_turn(TurnOutcome::Stopped, 1, T0 + 3).unwrap();
    open_root_turn(&mut source, "reference-next-turn", "Next task");
    projection::append_late_result_context(&mut source, &result, original_turn, &original, T0 + 4)
        .unwrap();
    let reference = source
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::UserInput(input) if input.text.contains("kind=\"result_context\"") => {
                Some(&input.text)
            }
            _ => None,
        })
        .expect("late request reference is required");
    assert!(reference.contains("\"original_message_truncated\":true"));
    assert!(reference.contains(&format!(
        "\"original_message\":\"{}\"",
        "界".repeat(HOST_MESSAGE_REFERENCE_MAX_BYTES / "界".len())
    )));
    let rendered = result.render().unwrap();
    let (seq, _, _) = projection::append_envelope(&mut source, result, T0 + 4).unwrap();
    assert!(source.events().iter().any(|event| event.seq == seq
        && matches!(&event.event, EventBody::UserInput(input) if input.text == rendered)));
}

/// Builds the exact persisted native wait output from the source-owned question snapshot.
/// 从来源拥有的问题快照构建精确的原生等待持久输出。
fn wait_answer_output(question: &CollaborationQuestionRecord) -> serde_json::Value {
    serde_json::json!({
        "status": "ok", "question_id": question.question_id,
        "question_state": question.state, "source": question.source,
        "target": question.target, "answer": question.answer,
    })
}

/// Finishes a real wait tool, consumes its exact answer atomically, and preserves replay after reload.
/// 完成真实等待工具，原子消费精确答案，并在重载后保持幂等回放。
#[test]
fn completed_wait_delivers_answer_once_with_its_persisted_tool_result() {
    use crate::v1::document::ToolCompletion;
    use crate::v1::event::{ApiPurposeKind, ApiRequestSnapshot, ToolPayload};
    let (mut source, _, _, target, question_id, _, _) =
        create_question_fixture("wait-source", "wait-answer", "wait-answer");
    source
        .reply_question(ReplyQuestionInput {
            operation_id: CollaborationOperationId::try_new("wait-reply").unwrap(),
            question_id: question_id.clone(),
            responder: target,
            body: String::from("Exact answer."),
            answered_at_ms: T0 + 2,
        })
        .unwrap();
    let snapshot = wait_answer_output(source.question_snapshot(&question_id).unwrap());
    let api = source
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 3,
        )
        .unwrap();
    let tool = source
        .start_tool(
            api,
            "wait_question",
            ToolPayload::try_new(serde_json::json!({"question_id": question_id})).unwrap(),
            None,
            T0 + 3,
        )
        .unwrap();
    let completion = ToolCompletion {
        output: Some(
            ToolPayload::try_new(serde_json::json!({
                "tool_call_id": "provider-wait", "tool_name": "wait_question",
                "model_output": snapshot.to_string(), "host_result": null, "is_error": false,
            }))
            .unwrap(),
        ),
        changeset: None,
        status: ToolCallStatus::Completed,
        finished_at_ms: T0 + 4,
        external_refs: Vec::new(),
        generated_resources: Vec::new(),
    };
    // Exhaust the delivery revision after the tool is prepared: completion must roll back both facts.
    // 在工具准备后耗尽交付修订号：完成失败必须同时回滚两个事实。
    let revision = source
        .metadata
        .collaboration
        .state
        .questions
        .get_mut(&question_id)
        .unwrap()
        .answer
        .as_mut()
        .unwrap()
        .delivery
        .delivery_revision;
    source
        .metadata
        .collaboration
        .state
        .questions
        .get_mut(&question_id)
        .unwrap()
        .answer
        .as_mut()
        .unwrap()
        .delivery
        .delivery_revision = u64::MAX;
    let before_failure = source.clone();
    assert!(source.finish_tool(api, tool, completion.clone()).is_err());
    assert_eq!(
        source, before_failure,
        "no terminal tool may survive a failed atomic answer receipt"
    );
    source
        .metadata
        .collaboration
        .state
        .questions
        .get_mut(&question_id)
        .unwrap()
        .answer
        .as_mut()
        .unwrap()
        .delivery
        .delivery_revision = revision;
    // Tool completion and answer delivery form one durable mutation, not two revisions.
    // 工具完成与答案交付构成一次可靠修改，而非两个修订。
    let before_completion_revision = source.memory_revision();
    let receipt = source
        .finish_tool(api, tool, completion.clone())
        .unwrap()
        .into_value();
    assert_eq!(source.memory_revision(), before_completion_revision + 1);
    assert_eq!(
        source
            .memory_revision_receipt(source.memory_revision())
            .unwrap()
            .changed_event_seqs
            .len(),
        2
    );
    let delivery = &source
        .question_snapshot(&question_id)
        .unwrap()
        .answer
        .as_ref()
        .unwrap()
        .delivery;
    assert_eq!(delivery.state, CollaborationAnswerDeliveryState::Delivered);
    assert_eq!(
        delivery.kind,
        Some(CollaborationAnswerDeliveryKind::WaitTool)
    );
    assert_eq!(delivery.tool_result_seq, Some(receipt.tool_seq));
    assert_eq!(delivery.tool_call_id, Some(tool));
    let mut reloaded = SessionDocument::from_parts(
        source.metadata.clone(),
        source.runtime_lock.clone(),
        source.compactions.clone(),
        source.segment_catalog().to_vec(),
        source.events().to_vec(),
    )
    .unwrap();
    let before_retry = reloaded.clone();
    assert!(matches!(
        reloaded.finish_tool(api, tool, completion).unwrap(),
        IdempotentMutation::Idempotent(_)
    ));
    assert_eq!(reloaded, before_retry);
}

/// Rejects unrelated tools, forged answers, mismatched questions and unsuccessful wait outputs.
/// 拒绝无关工具、伪造答案、问题错配及不成功的等待输出。
#[test]
fn wait_answer_evidence_requires_exact_successful_question_and_answer() {
    use crate::v1::document::ToolCompletion;
    use crate::v1::event::{ApiPurposeKind, ApiRequestSnapshot, ToolPayload};
    for scenario in [
        "wrong_tool",
        "wrong_question",
        "wrong_answer",
        "failed",
        "incoming",
        "pending_snapshot",
    ] {
        let (mut source, _, _, target, question_id, _, _) =
            create_question_fixture("wait-proof-source", "wait-proof", "wait-proof");
        source
            .reply_question(ReplyQuestionInput {
                operation_id: CollaborationOperationId::try_new("proof-reply").unwrap(),
                question_id: question_id.clone(),
                responder: target,
                body: String::from("Authorized answer."),
                answered_at_ms: T0 + 2,
            })
            .unwrap();
        let mut snapshot = wait_answer_output(source.question_snapshot(&question_id).unwrap());
        let tool_name = if scenario == "wrong_tool" {
            "shell"
        } else {
            "wait_question"
        };
        let argument_question = if scenario == "wrong_question" {
            "unrelated"
        } else {
            question_id.as_str()
        };
        if scenario == "wrong_answer" {
            snapshot["answer"]["body"] = serde_json::json!("Forged answer.");
        }
        if scenario == "incoming" {
            snapshot["status"] = serde_json::json!("incoming");
        }
        if scenario == "pending_snapshot" {
            snapshot["question_state"] = serde_json::json!("pending");
            snapshot["answer"] = serde_json::Value::Null;
        }
        let api = source
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 3,
            )
            .unwrap();
        let tool = source
            .start_tool(
                api,
                tool_name,
                ToolPayload::try_new(serde_json::json!({"question_id": argument_question}))
                    .unwrap(),
                None,
                T0 + 3,
            )
            .unwrap();
        let receipt = source.finish_tool(api, tool, ToolCompletion {
            output: Some(ToolPayload::try_new(serde_json::json!({
                "tool_name": tool_name, "model_output": snapshot.to_string(), "is_error": scenario == "failed",
            })).unwrap()), changeset: None,
            status: if scenario == "failed" { ToolCallStatus::Failed } else { ToolCallStatus::Completed },
            finished_at_ms: T0 + 4, external_refs: Vec::new(), generated_resources: Vec::new(),
        }).unwrap().into_value();
        let before = source.clone();
        assert!(
            source
                .deliver_answer(DeliverAnswerInput {
                    operation_id: CollaborationOperationId::try_new("forged-wait-consumption")
                        .unwrap(),
                    question_id: question_id.clone(),
                    delivery_kind: CollaborationAnswerDeliveryKind::WaitTool,
                    projected_seq: None,
                    projected_input_id: None,
                    tool_call_id: Some(tool),
                    tool_result_seq: Some(receipt.tool_seq),
                    delivered_at_ms: T0 + 5,
                })
                .is_err(),
            "{scenario}"
        );
        assert_eq!(source, before, "{scenario}");
        assert_eq!(
            source
                .question_snapshot(&question_id)
                .unwrap()
                .answer
                .as_ref()
                .unwrap()
                .delivery
                .state,
            CollaborationAnswerDeliveryState::Undelivered,
            "{scenario}"
        );
    }
}

/// Ends a business turn with an unanswered in-turn attempt without cancelling the source question.
/// 在回合内答疑未回答时结束业务回合，不撤销来源问题。
#[test]
fn business_completion_closes_question_attempt_without_requiring_answer() {
    let mut source = SessionDocument::new("attempt-source", "project-1", "source", T0);
    let source_turn = open_root_turn(&mut source, "attempt-source-root", "source work");
    let mut target = SessionDocument::new("attempt-target", "project-1", "target", T0);
    let target_turn = open_root_turn(&mut target, "attempt-target-root", "target work");
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("attempt-running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: T0,
        })
        .unwrap();
    let question_id = CollaborationQuestionId::try_new("attempt-question").unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("attempt-delivery").unwrap();
    let source_identity = source_participant(&source, source_turn);
    let target_identity = source_participant(&target, target_turn);
    source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("attempt-create").unwrap(),
            question_id: question_id.clone(),
            delivery_id: delivery_id.clone(),
            source: source_identity.clone(),
            target: target_identity.clone(),
            body: String::from("Optional question."),
            execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
            created_at_ms: T0 + 1,
        })
        .unwrap();
    target
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("attempt-accept").unwrap(),
            question_id: question_id.clone(),
            delivery_id: delivery_id.clone(),
            source: source_identity,
            target: target_identity,
            bound_turn_id: Some(target_turn),
            execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
            accepted_at_ms: T0 + 2,
        })
        .unwrap();
    // In-turn projection stages three facts: execution start, envelope and delivery receipt.
    // 回合内投影暂存三个事实：执行启动、信封及交付回执。
    let before_projection_revision = target.memory_revision();
    target
        .project_question_before_api(
            source.question_snapshot(&question_id).unwrap().clone(),
            T0 + 3,
        )
        .unwrap();
    assert_eq!(target.memory_revision(), before_projection_revision + 1);
    assert_eq!(
        target
            .memory_revision_receipt(target.memory_revision())
            .unwrap()
            .changed_event_seqs
            .len(),
        3
    );
    // Business closure includes attempt completion without exposing an intermediate revision.
    // 业务闭合包含答疑尝试完成，不暴露中间修订。
    let before_closure_revision = target.memory_revision();
    target
        .close_turn(TurnOutcome::Completed, 1, T0 + 4)
        .unwrap();
    assert_eq!(target.memory_revision(), before_closure_revision + 1);
    assert_eq!(
        target
            .memory_revision_receipt(target.memory_revision())
            .unwrap()
            .changed_event_seqs
            .len(),
        3
    );
    assert!(target.active_turn().unwrap().closed);
    let execution = target
        .collaboration_state()
        .question_executions
        .values()
        .next()
        .unwrap();
    assert_eq!(
        execution.state,
        CollaborationQuestionExecutionState::Completed
    );
    assert_eq!(
        execution.outcome,
        Some(CollaborationQuestionExecutionOutcome::Unanswered)
    );
    assert!(
        target
            .inbound_delivery_snapshot(&delivery_id)
            .unwrap()
            .settlement
            .is_none()
    );
    assert_eq!(
        source.question_snapshot(&question_id).unwrap().state,
        CollaborationQuestionState::Pending
    );
    let reloaded = SessionDocument::from_parts(
        target.metadata.clone(),
        target.runtime_lock.clone(),
        target.compactions.clone(),
        target.segment_catalog().to_vec(),
        target.events().to_vec(),
    )
    .unwrap();
    assert_eq!(target.collaboration_state(), reloaded.collaboration_state());
}

/// Builds one valid source participant for the current document generation.
/// 为当前文档代次构建一名有效来源参与者。
fn source_participant(document: &SessionDocument, turn_id: TurnId) -> CollaborationParticipant {
    CollaborationParticipant::new(
        document.metadata.session_id.clone(),
        document.metadata.session_id.clone(),
        document.metadata.project_tag.clone(),
        document.metadata.segment_generation,
        Some(turn_id),
        None,
    )
    .expect("source fixture participant must be valid")
}

/// Builds one valid naturally idle target participant for the current document generation.
/// 为当前文档代次构建一名有效的自然空闲目标参与者。
fn target_participant(document: &SessionDocument) -> CollaborationParticipant {
    CollaborationParticipant::new(
        document.metadata.session_id.clone(),
        document.metadata.session_id.clone(),
        document.metadata.project_tag.clone(),
        document.metadata.segment_generation,
        None,
        None,
    )
    .expect("target fixture participant must be valid")
}

/// Opens one real root turn through the public SessionDocument admission path.
/// 通过公开的 SessionDocument 准入路径打开一个真实根回合。
fn open_root_turn(document: &mut SessionDocument, action: &str, text: &str) -> TurnId {
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new(action).expect("fixture action id must be valid"),
                text,
                Vec::new(),
            ),
            T0,
        )
        .expect("root input admission must succeed")
        .into_value()
}

/// Finds the real provider-visible UserInput fact for one opened turn.
/// 查找某个已打开回合中真实的 Provider 可见 UserInput 事实。
fn user_input_fact(document: &SessionDocument, turn_id: TurnId) -> (Seq, PendingInputId) {
    document
        .events()
        .iter()
        .find_map(|event| match (&event.event, event.turn_id) {
            (EventBody::UserInput(input), Some(event_turn_id)) if event_turn_id == turn_id => {
                Some((event.seq, input.input_id))
            }
            _ => None,
        })
        .expect("the opened turn must have one real UserInput fact")
}

/// Creates one source question and its target identity on real documents.
/// 在真实文档上创建一个来源问题及其目标身份。
fn create_question_fixture(
    source_session_id: &str,
    question_suffix: &str,
    action_suffix: &str,
) -> (
    SessionDocument,
    SessionDocument,
    CollaborationParticipant,
    CollaborationParticipant,
    CollaborationQuestionId,
    CollaborationDeliveryId,
    Seq,
) {
    let mut source = SessionDocument::new(source_session_id, "project-1", "source", T0);
    let target = SessionDocument::new("target-session", "project-1", "target", T0);
    let source_turn_id = open_root_turn(
        &mut source,
        &format!("source-action-{action_suffix}"),
        "source work",
    );
    let source_participant = source_participant(&source, source_turn_id);
    let target_participant = target_participant(&target);
    let question_id = CollaborationQuestionId::try_new(format!("question-{question_suffix}"))
        .expect("fixture question id must be valid");
    let delivery_id = CollaborationDeliveryId::try_new(format!("delivery-{question_suffix}"))
        .expect("fixture delivery id must be valid");
    let create_operation = CollaborationOperationId::try_new(format!("create-{question_suffix}"))
        .expect("fixture operation id must be valid");
    let receipt = source
        .create_question(CreateQuestionInput {
            operation_id: create_operation,
            question_id: question_id.clone(),
            delivery_id: delivery_id.clone(),
            source: source_participant.clone(),
            target: target_participant.clone(),
            body: "temporary question".to_string(),
            execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            created_at_ms: T0 + 1,
        })
        .expect("source question creation must succeed")
        .into_value();
    assert_eq!(
        source
            .question_snapshot(&question_id)
            .expect("created question must be resident")
            .state,
        CollaborationQuestionState::Pending
    );
    (
        source,
        target,
        source_participant,
        target_participant,
        question_id,
        delivery_id,
        receipt.event_seq,
    )
}

/// Builds one target acceptance input bound to the exact source question locator.
/// 构建绑定精确来源问题定位的目标接纳输入。
fn acceptance_input(
    operation: &str,
    delivery_id: CollaborationDeliveryId,
    question_id: CollaborationQuestionId,
    source: CollaborationParticipant,
    target: CollaborationParticipant,
) -> AcceptDeliveryInput {
    AcceptDeliveryInput {
        operation_id: CollaborationOperationId::try_new(operation)
            .expect("fixture operation id must be valid"),
        delivery_id,
        question_id,
        source,
        target,
        bound_turn_id: None,
        execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
        accepted_at_ms: T0 + 2,
    }
}

/// A closed source turn cannot admit a new collaboration question.
/// 来源回合关闭后不能再准入新的协作问题。
#[test]
fn closed_source_turn_cannot_create_question() {
    let mut source = SessionDocument::new("closed-source", "project-1", "source", T0);
    let turn_id = open_root_turn(&mut source, "closed-source-action", "source work");
    source
        .close_turn(TurnOutcome::Completed, 1, T0 + 1)
        .expect("the fixture turn must close through the real domain method");
    assert_eq!(
        source.active_turn().map(|turn| (turn.turn_id, turn.closed)),
        Some((turn_id, true)),
        "precondition: the source turn is durably closed"
    );

    let target = SessionDocument::new("closed-target", "project-1", "target", T0);
    let source_participant = source_participant(&source, turn_id);
    let target_participant = target_participant(&target);
    let question_id = CollaborationQuestionId::try_new("closed-question").unwrap();
    let result = source.create_question(CreateQuestionInput {
        operation_id: CollaborationOperationId::try_new("closed-create").unwrap(),
        question_id: question_id.clone(),
        delivery_id: CollaborationDeliveryId::try_new("closed-delivery").unwrap(),
        source: source_participant,
        target: target_participant,
        body: "must be rejected".to_string(),
        execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
        created_at_ms: T0 + 2,
    });

    assert!(
        result.is_err(),
        "closed source turns must not create questions"
    );
    assert!(
        source.question_snapshot(&question_id).is_none(),
        "rejected creation must leave source authority unchanged"
    );
}

/// Retried execution operations return their original snapshots after completion, stop and reload.
/// 重试执行操作在完成、停止和重载后仍返回原始快照。
#[test]
fn question_execution_retries_preserve_original_receipts_after_stop_and_reload() {
    // Use one real accepted question so retry assertions exercise production admission rules.
    // 使用一个真实已接纳问题，使重试断言覆盖生产准入规则。
    let (_, mut target, source, participant, question_id, delivery_id, _) =
        create_question_fixture("retry-source", "retry", "retry");
    target
        .accept_delivery(acceptance_input(
            "retry-accept",
            delivery_id,
            question_id.clone(),
            source,
            participant.clone(),
        ))
        .unwrap();
    // The original start request is retried verbatim after its execution no longer runs.
    // 原始启动请求在执行不再运行后原样重试。
    let begin = BeginQuestionExecutionInput {
        operation_id: CollaborationOperationId::try_new("retry-begin").unwrap(),
        execution_id: CollaborationExecutionId::try_new("retry-execution").unwrap(),
        question_id,
        participant,
        purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
        parent_turn_id: None,
        started_at_ms: T0 + 3,
    };
    // Save the running-state receipt independently of the latest completed execution record.
    // 将运行态回执与最新的已完成执行记录分开保存。
    let original_start = target
        .begin_question_execution(begin.clone())
        .unwrap()
        .into_value();
    // Completion has its own identity and immutable result.
    // 完成操作拥有独立身份及不可变结果。
    let complete = CompleteQuestionExecutionInput {
        operation_id: CollaborationOperationId::try_new("retry-complete").unwrap(),
        execution_id: begin.execution_id.clone(),
        outcome: CollaborationQuestionExecutionOutcome::Unanswered,
        completed_at_ms: T0 + 4,
    };
    // Preserve the first completion result for both in-memory and reloaded retries.
    // 保留首次完成结果，用于内存与重载后的两种重试。
    let original_completion = target
        .complete_question_execution(complete.clone())
        .unwrap()
        .into_value();
    assert_eq!(
        target
            .begin_question_execution(begin.clone())
            .unwrap()
            .into_value(),
        original_start
    );
    target
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("retry-stop").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: T0 + 5,
        })
        .unwrap();
    // Serialize and reload the authority so no in-memory receipt cache can satisfy the assertion.
    // 序列化并重载权威，防止内存回执缓存满足断言。
    let mut reloaded = SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&target.metadata).unwrap()).unwrap(),
        target.runtime_lock.clone(),
        target.compactions.clone(),
        target.segment_catalog().to_vec(),
        target.events().to_vec(),
    )
    .unwrap();
    // Replays must not mutate stopped control, allocator, events or revision.
    // 重放不得改变停止控制、分配器、事件或修订。
    let before = reloaded.clone();
    // Verify the committed start, rather than treating the current completed state as its receipt.
    // 验证已提交启动，而不是把当前已完成状态当作其回执。
    let replay_start = reloaded.begin_question_execution(begin.clone()).unwrap();
    assert!(!replay_start.changed());
    assert_eq!(replay_start.into_value(), original_start);
    assert_eq!(
        reloaded
            .complete_question_execution(complete)
            .unwrap()
            .into_value(),
        original_completion
    );
    assert_eq!(reloaded, before);
    // A different operation is still a new admission and remains forbidden while stopped.
    // 不同操作仍是新的准入，在停止期间继续拒绝。
    assert!(
        reloaded
            .begin_question_execution(BeginQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("retry-new-operation").unwrap(),
                ..begin
            })
            .is_err()
    );
    assert_eq!(reloaded, before);
}

/// A question has at most one running target execution even when execution ids differ.
/// 即使执行身份不同，一个问题也最多只能有一个运行中的目标执行。
#[test]
fn one_question_cannot_start_two_executions() {
    let (source, mut target, source_participant, target_participant, question_id, delivery_id, _) =
        create_question_fixture("execution-source", "execution", "execution");
    let accepted = target
        .accept_delivery(acceptance_input(
            "accept-execution",
            delivery_id,
            question_id.clone(),
            source_participant,
            target_participant.clone(),
        ))
        .expect("target acceptance must succeed")
        .into_value();
    assert_eq!(accepted.state, CollaborationDeliveryState::Accepted);
    assert_eq!(target.collaboration_state().question_executions.len(), 0);

    let first = target
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin-execution-1").unwrap(),
            execution_id: CollaborationExecutionId::try_new("execution-1").unwrap(),
            question_id: question_id.clone(),
            participant: target_participant.clone(),
            purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            parent_turn_id: None,
            started_at_ms: T0 + 3,
        })
        .expect("the first QA execution must start")
        .into_value();
    assert_eq!(
        first.execution.state,
        CollaborationQuestionExecutionState::Running
    );
    assert_eq!(target.collaboration_state().question_executions.len(), 1);

    let second = target.begin_question_execution(BeginQuestionExecutionInput {
        operation_id: CollaborationOperationId::try_new("begin-execution-2").unwrap(),
        execution_id: CollaborationExecutionId::try_new("execution-2").unwrap(),
        question_id,
        participant: target_participant,
        purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
        parent_turn_id: None,
        started_at_ms: T0 + 4,
    });
    assert!(
        second.is_err(),
        "a second execution for one question must be rejected"
    );
    assert_eq!(target.collaboration_state().question_executions.len(), 1);
    assert!(
        source
            .collaboration_state()
            .questions
            .values()
            .all(|question| { question.state == CollaborationQuestionState::Pending })
    );
}

/// Forged, missing, and cross-question sequences cannot be projected or delivered.
/// 伪造、不存在或属于其他问题的 seq 不能被投影或交付。
#[test]
fn forged_and_cross_question_sequences_are_rejected() {
    let (
        mut source,
        mut target,
        initial_source_participant,
        target_participant,
        question_id,
        delivery_id,
        _,
    ) = create_question_fixture("sequence-source", "sequence-one", "sequence-one");
    let accepted_one = target
        .accept_delivery(acceptance_input(
            "accept-sequence-one",
            delivery_id.clone(),
            question_id.clone(),
            initial_source_participant,
            target_participant.clone(),
        ))
        .expect("first target acceptance must succeed")
        .into_value();

    source
        .close_turn(TurnOutcome::Completed, 1, T0 + 3)
        .expect("the first source turn must close before the second real input");

    let second_turn_id = open_root_turn(&mut source, "sequence-two-action", "second work");
    let (second_source_input_seq, second_source_input_id) =
        user_input_fact(&source, second_turn_id);
    let second_source_participant = source_participant(&source, second_turn_id);
    let question_two = CollaborationQuestionId::try_new("question-sequence-two").unwrap();
    let delivery_two = CollaborationDeliveryId::try_new("delivery-sequence-two").unwrap();
    source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("create-sequence-two").unwrap(),
            question_id: question_two.clone(),
            delivery_id: delivery_two.clone(),
            source: second_source_participant.clone(),
            target: target_participant.clone(),
            body: "second temporary question".to_string(),
            execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            created_at_ms: T0 + 5,
        })
        .expect("second source question must succeed");
    target
        .accept_delivery(acceptance_input(
            "accept-sequence-two",
            delivery_two,
            question_two,
            second_source_participant,
            target_participant.clone(),
        ))
        .expect("second target acceptance must succeed");

    let question_two = CollaborationQuestionId::try_new("question-sequence-two").unwrap();
    let delivery_two = CollaborationDeliveryId::try_new("delivery-sequence-two").unwrap();
    target
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin-sequence-two").unwrap(),
            execution_id: CollaborationExecutionId::try_new("execution-sequence-two").unwrap(),
            question_id: question_two.clone(),
            participant: target_participant.clone(),
            purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            parent_turn_id: None,
            started_at_ms: T0 + 6,
        })
        .unwrap();
    let second_projection = target
        .project_question_before_api(
            source.question_snapshot(&question_two).unwrap().clone(),
            T0 + 6,
        )
        .unwrap()
        .into_value();
    let projected = target.inbound_delivery_snapshot(&delivery_two).unwrap();
    let target_projection_turn = projected.projected_turn_id.unwrap();
    let target_projection_seq = projected.projected_seq.unwrap();
    let target_projection_input_id = projected.projected_input_id.unwrap();
    assert_eq!(
        second_projection.state,
        CollaborationDeliveryState::Projected
    );

    let before_projection = target
        .inbound_delivery_snapshot(&delivery_id)
        .cloned()
        .expect("first inbound delivery must be resident");
    let nonexistent_projection = target.mark_delivery_projected(ProjectDeliveryInput {
        operation_id: CollaborationOperationId::try_new("project-sequence-missing").unwrap(),
        delivery_id: delivery_id.clone(),
        question_id: question_id.clone(),
        projected_seq: Seq::try_new(JS_SAFE_INTEGER_MAX).unwrap(),
        projected_input_id: PendingInputId::try_new(JS_SAFE_INTEGER_MAX).unwrap(),
        projected_turn_id: TurnId::try_new(JS_SAFE_INTEGER_MAX).unwrap(),
        projected_at_ms: T0 + 7,
    });
    assert!(
        nonexistent_projection.is_err(),
        "a projection seq absent from the target stream must be rejected"
    );
    assert_eq!(
        target.inbound_delivery_snapshot(&delivery_id),
        Some(&before_projection),
        "a rejected projection must not mutate the target delivery"
    );

    let cross_question_projection = target.mark_delivery_projected(ProjectDeliveryInput {
        operation_id: CollaborationOperationId::try_new("project-sequence-cross-question").unwrap(),
        delivery_id: delivery_id.clone(),
        question_id,
        projected_seq: target_projection_seq,
        projected_input_id: target_projection_input_id,
        projected_turn_id: target_projection_turn,
        projected_at_ms: T0 + 8,
    });
    assert!(
        cross_question_projection.is_err(),
        "a sequence belonging to another question must be rejected"
    );
    assert_eq!(
        target.inbound_delivery_snapshot(&delivery_id),
        Some(&before_projection)
    );

    let answered = source
        .reply_question(ReplyQuestionInput {
            operation_id: CollaborationOperationId::try_new("answer-sequence-one").unwrap(),
            question_id: CollaborationQuestionId::try_new("question-sequence-one").unwrap(),
            responder: target_participant,
            body: "answer".to_string(),
            answered_at_ms: T0 + 8,
        })
        .expect("the source answer settlement must succeed")
        .into_value();
    assert_eq!(answered.state, CollaborationQuestionState::Answered);
    let answer_delivery_before = source
        .question_snapshot(&CollaborationQuestionId::try_new("question-sequence-one").unwrap())
        .and_then(|question| question.answer.as_ref())
        .map(|answer| answer.delivery.clone())
        .expect("answered question must carry delivery state");
    let forged_answer_delivery = source.deliver_answer(DeliverAnswerInput {
        operation_id: CollaborationOperationId::try_new("deliver-sequence-cross-question").unwrap(),
        question_id: CollaborationQuestionId::try_new("question-sequence-one").unwrap(),
        delivery_kind: CollaborationAnswerDeliveryKind::UserEvent,
        projected_seq: Some(second_source_input_seq),
        projected_input_id: Some(second_source_input_id),
        tool_call_id: None,
        tool_result_seq: None,
        delivered_at_ms: T0 + 9,
    });
    assert!(
        forged_answer_delivery.is_err(),
        "a source answer cannot cite a projection sequence from another question"
    );
    let answer_delivery_after = source
        .question_snapshot(&CollaborationQuestionId::try_new("question-sequence-one").unwrap())
        .and_then(|question| question.answer.as_ref())
        .map(|answer| answer.delivery.clone())
        .expect("answered question must retain delivery state after rejection");
    assert_eq!(answer_delivery_after, answer_delivery_before);
    assert_eq!(accepted_one.state, CollaborationDeliveryState::Accepted);
}

/// Non-empty collaboration metadata without an event coverage marker is invalid.
/// 非空协作权威 metadata 缺少事件覆盖标记时必须无效。
#[test]
fn nonempty_collaboration_without_event_marker_is_rejected() {
    let (mut source, _, _, _, question_id, _, _) =
        create_question_fixture("marker-source", "marker", "marker");
    assert!(
        !source.collaboration_state().questions.is_empty(),
        "precondition: a real mutation populated collaboration state"
    );
    assert!(
        source.collaboration_metadata().last_event_seq.is_some(),
        "precondition: the real mutation published an event coverage marker"
    );

    source.metadata.collaboration.last_event_seq = None;
    let result = source.validate_collaboration_state();
    assert!(
        result.is_err(),
        "non-empty authoritative collaboration state requires an event marker"
    );
    assert!(source.question_snapshot(&question_id).is_some());
}

/// Repeating one exact target acceptance returns the original revision and event receipt.
/// 重复完全相同的目标接纳必须返回原始修订号与事件回执。
#[test]
fn accept_retry_preserves_receipt_revision() {
    let (_, mut target, source_participant, target_participant, question_id, delivery_id, _) =
        create_question_fixture("accept-source", "accept", "accept");
    let input = acceptance_input(
        "accept-retry",
        delivery_id.clone(),
        question_id,
        source_participant,
        target_participant,
    );
    let first = target
        .accept_delivery(input.clone())
        .expect("first target acceptance must succeed");
    assert!(first.changed());
    let first_receipt = first.into_value();
    let first_revision = target
        .inbound_delivery_snapshot(&delivery_id)
        .expect("accepted delivery must be resident")
        .target_revision;
    let first_memory_revision = target.memory_revision();

    let second = target
        .accept_delivery(input)
        .expect("exact target acceptance retry must succeed idempotently");
    assert!(!second.changed());
    let second_receipt = second.into_value();
    assert_eq!(second_receipt, first_receipt);
    assert_eq!(
        target
            .inbound_delivery_snapshot(&delivery_id)
            .expect("accepted delivery must remain resident")
            .target_revision,
        first_revision,
        "idempotent retry must preserve target revision"
    );
    assert_eq!(target.memory_revision(), first_memory_revision);
}

/// The first answer wins, exact answer retry is idempotent, and cancellation loses.
/// 首个回答胜出，完全相同的回答重试幂等，撤销随后失败。
#[test]
fn answer_wins_over_cancel_and_retry_is_idempotent() {
    let (mut source, _, source_participant, target_participant, question_id, _, _) =
        create_question_fixture("answer-source", "answer", "answer");
    let answer_input = ReplyQuestionInput {
        operation_id: CollaborationOperationId::try_new("answer-winner").unwrap(),
        question_id: question_id.clone(),
        responder: target_participant,
        body: "answer wins".to_string(),
        answered_at_ms: T0 + 3,
    };
    let first = source
        .reply_question(answer_input.clone())
        .expect("the first answer must settle the question");
    assert!(first.changed());
    let first_receipt = first.into_value();
    assert_eq!(first_receipt.state, CollaborationQuestionState::Answered);
    assert_eq!(
        source
            .question_snapshot(&question_id)
            .expect("answered question must remain resident")
            .state,
        CollaborationQuestionState::Answered
    );

    let retry = source
        .reply_question(answer_input)
        .expect("an exact answer retry must be idempotent");
    assert!(!retry.changed());
    assert_eq!(retry.into_value(), first_receipt);

    let cancel = source.cancel_question(CancelQuestionInput {
        operation_id: CollaborationOperationId::try_new("answer-losing-cancel").unwrap(),
        question_id: question_id.clone(),
        requester: source_participant,
        reason: "too late".to_string(),
        cancelled_at_ms: T0 + 4,
    });
    assert!(
        cancel.is_err(),
        "cancellation cannot replace a winning answer"
    );
    assert_eq!(
        source
            .question_snapshot(&question_id)
            .expect("question must remain resident")
            .state,
        CollaborationQuestionState::Answered
    );
}

/// The first cancellation wins, exact cancellation retry is idempotent, and an answer loses.
/// 首个撤销胜出，完全相同的撤销重试幂等，回答随后失败。
#[test]
fn cancel_wins_over_answer_and_retry_is_idempotent() {
    let (mut source, _, _initial_source_participant, target_participant, first_question_id, _, _) =
        create_question_fixture("cancel-source", "cancel-first", "cancel-first");
    source
        .close_turn(TurnOutcome::Completed, 1, T0 + 3)
        .expect("the first source turn must close before the next real input");
    let next_turn_id = open_root_turn(&mut source, "cancel-second-action", "second work");
    let next_source_participant = source_participant(&source, next_turn_id);
    let second_question_id = CollaborationQuestionId::try_new("question-cancel-second").unwrap();
    source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("create-cancel-second").unwrap(),
            question_id: second_question_id.clone(),
            delivery_id: CollaborationDeliveryId::try_new("delivery-cancel-second").unwrap(),
            source: next_source_participant.clone(),
            target: target_participant.clone(),
            body: "second question".to_string(),
            execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            created_at_ms: T0 + 4,
        })
        .expect("the second source question must succeed");
    assert_eq!(
        source
            .question_snapshot(&first_question_id)
            .expect("first question must remain resident")
            .state,
        CollaborationQuestionState::Pending
    );

    let cancel_input = CancelQuestionInput {
        operation_id: CollaborationOperationId::try_new("cancel-winner").unwrap(),
        question_id: second_question_id.clone(),
        requester: next_source_participant,
        reason: "source stopped waiting".to_string(),
        cancelled_at_ms: T0 + 5,
    };
    let first = source
        .cancel_question(cancel_input.clone())
        .expect("the first cancellation must settle the question");
    assert!(first.changed());
    let first_receipt = first.into_value();
    assert_eq!(first_receipt.state, CollaborationQuestionState::Cancelled);

    let retry = source
        .cancel_question(cancel_input)
        .expect("an exact cancellation retry must be idempotent");
    assert!(!retry.changed());
    assert_eq!(retry.into_value(), first_receipt);

    let answer = source.reply_question(ReplyQuestionInput {
        operation_id: CollaborationOperationId::try_new("cancel-losing-answer").unwrap(),
        question_id: second_question_id.clone(),
        responder: target_participant,
        body: "too late".to_string(),
        answered_at_ms: T0 + 6,
    });
    assert!(
        answer.is_err(),
        "an answer cannot replace a winning cancellation"
    );
    assert_eq!(
        source
            .question_snapshot(&second_question_id)
            .expect("cancelled question must remain resident")
            .state,
        CollaborationQuestionState::Cancelled
    );
}

/// Pending source and target collaboration does not block ordinary turn closure; waiting is explicit.
/// 待处理的来源和目标协作不会阻止普通回合闭合；等待必须显式执行。
#[test]
fn pending_collaboration_does_not_block_turn_closure() {
    let (
        mut source,
        mut target,
        source_participant,
        target_participant,
        question_id,
        delivery_id,
        _,
    ) = create_question_fixture("pending-closure", "pending-closure", "pending-closure");
    let source_turn_id = source_participant
        .turn_id
        .expect("source question must retain its source turn");

    target
        .accept_delivery(acceptance_input(
            "accept-pending-closure",
            delivery_id,
            question_id,
            source_participant,
            target_participant,
        ))
        .expect("target acceptance must succeed");

    assert!(
        source.has_unsettled_collaboration_for_edit_or_delete(),
        "the source delete guard must retain the pending question"
    );
    assert!(
        target.has_unsettled_collaboration_for_edit_or_delete(),
        "the target delete guard must retain the accepted delivery"
    );
    assert!(
        !source.has_unsettled_collaboration(Some(source_turn_id)),
        "turn completion must only inspect explicit completion-blocking child tasks"
    );
    source
        .close_turn(TurnOutcome::Completed, 1, T0 + 3)
        .expect("a pending source question must not block ordinary source closure");
    assert_eq!(
        source.active_turn().map(|turn| (turn.turn_id, turn.closed)),
        Some((source_turn_id, true))
    );

    let target_turn_id = open_root_turn(&mut target, "pending-target-action", "target work");
    assert!(
        !target.has_unsettled_collaboration(Some(target_turn_id)),
        "a naturally idle question is not an implicit target-turn completion blocker"
    );
    target
        .close_turn(TurnOutcome::Completed, 1, T0 + 4)
        .expect("an accepted target delivery must not block ordinary target closure");
    assert_eq!(
        target.active_turn().map(|turn| (turn.turn_id, turn.closed)),
        Some((target_turn_id, true))
    );
    assert!(
        source.has_unsettled_collaboration_for_edit_or_delete()
            && target.has_unsettled_collaboration_for_edit_or_delete(),
        "turn closure must not clear the independent edit/delete preservation guard"
    );
}

/// Existing questions remain replyable or cancellable after source closure without reopening that turn.
/// 来源回合闭合后，已有问题仍可回答或撤销，但不会重新打开原回合。
#[test]
fn closed_source_turn_keeps_existing_questions_mutable_without_reopening() {
    let (mut source, _, source_participant, target_participant, first_question_id, _, _) =
        create_question_fixture("closed-existing", "closed-existing-one", "closed-existing");
    let source_turn_id = source_participant
        .turn_id
        .expect("source question must retain its source turn");
    let second_question_id = CollaborationQuestionId::try_new("closed-existing-two").unwrap();
    source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("create-closed-existing-two").unwrap(),
            question_id: second_question_id.clone(),
            delivery_id: CollaborationDeliveryId::try_new("delivery-closed-existing-two").unwrap(),
            source: source_participant.clone(),
            target: target_participant.clone(),
            body: "second existing question".to_string(),
            execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            created_at_ms: T0 + 2,
        })
        .expect("the second question must be admitted while the source turn is open");

    source
        .close_turn(TurnOutcome::Completed, 1, T0 + 3)
        .expect("the source turn must close before post-closure settlement");

    let answer = source
        .reply_question(ReplyQuestionInput {
            operation_id: CollaborationOperationId::try_new("answer-after-close").unwrap(),
            question_id: first_question_id.clone(),
            responder: target_participant,
            body: "answer after source closure".to_string(),
            answered_at_ms: T0 + 4,
        })
        .expect("an existing question must remain answerable after source closure")
        .into_value();
    assert_eq!(answer.state, CollaborationQuestionState::Answered);

    let cancel = source
        .cancel_question(CancelQuestionInput {
            operation_id: CollaborationOperationId::try_new("cancel-after-close").unwrap(),
            question_id: second_question_id.clone(),
            requester: source_participant,
            reason: "cancel after source closure".to_string(),
            cancelled_at_ms: T0 + 5,
        })
        .expect("an existing question must remain cancellable after source closure")
        .into_value();
    assert_eq!(cancel.state, CollaborationQuestionState::Cancelled);
    assert_eq!(
        source.active_turn().map(|turn| (turn.turn_id, turn.closed)),
        Some((source_turn_id, true)),
        "post-closure question settlement must not reopen or replace the source turn"
    );
    assert_eq!(
        source
            .question_snapshot(&first_question_id)
            .expect("answered question must remain queryable")
            .state,
        CollaborationQuestionState::Answered
    );
    assert_eq!(
        source
            .question_snapshot(&second_question_id)
            .expect("cancelled question must remain queryable")
            .state,
        CollaborationQuestionState::Cancelled
    );
}

/// Local guards follow each document's terminal facts; the source remains guarded until acknowledgement.
/// 本地保护依据各自文档终态；来源在收到确认之前继续受保护。
#[test]
fn source_and_target_settlement_clear_delete_guard_only_after_both_receipts() {
    let (
        mut source,
        mut target,
        source_participant,
        target_participant,
        question_id,
        delivery_id,
        _,
    ) = create_question_fixture(
        "settlement-delete",
        "settlement-delete",
        "settlement-delete",
    );
    target
        .accept_delivery(acceptance_input(
            "accept-settlement-delete",
            delivery_id.clone(),
            question_id.clone(),
            source_participant.clone(),
            target_participant,
        ))
        .expect("target acceptance must succeed");
    assert!(source.has_unsettled_collaboration_for_edit_or_delete());
    assert!(target.has_unsettled_collaboration_for_edit_or_delete());

    let cancelled = source
        .cancel_question(CancelQuestionInput {
            operation_id: CollaborationOperationId::try_new("cancel-settlement-delete").unwrap(),
            question_id: question_id.clone(),
            requester: source_participant,
            reason: "settle cancellation".to_string(),
            cancelled_at_ms: T0 + 3,
        })
        .expect("source cancellation must settle the question")
        .into_value();
    assert_eq!(cancelled.state, CollaborationQuestionState::Cancelled);
    assert!(
        source.has_unsettled_collaboration_for_edit_or_delete(),
        "source must retain the delete guard until target settlement acknowledgement"
    );

    let target_settlement_input = SettleInboundDeliveryInput {
        operation_id: CollaborationOperationId::try_new("target-settle-cancellation").unwrap(),
        delivery_id: delivery_id.clone(),
        question_id: question_id.clone(),
        source_operation_id: CollaborationOperationId::try_new("cancel-settlement-delete").unwrap(),
        settlement: CollaborationTargetSettlementState::Cancelled,
        source_event_seq: cancelled.event_seq,
        acknowledged_at_ms: T0 + 4,
    };
    let target_first = target
        .settle_inbound_delivery(target_settlement_input.clone())
        .expect("target settlement must succeed")
        .into_value();
    let target_record = target
        .inbound_delivery_snapshot(&delivery_id)
        .expect("target settlement must remain queryable");
    assert_eq!(
        target_record
            .settlement
            .as_ref()
            .expect("target settlement record must exist")
            .state,
        CollaborationTargetSettlementState::Cancelled
    );
    assert!(
        !target.has_unsettled_collaboration_for_edit_or_delete(),
        "the target has locally settled; service deletion must durably forward this receipt to the source"
    );
    let target_revision = target_record.target_revision;

    let target_retry = target
        .settle_inbound_delivery(target_settlement_input)
        .expect("exact target settlement retry must be idempotent");
    assert!(!target_retry.changed());
    assert_eq!(target_retry.into_value(), target_first);
    assert_eq!(
        target
            .inbound_delivery_snapshot(&delivery_id)
            .expect("target settlement must remain resident")
            .target_revision,
        target_revision,
        "target settlement retry must preserve target revision"
    );

    let source_ack_input = AcknowledgeDeliverySettlementInput {
        operation_id: CollaborationOperationId::try_new("source-ack-cancellation").unwrap(),
        delivery_id: delivery_id.clone(),
        question_id,
        settlement: CollaborationTargetSettlementState::Cancelled,
        source_event_seq: cancelled.event_seq,
        target_event_seq: target_first.event_seq,
        target_revision,
        acknowledged_at_ms: T0 + 5,
    };
    let source_first_ack = source
        .acknowledge_delivery_settlement(source_ack_input.clone())
        .expect("source acknowledgement must succeed")
        .into_value();
    assert_eq!(
        source
            .delivery_snapshot(&delivery_id)
            .expect("source delivery must remain queryable")
            .settlement_ack
            .as_ref()
            .expect("source settlement acknowledgement must exist")
            .state,
        CollaborationTargetSettlementState::Cancelled
    );
    assert!(
        !source.has_unsettled_collaboration_for_edit_or_delete()
            && !target.has_unsettled_collaboration_for_edit_or_delete(),
        "both edit/delete guards clear only after source and target terminal receipts"
    );

    let source_ack_retry = source
        .acknowledge_delivery_settlement(source_ack_input)
        .expect("exact source acknowledgement retry must be idempotent");
    assert!(!source_ack_retry.changed());
    assert_eq!(source_ack_retry.into_value(), source_first_ack);
}
