//! Exact owned returns keep the parent open until content and terminal state publish together.
//! 精确所属回流使父回合保持开放，直至正文与终态一起发布。

use super::*;
use crate::v1::collaboration::*;

/// Allows declared host-only control events while retaining strict source-turn boundaries.
/// 允许已声明的仅宿主控制事件，同时保留严格来源回合边界。
/// Returns domain errors; forged ownership or provider visibility must still be rejected.
/// 返回领域错误；伪造归属或供应商可见性仍必须被拒绝。
#[test]
fn owned_return_source_accepts_only_unscoped_host_collaboration() -> Result<(), DomainError> {
    // Insert a real control transition inside the child's eventual terminal source range.
    // 在子任务最终终态来源范围内插入真实控制转换。
    let mut child = new_document();
    let turn = child.submit_root_input_fixture("child work", vec![], T0)?;
    child.set_collaboration_run_control(SetRunControlInput {
        operation_id: CollaborationOperationId::try_new("child-running")?,
        state: CollaborationRunControl::Running,
        changed_at_ms: T0 + 1,
    })?;
    let api = child.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let text = child
        .append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0 + 3)?
        .into_value();
    complete_with_usage(&mut child, api, UsageSnapshot::default(), T0 + 4)?;
    child.close_turn(TurnOutcome::Completed, 5, T0 + 5)?;
    advance_fixture_durable(&mut child)?;
    let request = SubagentReturnBuildRequest {
        final_turn_id: turn,
        child_task_id: SubagentTaskId::try_new("control-source-child")?,
        source_assistant_text_seq: text,
    };
    assert!(child.build_subagent_return_input(request.clone()).is_ok());
    for foreign_turn in [false, true] {
        // Only the declared no-turn, HostOnly combination is allowed through this boundary.
        // 只有已声明的无回合且 HostOnly 组合可通过此边界。
        let mut forged = child.clone();
        let control = forged
            .events
            .iter_mut()
            .find(|event| matches!(&event.event, EventBody::Collaboration(_)))
            .unwrap();
        if foreign_turn {
            control.turn_id = Some(TurnId::try_new(turn.value() + 100)?);
        } else {
            control.visibility = EventVisibility::Provider;
        }
        assert!(matches!(
            forged.build_subagent_return_input(request.clone()),
            Err(DomainError::InvalidStatistics {
                field: "child_source_range_turn"
            })
        ));
    }
    Ok(())
}

/// Builds an active owned child plus a return constructed from a real durable child document.
/// 构造活动所属子任务，以及由真实持久子文档构造的回流。
/// Returns the parent, original parent turn, execution identity and opaque validated result.
/// 返回父文档、原父回合、执行身份及不透明已校验结果。
fn owned_return_fixture() -> Result<
    (
        SessionDocument,
        TurnId,
        CollaborationExecutionId,
        SubagentReturnInput,
    ),
    DomainError,
> {
    // The child builder validates the actual completed assistant and terminal turn.
    // 子构造器校验实际已完成 assistant 和终态回合。
    let (child, final_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id,
        child_task_id: SubagentTaskId::try_new("owned-return-task")?,
        source_assistant_text_seq,
    })?;
    // These identities are reserved once and reused by all retries.
    // 这些身份只预留一次，并由所有重试复用。
    let mut parent = new_document();
    let turn = parent.submit_root_input_fixture("delegate", vec![], T0)?;
    let execution = CollaborationExecutionId::try_new(input.child_root_action_id.as_str())?;
    let task_id = SubagentTaskId::try_new("owned-return-task")?;
    parent.reserve_child_task(ReserveChildTaskInput {
        operation_id: CollaborationOperationId::try_new("owned-return-reserve")?,
        source_tool_name: String::from("spawn_agent"),
        execution_id: execution.clone(),
        task_id: task_id.clone(),
        parent_turn_id: turn,
        child: CollaborationParticipant::new(
            "child-session",
            "sess-1",
            "project-1",
            1,
            None,
            Some(task_id.clone()),
        )?,
        reserved_at_ms: T0 + 1,
    })?;
    parent.activate_child_task(ActivateChildTaskInput {
        operation_id: CollaborationOperationId::try_new("owned-return-activate")?,
        execution_id: execution.clone(),
        task_id,
        active_at_ms: T0 + 2,
    })?;
    Ok((parent, turn, execution, input))
}

/// A completed child body and its ownership release share one revision and survive cold replay.
/// 已完成子正文及其归属释放共享一个修订，并经受冷重放。
/// Returns domain errors; assertions check the gate, pending dispatch and retry publication.
/// 返回领域错误；断言检查完成门、待分发及重试发布。
#[test]
fn owned_return_publishes_completion_atomically_and_replays_after_reload() -> Result<(), DomainError>
{
    let (mut parent, turn, execution, input) = owned_return_fixture()?;
    assert_eq!(
        parent
            .completion_obligations(turn)
            .open_child_task_ids
            .len(),
        1
    );
    assert!(parent.pending_subagent_return_dispatch()?.is_none());
    let before = parent.memory_revision();
    let returned =
        parent.complete_child_task_with_return(execution.clone(), input.clone(), T0 + 30)?;
    assert!(returned.changed());
    let receipt = returned.into_value();
    assert_eq!(parent.memory_revision(), before + 1);
    assert_eq!(
        parent
            .memory_revision_receipt(before + 1)?
            .changed_event_seqs
            .len(),
        2
    );
    assert!(
        parent
            .completion_obligations(turn)
            .open_child_task_ids
            .is_empty()
    );
    assert_eq!(
        parent.pending_subagent_return_dispatch()?,
        Some(receipt.clone())
    );
    advance_fixture_durable(&mut parent)?;
    let mut restored = SessionDocument::from_parts(
        parent.metadata.clone(),
        parent.runtime_lock.clone(),
        parent.compactions.clone(),
        parent.segment_catalog.clone(),
        parent.events.clone(),
    )?;
    let snapshot = restored.clone();
    let retry = restored.complete_child_task_with_return(execution, input, T0 + 31)?;
    assert!(!retry.changed());
    assert_eq!(retry.into_value(), receipt);
    assert_eq!(restored, snapshot);
    Ok(())
}

/// A conflict after content staging rolls back every event and retains the original obligation.
/// 正文暂存之后的冲突回滚全部事件，并保留原完成约束。
/// Returns domain errors; assertions compare the entire owner before and after rejection.
/// 返回领域错误；断言比较拒绝前后的完整所有者。
#[test]
fn owned_return_terminal_conflict_is_failure_atomic() -> Result<(), DomainError> {
    let (mut parent, turn, execution, input) = owned_return_fixture()?;
    parent.set_collaboration_run_control(SetRunControlInput {
        operation_id: CollaborationOperationId::try_new("owned-return-reserve:terminal:completed")?,
        state: CollaborationRunControl::Running,
        changed_at_ms: T0 + 3,
    })?;
    let before = parent.clone();
    assert!(
        parent
            .complete_child_task_with_return(execution, input, T0 + 30)
            .is_err()
    );
    assert_eq!(parent, before);
    assert_eq!(
        parent
            .completion_obligations(turn)
            .open_child_task_ids
            .len(),
        1
    );
    assert!(parent.pending_subagent_return_dispatch()?.is_none());
    Ok(())
}

/// Unknown execution and a replaced parent turn cannot redirect an owned completion.
/// 未知执行和被替代父回合不能重定向所属完成。
/// Returns domain errors; rejected attempts must preserve the whole document.
/// 返回领域错误；被拒尝试必须保留整个文档。
#[test]
fn owned_return_rejects_wrong_execution_and_parent_turn() -> Result<(), DomainError> {
    let (mut parent, _, execution, input) = owned_return_fixture()?;
    let before = parent.clone();
    assert!(
        parent
            .complete_child_task_with_return(
                CollaborationExecutionId::try_new("another-execution")?,
                input.clone(),
                T0 + 30
            )
            .is_err()
    );
    assert_eq!(parent, before);
    parent.close_turn(TurnOutcome::Stopped, 1, T0 + 4)?;
    parent.submit_root_input_fixture("another task", vec![], T0 + 5)?;
    let before = parent.clone();
    assert!(
        parent
            .complete_child_task_with_return(execution, input, T0 + 30)
            .is_err()
    );
    assert_eq!(parent, before);
    Ok(())
}

/// A valid old result cannot settle another execution or generation of the same physical child.
/// 合法旧结果不能结算同一物理子会话的其他执行或代次。
/// Returns domain errors; both rejection cases preserve the full parent and its completion duty.
/// 返回领域错误；两种拒绝情形都保留完整父文档及其完成义务。
#[test]
fn owned_return_rejects_reused_child_execution_and_generation() -> Result<(), DomainError> {
    // The input is produced by the real durable source builder, not assembled by this test.
    // 输入由真实可靠来源构造器产生，不由本测试拼装。
    let (original, _, execution, input) = owned_return_fixture()?;
    // Capture the original physical participant while creating distinct new reservation owners.
    // 捕获原物理参与者，同时创建不同的新预留所有者。
    let original_task = original.collaboration_state().child_tasks[&execution].clone();
    for replaced_generation in [false, true] {
        // Both new parents contain a real active reservation for the same child session and task.
        // 两个新父文档都包含针对同一子会话和任务的真实活动预留。
        let mut parent = new_document();
        let turn = parent.submit_root_input_fixture("delegate again", vec![], T0)?;
        let mut participant = original_task.child.clone();
        let claimed_execution = if replaced_generation {
            participant.segment_generation += 1;
            execution.clone()
        } else {
            CollaborationExecutionId::try_new("a-new-physical-execution")?
        };
        parent.reserve_child_task(ReserveChildTaskInput {
            operation_id: CollaborationOperationId::try_new("reuse-reserve")?,
            source_tool_name: String::from("spawn_agent"),
            execution_id: claimed_execution.clone(),
            task_id: original_task.task_id.clone(),
            parent_turn_id: turn,
            child: participant,
            reserved_at_ms: T0 + 1,
        })?;
        parent.activate_child_task(ActivateChildTaskInput {
            operation_id: CollaborationOperationId::try_new("reuse-active")?,
            execution_id: claimed_execution.clone(),
            task_id: original_task.task_id.clone(),
            active_at_ms: T0 + 2,
        })?;
        // An existing matching physical child is insufficient authority for this old proof.
        // 存在匹配物理子会话不足以授权使用此旧凭据。
        let before = parent.clone();
        assert!(matches!(
            parent.complete_child_task_with_return(claimed_execution, input.clone(), T0 + 30),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.return_binding"
            })
        ));
        assert_eq!(parent, before);
        assert_eq!(
            parent
                .completion_obligations(turn)
                .open_child_task_ids
                .len(),
            1
        );
    }
    Ok(())
}

/// Recovery follows the original root action even after a newer child task starts.
/// 即使更新的子任务已经开始，恢复仍跟随原根动作。
/// Returns domain errors; assertions distinguish original completion, unfinished work and missing identity.
/// 返回领域错误；断言区分原完成、未结束工作及缺失身份。
#[test]
fn owned_return_recovery_never_selects_the_latest_child_turn() -> Result<(), DomainError> {
    // Build a completed source through the actual durable business fixture.
    // 通过实际可靠业务夹具构造已完成来源。
    let (mut child, turn, text) = durable_child_return_fixture()?;
    let task = SubagentTaskId::try_new("owned-return-task")?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: turn,
        child_task_id: task.clone(),
        source_assistant_text_seq: text,
    })?;
    let execution = CollaborationExecutionId::try_new(input.child_root_action_id.as_str())?;
    // A real later root input must not replace the historical result's execution owner.
    // 真实后续根输入不能替换历史结果的执行所有者。
    child.submit_root_input(
        crate::v1::document::ClientInputSubmission::new(
            crate::v1::ids::ClientActionId::try_new("later-child-run")?,
            "new task",
            vec![],
        ),
        T0 + 100,
    )?;
    advance_fixture_durable(&mut child)?;
    let recovered = child
        .recover_completed_child_return_request(&execution, task.clone())?
        .unwrap();
    assert_eq!(recovered.final_turn_id, turn);
    assert_eq!(recovered.source_assistant_text_seq, text);
    assert!(
        child
            .recover_completed_child_return_request(
                &CollaborationExecutionId::try_new("later-child-run")?,
                task.clone()
            )?
            .is_none()
    );
    assert!(
        child
            .recover_completed_child_return_request(
                &CollaborationExecutionId::try_new("absent-run")?,
                task
            )
            .is_err()
    );
    Ok(())
}

/// Reused source sequence numbers in another generation remain distinct through durable replay.
/// 另一代次复用的来源序号在可靠重放中仍保持不同。
/// Returns domain errors; checks cover overlap rejection scope, coverage frontiers and hydration.
/// 返回领域错误；检查覆盖重叠拒绝范围、覆盖前沿及水合。
#[test]
fn owned_return_generation_partitions_sequences_and_frontiers() -> Result<(), DomainError> {
    // Internal opaque inputs isolate the parent identity contract from storage-generation construction.
    // 内部不透明输入将父身份契约与存储代次构造隔离。
    let (mut parent, _, _, first) = owned_return_fixture()?;
    let mut second = first.clone();
    second.child_segment_generation += 1;
    let first_receipt = parent
        .append_subagent_return(first.clone(), T0 + 30)?
        .into_value();
    let second_receipt = parent
        .append_subagent_return(second.clone(), T0 + 31)?
        .into_value();
    assert_ne!(first_receipt.return_seq, second_receipt.return_seq);
    assert!(!parent.append_subagent_return(second, T0 + 32)?.changed());
    assert_eq!(
        parent.subagent_return_frontier_before(
            &first.child_session_id,
            first.child_segment_generation,
            Seq::try_new(first_receipt.child_turn_stats_seq.value() + 1)?
        ),
        Some(first_receipt.child_turn_stats_seq)
    );
    assert!(
        parent
            .subagent_return_frontier_before(
                &first.child_session_id,
                3,
                Seq::try_new(first_receipt.child_turn_stats_seq.value() + 1)?
            )
            .is_none()
    );
    advance_fixture_durable(&mut parent)?;
    SessionDocument::from_parts(
        parent.metadata,
        parent.runtime_lock,
        parent.compactions,
        parent.segment_catalog,
        parent.events,
    )?;
    Ok(())
}
