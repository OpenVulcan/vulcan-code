//! Exact non-success source and parent publication regressions.
//! 精确非成功来源与父侧发布回归。

use super::*;
use crate::v1::document::ClientInputSubmission;
use crate::v1::ids::ClientActionId;
mod projection;

/// Cold recovery selects the original failure even after a newer root begins.
/// 即使新根输入已经开始，冷恢复仍选择原始失败。
/// Returns nothing; verifies immutable classification, execution ownership and strict closure decoding.
/// 不返回值；验证不可变分类、执行归属及严格闭合解码。
#[test]
fn child_non_success_cold_recovery_keeps_original_failure() {
    // The first execution has actual durable failure evidence.
    // 第一个执行具有实际可靠失败证据。
    let execution = CollaborationExecutionId::try_new("cold-original-failure").unwrap();
    // Preserve its original turn before adding unrelated later work.
    // 增加无关后续工作前保留原回合。
    let (mut child, turn) = failed_child(&execution);
    assert!(
        child
            .build_child_task_non_success_input(
                execution.clone(),
                turn,
                CollaborationChildTaskTerminalOutcome::Stopped
            )
            .is_err()
    );
    // A newer open task must neither mask nor replace the original closed execution.
    // 更新的开放任务不能遮蔽或替代原已闭合执行。
    let newer = CollaborationExecutionId::try_new("cold-newer-work").unwrap();
    child
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new(newer.as_str()).unwrap(),
                "new work",
                vec![],
            ),
            T0 + 10,
        )
        .unwrap();
    // Publish the complete source before constructing opaque evidence.
    // 构造不透明证据前发布完整来源。
    let revision = child.prepare_flush_candidates().unwrap().unwrap().revision;
    child.advance_durable(revision).unwrap();
    // Cold reconstruction must produce exactly the same proof as the explicit source builder.
    // 冷重建必须产出与显式来源构造器完全相同的凭据。
    let restored = reload(&child).unwrap();
    assert_eq!(
        restored
            .recover_child_task_non_success_input(execution.clone())
            .unwrap(),
        Some(
            restored
                .build_child_task_non_success_input(
                    execution,
                    turn,
                    CollaborationChildTaskTerminalOutcome::Failed
                )
                .unwrap()
        )
    );
    assert!(
        restored
            .recover_child_task_non_success_input(newer)
            .unwrap()
            .is_none()
    );
}

/// Closure classification is required and must agree with the aggregate outcome after reload.
/// 闭合分类为必填，并且重载后必须与聚合结果一致。
/// Returns nothing; malformed source facts are rejected instead of inferred from diagnostics.
/// 不返回值；拒绝畸形来源事实，不根据诊断推断。
#[test]
fn child_non_success_closure_reason_is_strict() {
    // Produce one valid failed business closure with real events.
    // 使用真实事件产生一个有效失败业务闭合。
    let execution = CollaborationExecutionId::try_new("strict-failure-reason").unwrap();
    // Keep the valid document available for deliberate corruption checks.
    // 保留有效文档以执行刻意损坏检查。
    let (mut child, _) = failed_child(&execution);
    for event in child.events.iter_mut() {
        if let EventBody::TurnBoundary(boundary) = &mut event.event {
            // A missing reason is not a supported legacy format in this unreleased contract.
            // 本未发布契约不支持缺失原因的历史格式。
            let mut serialized = serde_json::to_value(&*boundary).unwrap();
            serialized.as_object_mut().unwrap().remove("reason");
            assert!(
                serde_json::from_value::<crate::v1::event::TurnBoundaryEvent>(serialized).is_err()
            );
            boundary.reason = crate::v1::event::TurnClosureReason::Completed;
        }
    }
    assert!(matches!(
        reload(&child),
        Err(DomainError::TurnClosureMismatch { .. })
    ));
}

/// Builds a real stopped child with an execution-owned root and permanent host error.
/// 构造具有执行所属根输入及永久宿主错误的真实停止子文档。
/// `execution` supplies root ownership; returns the durable child and its exact turn.
/// `execution` 提供根归属；返回可靠子文档及精确回合。
fn failed_child(execution: &CollaborationExecutionId) -> (SessionDocument, TurnId) {
    // The session and generation match the canonical child participant fixture.
    // 会话与代次匹配规范子参与者构造。
    let mut document = SessionDocument::new("child-session", "project-1", "child", T0);
    // Actual root action identity proves which runtime execution failed before Provider startup.
    // 实际根动作身份提供 Provider 启动前失败的运行时执行凭据。
    let turn = document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new(execution.as_str()).unwrap(),
                "work",
                vec![],
            ),
            T0,
        )
        .unwrap()
        .into_value();
    document
        .fail_execution_boundary_and_close_turn(
            "startup_failed",
            Some("original diagnostic"),
            T0 + 3,
        )
        .unwrap();
    // Publish through the Core durability helper rather than assigning revision counters.
    // 通过 Core 持久化辅助器发布，不直接赋值修订计数。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    (document, turn)
}

/// Reconstructs canonical serialized metadata and events for cold-validation assertions.
/// 重建规范序列化元数据及事件，用于冷校验断言。
/// `document` supplies real persisted state; returns recovery or its domain error.
/// `document` 提供真实持久状态；返回恢复结果或领域错误。
fn reload(document: &SessionDocument) -> Result<SessionDocument, DomainError> {
    SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog.clone(),
        serde_json::from_str(&serde_json::to_string(document.events()).unwrap()).unwrap(),
    )
}

/// Failure publication retains original invocation receipts and cold-recoverable continuation authority.
/// 失败发布保留原调用回执及可冷恢复的续跑权威。
/// Returns no value; assertions cover atomic revision, exact retry and original parent ownership.
/// 不返回值；断言覆盖原子修订、精确重试及原父归属。
#[test]
fn child_non_success_publication_is_atomic_and_recoverable() {
    // Parent ownership is established by the real reservation before any failure is reported.
    // 任何失败报告之前，通过真实预留建立父归属。
    let (mut parent, turn) = document_with_open_turn();
    parent
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: operation("parent-running"),
            state: CollaborationRunControl::Running,
            changed_at_ms: T0,
        })
        .unwrap();
    // Retain the complete original reservation to exercise replay after later result publication.
    // 保留完整原预留，在后续结果发布后验证重放。
    let reservation = reserve_input(operation("non-success-reserve"), task("failed-child"), turn);
    // Original receipt must not change when the child gains a result later.
    // 子任务后来获得结果时，原回执不能改变。
    let reserved = parent
        .reserve_child_task(reservation.clone())
        .unwrap()
        .into_value();
    // Build source facts from the exact child's durable root and failed turn.
    // 从精确子文档可靠根输入和失败回合构造来源事实。
    let (child, child_turn) = failed_child(&reservation.execution_id);
    // Opaque inputs cannot be replaced by an arbitrary mailbox payload.
    // 不透明输入不能替换为任意邮箱载荷。
    let input = child
        .build_child_task_non_success_input(
            reservation.execution_id.clone(),
            child_turn,
            CollaborationChildTaskTerminalOutcome::Failed,
        )
        .unwrap();
    // Both terminal and result must consume exactly one parent publication revision.
    // 终态和结果必须恰好消耗一个父发布修订。
    let revision = parent.memory_revision();
    // The original result receipt remains the stable source of subsequent projection.
    // 原结果回执保持为后续投影的稳定来源。
    let result = parent
        .record_child_task_non_success(input.clone(), T0 + 5)
        .unwrap()
        .into_value();
    assert_eq!(parent.memory_revision(), revision + 1);
    assert_eq!(result.proof.failure_notes[0].reason_code, "startup_failed");
    assert_eq!(
        result.proof.failure_notes[0].redacted_detail.as_deref(),
        Some("original diagnostic")
    );
    assert_eq!(
        parent.reserve_child_task(reservation).unwrap().into_value(),
        reserved
    );
    assert!(
        parent
            .completion_obligations(turn)
            .open_child_task_ids
            .is_empty()
    );
    // Pending parent result replaces the live-child obligation in the same published state.
    // 在同一发布状态中，待处理父结果替代活动子任务义务。
    let pending = parent.pending_collaboration_continuation().unwrap();
    assert_eq!(pending.turn_id, turn);
    assert_eq!(pending.acceptance_seq, result.event_seq);
    assert!(matches!(
        pending.source,
        CollaborationContinuationSource::ChildTaskResult { .. }
    ));
    assert!(
        parent
            .complete_turn_with_lifetime_usage(turn, T0 + 6, None)
            .is_err()
    );
    // Actual cold reconstruction must preserve exact receipts and the completion gate.
    // 实际冷重建必须保留精确回执与完成门。
    let mut restored = reload(&parent).unwrap();
    assert_eq!(restored.pending_collaboration_continuation(), Some(pending));
    // A repeated publication cannot allocate a fresh revision or alter a terminal reason.
    // 重复发布不能分配新修订或改变终态原因。
    let repeated = restored
        .record_child_task_non_success(input, T0 + 7)
        .unwrap();
    assert!(!repeated.changed());
    assert_eq!(repeated.into_value(), result);
    assert_eq!(restored.memory_revision(), parent.memory_revision());
}

/// Source construction rejects undurable data, unrelated executions and invented failure classifications.
/// 来源构造拒绝未持久化数据、无关执行及虚构失败分类。
/// Returns no value; all failures must occur before parent mutation is possible.
/// 不返回值；所有失败必须发生在可能修改父文档之前。
#[test]
fn child_non_success_source_requires_exact_durable_failure() {
    // The execution owns the actual root input and cannot be swapped by a caller.
    // 执行拥有实际根输入，调用方不能将其替换。
    let execution = CollaborationExecutionId::try_new("exact-failed-run").unwrap();
    // Start with authentic canonical failure facts.
    // 从真实规范失败事实开始。
    let (child, turn) = failed_child(&execution);
    assert!(
        child
            .build_child_task_non_success_input(
                CollaborationExecutionId::try_new("another-run").unwrap(),
                turn,
                CollaborationChildTaskTerminalOutcome::Failed
            )
            .is_err()
    );
    assert!(
        child
            .build_child_task_non_success_input(
                execution.clone(),
                turn,
                CollaborationChildTaskTerminalOutcome::Completed
            )
            .is_err()
    );
    // A dirty canonical source is not a durable result even when its error text is available.
    // 即使错误文本可用，脏规范来源仍不是可靠结果。
    let mut dirty = child.clone();
    dirty
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: operation("dirty-source"),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: T0 + 5,
        })
        .unwrap();
    assert!(matches!(
        dirty.build_child_task_non_success_input(
            execution.clone(),
            turn,
            CollaborationChildTaskTerminalOutcome::Failed
        ),
        Err(DomainError::SubagentSourceNotDurable { .. })
    ));
    // Removing the real diagnostic cannot create an unsupported host failure classification.
    // 移除真实诊断后，不能生成缺乏支持的宿主失败分类。
    let mut no_diagnostic = child;
    no_diagnostic
        .events
        .retain(|event| !matches!(&event.event, EventBody::DisplayNote(_)));
    assert!(
        no_diagnostic
            .build_child_task_non_success_input(
                execution,
                turn,
                CollaborationChildTaskTerminalOutcome::Failed
            )
            .is_err()
    );
}

/// Late report-operation conflicts roll back the already staged child terminal.
/// 较晚报告操作冲突会回滚已暂存的子任务终态。
/// Returns no value; the complete canonical parent must remain byte-for-byte equivalent.
/// 不返回值；完整规范父文档必须保持完全一致。
#[test]
fn child_non_success_report_conflict_rolls_back_terminal() {
    // Reserve a real child, preserving the exact numeric origin of its report operation.
    // 预留真实子任务，保留其报告操作的精确数字来源。
    let (mut parent, turn) = document_with_open_turn();
    // This reservation selects both the canonical child and the pending business owner.
    // 此预留同时选定规范子任务与待处理业务所有者。
    let reservation = reserve_input(
        operation("conflicted-report-reserve"),
        task("conflicted-child"),
        turn,
    );
    // The original reservation event supplies the bounded report-operation key.
    // 原预留事件提供有界报告操作键。
    let reserved = parent
        .reserve_child_task(reservation.clone())
        .unwrap()
        .into_value();
    parent
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: operation(&format!("child-result:{}", reserved.event_seq.value())),
            state: CollaborationRunControl::Running,
            changed_at_ms: T0 + 2,
        })
        .unwrap();
    // The child source itself is valid; failure occurs only after parent terminal staging.
    // 子来源本身有效；失败仅发生在父终态暂存之后。
    let (child, child_turn) = failed_child(&reservation.execution_id);
    // The opaque input is accepted by source validation before triggering the parent conflict.
    // 在触发父冲突之前，不透明输入已通过来源校验。
    let input = child
        .build_child_task_non_success_input(
            reservation.execution_id,
            child_turn,
            CollaborationChildTaskTerminalOutcome::Failed,
        )
        .unwrap();
    // Capture every parent field, not just the child state or revision.
    // 捕获父文档全部字段，而非仅子状态或修订。
    let before = parent.clone();
    assert!(parent.record_child_task_non_success(input, T0 + 5).is_err());
    assert_eq!(parent, before);
}

/// Changed diagnostics and orphaned report operations cannot pass exact replay or cold validation.
/// 变化的诊断和孤立报告操作不能通过精确重放或冷校验。
/// Returns no value; the original receipt and canonical document remain unchanged.
/// 不返回值；原回执和规范文档保持不变。
#[test]
fn child_non_success_rejects_changed_proof_and_orphaned_metadata() {
    // Parent and child are connected through the existing authenticated reservation fixture.
    // 父子通过已有已认证预留构造连接。
    let (mut parent, turn) = document_with_open_turn();
    // Preserve exact execution ownership across changed-source attempts.
    // 在来源变化尝试中保留精确执行归属。
    let reservation = reserve_input(
        operation("immutable-result-reserve"),
        task("immutable-child"),
        turn,
    );
    parent.reserve_child_task(reservation.clone()).unwrap();
    // Source diagnostics are frozen at the first publication.
    // 来源诊断在首次发布时冻结。
    let (mut child, child_turn) = failed_child(&reservation.execution_id);
    // The original opaque source proof is published once.
    // 原不透明来源凭据仅发布一次。
    let original = child
        .build_child_task_non_success_input(
            reservation.execution_id.clone(),
            child_turn,
            CollaborationChildTaskTerminalOutcome::Failed,
        )
        .unwrap();
    parent
        .record_child_task_non_success(original, T0 + 5)
        .unwrap();
    // Simulate an inconsistent source snapshot under the same immutable execution identity.
    // 模拟同一不可变执行身份下的不一致来源快照。
    for event in child.events.iter_mut() {
        if let EventBody::DisplayNote(note) = &mut event.event {
            note.redacted_detail = Some(String::from("changed diagnostic"));
        }
    }
    // A second source construction cannot bypass conflict detection at the parent.
    // 第二次来源构造不能绕过父侧冲突检测。
    let changed = child
        .build_child_task_non_success_input(
            reservation.execution_id.clone(),
            child_turn,
            CollaborationChildTaskTerminalOutcome::Failed,
        )
        .unwrap();
    // Exact comparison verifies no partial state escaped the rejection.
    // 精确比较确认拒绝过程中没有部分状态逸出。
    let before = parent.clone();
    assert!(
        parent
            .record_child_task_non_success(changed, T0 + 6)
            .is_err()
    );
    assert_eq!(parent, before);
    parent
        .metadata
        .collaboration
        .state
        .child_tasks
        .get_mut(&reservation.execution_id)
        .unwrap()
        .non_success_result = None;
    assert!(reload(&parent).is_err());
}

/// Explicit stop publishes without fabricated diagnostics, but cannot be relabelled as a host failure.
/// 明确停止无需虚构诊断即可发布，但不能重新标记为宿主失败。
/// Returns no value; a real active child and stopped business turn supply every source fact.
/// 不返回值；真实活动子任务与停止业务回合提供全部来源事实。
#[test]
fn child_non_success_stop_preserves_distinct_host_outcome() {
    // Parent reserves and activates the task before it is explicitly stopped.
    // 父会话在任务被明确停止前预留并激活它。
    let (mut parent, parent_turn) = document_with_open_turn();
    // Reuse the declared test task identity in every lifecycle transition.
    // 在每次生命周期转换中复用已声明测试任务身份。
    let task_id = task("stopped-child");
    // The original execution remains bound to this child's root input.
    // 原执行始终绑定此子任务根输入。
    let reservation = reserve_input(operation("stopped-reserve"), task_id.clone(), parent_turn);
    parent.reserve_child_task(reservation.clone()).unwrap();
    parent
        .activate_child_task(activate_input(operation("stopped-activate"), task_id))
        .unwrap();
    // Create a normal child root and close it without an error note or assistant response.
    // 创建正常子根输入，关闭时没有错误提示或助手回复。
    let mut child = SessionDocument::new("child-session", "project-1", "child", T0);
    // The source turn is derived from actual submission, never supplied by a later active task.
    // 来源回合来自实际提交，绝不由后续活动任务提供。
    let turn = child
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new(reservation.execution_id.as_str()).unwrap(),
                "work",
                vec![],
            ),
            T0,
        )
        .unwrap()
        .into_value();
    child.close_turn(TurnOutcome::Stopped, 1, T0 + 3).unwrap();
    // The proof must be built only after the complete source closure is published.
    // 仅在完整来源闭合已发布之后构造凭据。
    let revision = child.prepare_flush_candidates().unwrap().unwrap().revision;
    child.advance_durable(revision).unwrap();
    assert!(
        child
            .build_child_task_non_success_input(
                reservation.execution_id.clone(),
                turn,
                CollaborationChildTaskTerminalOutcome::Failed
            )
            .is_err()
    );
    // Stopped is a valid non-success outcome even when there is no failure diagnostic.
    // 即使没有失败诊断，停止也是有效非成功结果。
    let input = child
        .build_child_task_non_success_input(
            reservation.execution_id,
            turn,
            CollaborationChildTaskTerminalOutcome::Stopped,
        )
        .unwrap();
    // The parent preserves the explicit stop rather than synthesizing a failed assistant answer.
    // 父会话保留明确停止，而不合成失败助手回复。
    let result = parent
        .record_child_task_non_success(input, T0 + 4)
        .unwrap()
        .into_value();
    assert_eq!(
        result.proof.outcome,
        CollaborationChildTaskTerminalOutcome::Stopped
    );
    assert!(result.proof.failure_notes.is_empty());
    reload(&parent).unwrap();
}
