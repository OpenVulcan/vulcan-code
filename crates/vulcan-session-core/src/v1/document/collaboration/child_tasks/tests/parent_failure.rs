//! Parent business failure settlement of the failed turn's open child tasks.
//! 父业务失败对失败回合开放子任务的结算。

use super::*;

/// A failed parent turn stops its reserved and active children in the same closure.
/// 失败的父回合在同一次闭合中停止其预留与活动子任务。
/// Returns nothing; verifies terminal outcomes, settled obligations and idempotent replay.
/// 不返回值；验证终态结果、已结清义务及幂等重放。
#[test]
fn parent_turn_failure_stops_its_open_child_tasks() {
    let (mut document, parent_turn_id) = document_with_open_turn();
    let active = task("child-active");
    let reserved = task("child-reserved");
    document
        .reserve_child_task(reserve_input(
            operation("reserve-active"),
            active.clone(),
            parent_turn_id,
        ))
        .expect("active reservation must commit");
    document
        .activate_child_task(activate_input(operation("activate-active"), active.clone()))
        .expect("activation must commit");
    document
        .reserve_child_task(reserve_input(
            operation("reserve-reserved"),
            reserved.clone(),
            parent_turn_id,
        ))
        .expect("pending reservation must commit");

    assert!(
        document
            .fail_turn_without_active_api("test.parent_failure", None, T0 + 5)
            .expect("parent failure must close its turn")
    );

    assert!(document.active_turn().is_none_or(|turn| turn.closed));
    for task_id in [&active, &reserved] {
        let record = document
            .collaboration_state()
            .child_tasks
            .values()
            .find(|record| &record.task_id == task_id)
            .expect("child task record must remain");
        assert_eq!(record.state, CollaborationChildTaskState::Terminal);
        assert_eq!(
            record.terminal_outcome,
            Some(CollaborationChildTaskTerminalOutcome::Stopped)
        );
        assert_eq!(record.terminal_at_ms, Some(T0 + 5));
    }
    assert!(
        document
            .completion_obligations(parent_turn_id)
            .open_child_task_ids
            .is_empty()
    );
    assert!(!document.has_unsettled_collaboration_for_edit_or_delete());
    // Replaying the same failure adds no facts and leaves the stopped children untouched.
    // 重放同一失败不新增事实，已停止子任务保持不变。
    let event_count = document.events().len();
    assert!(
        !document
            .fail_turn_without_active_api("test.parent_failure", None, T0 + 6)
            .expect("replay must be accepted")
    );
    assert_eq!(document.events().len(), event_count);
}

/// An open child whose parent turn already closed is stopped once, and never while the turn is open.
/// 父回合已闭合的开放子任务只被停止一次，回合开放期间绝不停止。
/// Returns nothing; the closed turn is forced directly to model state left by older builds.
/// 不返回值；直接强制闭合回合以模拟旧版本遗留状态。
#[test]
fn child_of_closed_parent_turn_is_stopped_once() {
    let (mut document, parent_turn_id) = document_with_open_turn();
    let active = task("child-orphan");
    let reservation = document
        .reserve_child_task(reserve_input(
            operation("reserve-orphan"),
            active.clone(),
            parent_turn_id,
        ))
        .expect("reservation must commit")
        .into_value();
    document
        .activate_child_task(activate_input(operation("activate-orphan"), active))
        .expect("activation must commit");
    let execution_id = reservation.task.execution_id;

    assert!(
        document
            .stop_child_task_of_closed_parent_turn(&execution_id, T0 + 5)
            .expect("open parent turn must be accepted")
            .is_none()
    );

    document
        .active_turn
        .as_mut()
        .expect("parent turn must be resident")
        .closed = true;
    let stopped = document
        .stop_child_task_of_closed_parent_turn(&execution_id, T0 + 6)
        .expect("closed parent turn must stop its child")
        .expect("open child must be stopped");
    assert!(stopped.changed());
    let record = &document.collaboration_state().child_tasks[&execution_id];
    assert_eq!(record.state, CollaborationChildTaskState::Terminal);
    assert_eq!(
        record.terminal_outcome,
        Some(CollaborationChildTaskTerminalOutcome::Stopped)
    );
    assert!(
        document
            .stop_child_task_of_closed_parent_turn(&execution_id, T0 + 7)
            .expect("terminal child must be accepted")
            .is_none()
    );
}
