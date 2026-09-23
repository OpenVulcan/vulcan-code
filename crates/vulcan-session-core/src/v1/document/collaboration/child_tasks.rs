//! Parent-bound child-task lifecycle mutations for the V1 session document.
//! V1 会话文档中按父回合绑定的子任务生命周期变更。

use super::*;

/// Reconstructs the child-task snapshot retained by an idempotent operation.
/// 返回幂等操作所保留的原始子任务快照。
fn child_task_receipt(
    document: &SessionDocument,
    operation: &CollaborationOperationRecord,
) -> Result<CollaborationChildTaskReceipt, DomainError> {
    let CollaborationOperationResult::ChildTask {
        execution_id,
        task_id,
        state,
    } = &operation.result
    else {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.operation.result",
        });
    };
    let task = document
        .collaboration_state()
        .child_tasks
        .get(execution_id)
        .cloned()
        .ok_or_else(|| DomainError::CollaborationNotFound {
            kind: "child_task",
            identity: task_id.as_str().to_string(),
        })?;
    if task.task_id != *task_id {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.child_task.execution_binding",
        });
    }
    let mut task = task;
    // A lifecycle or original report receipt predates its later context projection.
    // 生命周期或原报告回执先于其后续上下文投影。
    task.non_success_projection = None;
    // Earlier lifecycle receipts must not acquire a result that was published later.
    // 较早生命周期回执不能获得后来才发布的结果。
    if !matches!(
        operation.semantic,
        CollaborationOperationSemantic::RecordChildTaskNonSuccess { .. }
    ) {
        task.non_success_result = None;
    }
    // Reconstruct the operation-time snapshot instead of exposing a later live state.
    // 重建操作提交时的快照，避免把之后的实时状态暴露为原始回执。
    match *state {
        CollaborationChildTaskState::Reserved => {
            task.state = CollaborationChildTaskState::Reserved;
            task.active_at_ms = None;
            task.terminal_at_ms = None;
            task.terminal_outcome = None;
        }
        CollaborationChildTaskState::Active => {
            if task.active_at_ms.is_none() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.child_task_receipt",
                });
            }
            task.state = CollaborationChildTaskState::Active;
            task.terminal_at_ms = None;
            task.terminal_outcome = None;
        }
        CollaborationChildTaskState::Terminal => {
            if task.terminal_at_ms.is_none() || task.terminal_outcome.is_none() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.child_task_receipt",
                });
            }
            task.state = CollaborationChildTaskState::Terminal;
        }
    }
    validate_child_task_record(&task)?;
    Ok(CollaborationChildTaskReceipt {
        task,
        event_seq: operation.event_seq,
    })
}

/// Requires one exact open turn in the current document generation.
/// 要求当前文档代次中存在且精确匹配的一条开放回合。
fn require_current_parent_turn(
    document: &SessionDocument,
    parent_turn_id: TurnId,
) -> Result<(), DomainError> {
    let active = document.active_turn().ok_or(DomainError::NoActiveTurn)?;
    if active.closed {
        return Err(DomainError::NoActiveTurn);
    }
    if active.turn_id != parent_turn_id {
        return Err(DomainError::TurnIdentityMismatch {
            expected: parent_turn_id.value(),
            actual: active.turn_id.value(),
        });
    }
    Ok(())
}

/// Validates the child participant and its exact task identity binding.
/// 校验子参与者以及其与子任务身份的精确绑定。
fn validate_child_task_binding(input: &ReserveChildTaskInput) -> Result<(), DomainError> {
    input
        .child
        .validate("collaboration.child_task.child", false)?;
    if input.child.task_id.as_ref() != Some(&input.task_id) {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.child_task.task_binding",
        });
    }
    Ok(())
}

/// Rejects child activation from a stale generation or a stopped execution tree.
/// 拒绝来自过时代次或已停止执行树的子任务激活。
fn validate_child_task_activation(
    document: &SessionDocument,
    task: &CollaborationChildTaskRecord,
) -> Result<(), DomainError> {
    if task.parent_segment_generation != document.segment_generation() {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.child_task.parent_segment_generation",
        });
    }
    if matches!(
        document.collaboration_run_control(),
        CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
    ) {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.run_control.child_task_activation",
        });
    }
    Ok(())
}

impl SessionDocument {
    /// Reserves one child task against the exact currently open parent turn.
    /// 将一条子任务预留并绑定到当前精确开放的父回合。
    pub fn reserve_child_task(
        &mut self,
        input: ReserveChildTaskInput,
    ) -> Result<IdempotentMutation<CollaborationChildTaskReceipt>, DomainError> {
        validate_child_task_binding(&input)?;
        validate_timestamp(
            "collaboration.child_task.reserved_at_ms",
            input.reserved_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::ReserveChildTask {
            source_tool_name: input.source_tool_name.clone(),
            execution_id: input.execution_id.clone(),
            task_id: input.task_id.clone(),
            parent_turn_id: input.parent_turn_id,
            child: input.child.clone(),
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            return Ok(IdempotentMutation::Idempotent(child_task_receipt(
                self, &existing,
            )?));
        }

        // Validate the parent binding before staging any state or allocating an event sequence.
        // 在暂存状态或分配事件序号前校验父回合绑定。
        require_current_parent_turn(self, input.parent_turn_id)?;
        if self
            .collaboration_state()
            .child_tasks
            .contains_key(&input.execution_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }

        // A reusable child session may have many historical executions, but only one live owner.
        // 可复用子会话可以有多次历史执行，但只能有一个活动所有者。
        if self.collaboration_state().child_tasks.values().any(|task| {
            task.task_id == input.task_id && task.state != CollaborationChildTaskState::Terminal
        }) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.concurrent_execution",
            });
        }

        let task = CollaborationChildTaskRecord {
            reservation_operation_id: input.operation_id.clone(),
            source_tool_name: input.source_tool_name,
            execution_id: input.execution_id.clone(),
            task_id: input.task_id.clone(),
            parent_turn_id: input.parent_turn_id,
            child: input.child,
            parent_segment_generation: self.segment_generation(),
            state: CollaborationChildTaskState::Reserved,
            reserved_at_ms: input.reserved_at_ms,
            active_at_ms: None,
            terminal_at_ms: None,
            terminal_outcome: None,
            non_success_result: None,
            non_success_projection: None,
        };
        validate_child_task_record(&task)?;

        let mut state = self.collaboration_state().clone();
        state
            .child_tasks
            .insert(task.execution_id.clone(), task.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::ChildTask {
                    execution_id: task.execution_id.clone(),
                    task_id: task.task_id.clone(),
                    state: task.state,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::ChildTaskReserved,
            CollaborationAfterState::ChildTask { task: task.clone() },
            input.reserved_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationChildTaskReceipt { task, event_seq },
        ))
    }

    /// Activates one child task only after a matching reservation exists.
    /// 仅在存在匹配预留后激活一条子任务。
    pub fn activate_child_task(
        &mut self,
        input: ActivateChildTaskInput,
    ) -> Result<IdempotentMutation<CollaborationChildTaskReceipt>, DomainError> {
        validate_timestamp("collaboration.child_task.active_at_ms", input.active_at_ms)?;
        let semantic = CollaborationOperationSemantic::ActivateChildTask {
            execution_id: input.execution_id.clone(),
            task_id: input.task_id.clone(),
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            return Ok(IdempotentMutation::Idempotent(child_task_receipt(
                self, &existing,
            )?));
        }

        let current = self
            .collaboration_state()
            .child_tasks
            .get(&input.execution_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "child_task",
                identity: input.task_id.as_str().to_string(),
            })?;
        if current.task_id != input.task_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.execution_binding",
            });
        }
        if current.state != CollaborationChildTaskState::Reserved {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.activation_transition",
            });
        }
        validate_child_task_activation(self, &current)?;

        let mut task = current;
        task.state = CollaborationChildTaskState::Active;
        task.active_at_ms = Some(input.active_at_ms);
        task.terminal_at_ms = None;
        task.terminal_outcome = None;
        validate_child_task_record(&task)?;

        let mut state = self.collaboration_state().clone();
        state
            .child_tasks
            .insert(task.execution_id.clone(), task.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::ChildTask {
                    execution_id: task.execution_id.clone(),
                    task_id: task.task_id.clone(),
                    state: task.state,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::ChildTaskActivated,
            CollaborationAfterState::ChildTask { task: task.clone() },
            input.active_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationChildTaskReceipt { task, event_seq },
        ))
    }

    /// Records an active child outcome or a failure/stop before activation, without inventing activation.
    /// 记录活动子任务结果或激活前的失败及停止，不制造激活事实。
    pub fn terminal_child_task(
        &mut self,
        input: TerminalChildTaskInput,
    ) -> Result<IdempotentMutation<CollaborationChildTaskReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.child_task.terminal_at_ms",
            input.terminal_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::TerminalChildTask {
            execution_id: input.execution_id.clone(),
            task_id: input.task_id.clone(),
            outcome: input.outcome,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            return Ok(IdempotentMutation::Idempotent(child_task_receipt(
                self, &existing,
            )?));
        }

        let current = self
            .collaboration_state()
            .child_tasks
            .get(&input.execution_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "child_task",
                identity: input.task_id.as_str().to_string(),
            })?;
        if current.task_id != input.task_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.execution_binding",
            });
        }
        let was_reserved = match current.state {
            CollaborationChildTaskState::Reserved => {
                if !matches!(
                    input.outcome,
                    CollaborationChildTaskTerminalOutcome::Failed
                        | CollaborationChildTaskTerminalOutcome::Stopped
                ) {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.child_task.reserved_terminal_outcome",
                    });
                }
                true
            }
            CollaborationChildTaskState::Active => false,
            CollaborationChildTaskState::Terminal => {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_task.terminal_transition",
                });
            }
        };

        let mut task = current;
        task.state = CollaborationChildTaskState::Terminal;
        task.terminal_at_ms = Some(input.terminal_at_ms);
        task.terminal_outcome = Some(input.outcome);
        if was_reserved {
            // Startup failure or cancellation never creates an activation fact.
            // 启动失败或取消不会创建激活事实。
            task.active_at_ms = None;
        }
        validate_child_task_record(&task)?;

        let mut state = self.collaboration_state().clone();
        state
            .child_tasks
            .insert(task.execution_id.clone(), task.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::ChildTask {
                    execution_id: task.execution_id.clone(),
                    task_id: task.task_id.clone(),
                    state: task.state,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::ChildTaskTerminal,
            CollaborationAfterState::ChildTask { task: task.clone() },
            input.terminal_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationChildTaskReceipt { task, event_seq },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::TurnOutcome;
    mod non_success;

    /// Reuses one child across parent turns without allowing old receipts to settle the new run.
    /// 跨父回合复用同一子会话，旧回执不得结算新执行。
    #[test]
    fn reused_child_execution_retains_exact_receipts_after_reload() {
        let (mut document, first_turn) = document_with_open_turn();
        let task_id = task("reused-child");
        let first_reservation =
            reserve_input(operation("first-reserve"), task_id.clone(), first_turn);
        let first_receipt = document
            .reserve_child_task(first_reservation.clone())
            .unwrap()
            .into_value();
        let first_terminal = terminal_input(
            operation("first-failed"),
            task_id.clone(),
            CollaborationChildTaskTerminalOutcome::Failed,
        );
        let first_result = document
            .terminal_child_task(first_terminal.clone())
            .unwrap()
            .into_value();
        document
            .close_turn(TurnOutcome::Stopped, 1, T0 + 4)
            .unwrap();
        let second_turn = document
            .submit_root_input_fixture("reuse child", Vec::new(), T0 + 5)
            .unwrap();
        let second_execution = CollaborationExecutionId::try_new("second-execution").unwrap();
        let mut second_reservation =
            reserve_input(operation("second-reserve"), task_id.clone(), second_turn);
        second_reservation.execution_id = second_execution.clone();
        second_reservation.reserved_at_ms = T0 + 6;
        document.reserve_child_task(second_reservation).unwrap();
        // Round-trip the actual canonical metadata and event stream, not a synthetic map.
        // 往返加载实际规范元数据及事件流，不使用模拟映射。
        let mut document = SessionDocument::from_parts(
            serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            serde_json::from_str(&serde_json::to_string(document.events()).unwrap()).unwrap(),
        )
        .unwrap();
        let before_retry = document.clone();
        assert_eq!(
            document
                .reserve_child_task(first_reservation)
                .unwrap()
                .into_value(),
            first_receipt
        );
        assert_eq!(
            document
                .terminal_child_task(first_terminal)
                .unwrap()
                .into_value(),
            first_result
        );
        assert_eq!(document, before_retry);
        assert_eq!(document.collaboration_state().child_tasks.len(), 2);
        assert!(
            document
                .completion_obligations(first_turn)
                .open_child_task_ids
                .is_empty()
        );
        assert_eq!(
            document
                .completion_obligations(second_turn)
                .open_child_task_ids,
            vec![task_id.clone()]
        );
        // A correct execution key with a different physical child cannot mutate the reservation.
        // 正确执行键搭配不同物理子会话时也不能修改预留。
        let mut wrong_child = activate_input(operation("wrong-child"), task("other-child"));
        wrong_child.execution_id = second_execution.clone();
        assert!(document.activate_child_task(wrong_child).is_err());
        assert_eq!(document, before_retry);
        let mut activation = activate_input(operation("second-active"), task_id.clone());
        activation.execution_id = second_execution.clone();
        activation.active_at_ms = T0 + 7;
        document.activate_child_task(activation).unwrap();
        let mut terminal = terminal_input(
            operation("second-completed"),
            task_id,
            CollaborationChildTaskTerminalOutcome::Completed,
        );
        terminal.execution_id = second_execution;
        terminal.terminal_at_ms = T0 + 8;
        document.terminal_child_task(terminal).unwrap();
        assert!(
            document
                .completion_obligations(second_turn)
                .open_child_task_ids
                .is_empty()
        );
        document.validate_collaboration_state().unwrap();
    }

    /// Rejects concurrent reservations at admission and during cold metadata validation.
    /// 在准入和冷元数据校验时拒绝并发预留。
    #[test]
    fn child_execution_rejects_duplicate_live_owners_and_forged_keys() {
        let (mut document, turn) = document_with_open_turn();
        let task_id = task("concurrent-child");
        let first = document
            .reserve_child_task(reserve_input(
                operation("reserve-first"),
                task_id.clone(),
                turn,
            ))
            .unwrap()
            .into_value();
        let second_id = CollaborationExecutionId::try_new("another-execution").unwrap();
        let mut competing = reserve_input(operation("reserve-competing"), task_id, turn);
        competing.execution_id = second_id.clone();
        let before = document.clone();
        assert!(matches!(
            document.reserve_child_task(competing),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.concurrent_execution"
            })
        ));
        assert_eq!(document, before);
        let mut state = document.collaboration_state().clone();
        let mut forged = first.task.clone();
        forged.execution_id = second_id.clone();
        state.child_tasks.insert(second_id.clone(), forged);
        assert!(matches!(
            state.validate(),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.concurrent_execution"
            })
        ));
        let mut state = document.collaboration_state().clone();
        state.child_tasks.clear();
        state.child_tasks.insert(second_id, first.task);
        assert!(matches!(
            state.validate(),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.key"
            })
        ));
    }
    use crate::v1::ids::SubagentTaskId;

    const T0: i64 = 1_700_000_000_000;

    /// Builds one valid child participant bound to the supplied task identity.
    /// 构造一名绑定到指定任务身份的有效子参与者。
    fn child(task_id: &SubagentTaskId) -> CollaborationParticipant {
        CollaborationParticipant::new(
            "child-session",
            "parent-session",
            "project-1",
            1,
            None,
            Some(task_id.clone()),
        )
        .expect("fixture participant must be valid")
    }

    /// Builds a parent document with one real currently open turn.
    /// 构造一份包含真实当前开放回合的父文档。
    fn document_with_open_turn() -> (SessionDocument, TurnId) {
        let mut document = SessionDocument::new("parent-session", "project-1", "parent", T0);
        let turn_id = document
            .submit_root_input_fixture("delegate", Vec::new(), T0)
            .expect("fixture root input must open a turn");
        (document, turn_id)
    }

    /// Builds a stable operation identity for a child-task mutation fixture.
    /// 构造一个用于子任务变更夹具的稳定操作身份。
    fn operation(value: &str) -> CollaborationOperationId {
        CollaborationOperationId::try_new(value).expect("fixture operation id must be valid")
    }

    /// Builds a stable child task identity for a fixture.
    /// 构造一个用于夹具的稳定子任务身份。
    fn task(value: &str) -> SubagentTaskId {
        SubagentTaskId::try_new(value).expect("fixture task id must be valid")
    }

    /// Returns one reservation input with deterministic fixture values.
    /// 返回一份使用确定性夹具值的预留输入。
    fn reserve_input(
        operation_id: CollaborationOperationId,
        task_id: SubagentTaskId,
        parent_turn_id: TurnId,
    ) -> ReserveChildTaskInput {
        ReserveChildTaskInput {
            source_tool_name: String::from("spawn_agent"),
            operation_id,
            execution_id: CollaborationExecutionId::try_new(format!(
                "{}-execution",
                task_id.as_str()
            ))
            .unwrap(),
            task_id: task_id.clone(),
            parent_turn_id,
            child: child(&task_id),
            reserved_at_ms: T0 + 1,
        }
    }

    /// Returns one activation input with deterministic fixture values.
    /// 返回一份使用确定性夹具值的激活输入。
    fn activate_input(
        operation_id: CollaborationOperationId,
        task_id: SubagentTaskId,
    ) -> ActivateChildTaskInput {
        ActivateChildTaskInput {
            operation_id,
            execution_id: CollaborationExecutionId::try_new(format!(
                "{}-execution",
                task_id.as_str()
            ))
            .unwrap(),
            task_id,
            active_at_ms: T0 + 2,
        }
    }

    /// Returns one terminal input with deterministic fixture values.
    /// 返回一份使用确定性夹具值的终态输入。
    fn terminal_input(
        operation_id: CollaborationOperationId,
        task_id: SubagentTaskId,
        outcome: CollaborationChildTaskTerminalOutcome,
    ) -> TerminalChildTaskInput {
        TerminalChildTaskInput {
            operation_id,
            execution_id: CollaborationExecutionId::try_new(format!(
                "{}-execution",
                task_id.as_str()
            ))
            .unwrap(),
            task_id,
            outcome,
            terminal_at_ms: T0 + 3,
        }
    }

    /// Verifies the complete child-task lifecycle and parent completion obligation.
    /// 验证完整子任务生命周期以及父回合完成义务。
    #[test]
    fn child_task_lifecycle_tracks_obligations_and_events() {
        let (mut document, parent_turn_id) = document_with_open_turn();
        let base_event_count = document.events().len();
        let task_id = task("child-a");
        let reserved = document
            .reserve_child_task(reserve_input(
                operation("reserve-a"),
                task_id.clone(),
                parent_turn_id,
            ))
            .expect("reservation must commit")
            .into_value();
        assert_eq!(reserved.task.state, CollaborationChildTaskState::Reserved);
        assert_eq!(
            document
                .completion_obligations(parent_turn_id)
                .open_child_task_ids,
            vec![task_id.clone()]
        );

        let active = document
            .activate_child_task(activate_input(operation("activate-a"), task_id.clone()))
            .expect("activation must commit")
            .into_value();
        assert_eq!(active.task.state, CollaborationChildTaskState::Active);
        assert_eq!(active.task.active_at_ms, Some(T0 + 2));

        let terminal = document
            .terminal_child_task(terminal_input(
                operation("terminal-a"),
                task_id.clone(),
                CollaborationChildTaskTerminalOutcome::Completed,
            ))
            .expect("terminal transition must commit")
            .into_value();
        assert_eq!(terminal.task.state, CollaborationChildTaskState::Terminal);
        assert_eq!(
            terminal.task.terminal_outcome,
            Some(CollaborationChildTaskTerminalOutcome::Completed)
        );
        assert!(
            document
                .completion_obligations(parent_turn_id)
                .open_child_task_ids
                .is_empty()
        );
        assert_eq!(document.collaboration_state().child_tasks.len(), 1);
        assert_eq!(document.events().len(), base_event_count + 3);
        let collaboration_kinds: Vec<_> = document
            .events()
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::Collaboration(collaboration) => Some(collaboration.event_kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            collaboration_kinds,
            vec![
                CollaborationEventKind::ChildTaskReserved,
                CollaborationEventKind::ChildTaskActivated,
                CollaborationEventKind::ChildTaskTerminal,
            ]
        );
    }

    /// Verifies that a reserved task records startup failure or stop without an activation event.
    /// 验证已预留任务记录启动失败或停止时不生成激活事件。
    #[test]
    fn reserved_startup_failure_or_stop_can_terminalize_without_activation() {
        for outcome in [
            CollaborationChildTaskTerminalOutcome::Failed,
            CollaborationChildTaskTerminalOutcome::Stopped,
        ] {
            let (mut document, parent_turn_id) = document_with_open_turn();
            let task_id = task("child-fail");
            document
                .reserve_child_task(reserve_input(
                    operation("reserve-fail"),
                    task_id.clone(),
                    parent_turn_id,
                ))
                .expect("reservation must commit");

            let terminal = document
                .terminal_child_task(terminal_input(
                    operation("terminal-fail"),
                    task_id,
                    outcome,
                ))
                .expect("reserved failure or stop must be terminalizable")
                .into_value();
            assert_eq!(terminal.task.state, CollaborationChildTaskState::Terminal);
            assert_eq!(terminal.task.active_at_ms, None);
            assert_eq!(terminal.task.terminal_outcome, Some(outcome));
            let collaboration_kinds: Vec<_> = document
                .events()
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::Collaboration(collaboration) => Some(collaboration.event_kind),
                    _ => None,
                })
                .collect();
            assert_eq!(
                collaboration_kinds,
                vec![
                    CollaborationEventKind::ChildTaskReserved,
                    CollaborationEventKind::ChildTaskTerminal,
                ]
            );
        }
    }

    /// Verifies that reservation requires the real current open parent turn.
    /// 验证预留要求真实存在且当前开放的父回合。
    #[test]
    fn reserve_requires_the_current_open_parent_turn() {
        let mut document = SessionDocument::new("parent-session", "project-1", "parent", T0);
        let task_id = task("child-no-turn");
        let input = reserve_input(
            operation("reserve-no-turn"),
            task_id.clone(),
            TurnId::try_new(1).expect("fixture turn id must be valid"),
        );
        assert!(matches!(
            document.reserve_child_task(input),
            Err(DomainError::NoActiveTurn)
        ));
        assert!(document.collaboration_state().child_tasks.is_empty());

        let (mut document, parent_turn_id) = document_with_open_turn();
        let wrong_turn =
            TurnId::try_new(parent_turn_id.value() + 1).expect("fixture turn id must be valid");
        let before = document.clone();
        assert!(matches!(
            document.reserve_child_task(reserve_input(
                operation("reserve-wrong-turn"),
                task_id,
                wrong_turn,
            )),
            Err(DomainError::TurnIdentityMismatch { .. })
        ));
        assert_eq!(document, before);
    }

    /// Verifies exact operation retries and semantic conflicts for child tasks.
    /// 验证子任务操作的精确重试与语义冲突行为。
    #[test]
    fn child_task_operations_are_idempotent_and_conflicts_are_rejected() {
        let (mut document, parent_turn_id) = document_with_open_turn();
        let task_id = task("child-idempotent");
        let reserve = reserve_input(
            operation("reserve-idempotent"),
            task_id.clone(),
            parent_turn_id,
        );
        let first = document
            .reserve_child_task(reserve.clone())
            .expect("reservation must commit")
            .into_value();
        let revision = document.memory_revision();
        let event_count = document.events().len();
        let second = document
            .reserve_child_task(reserve)
            .expect("same reservation must be idempotent");
        assert!(matches!(&second, IdempotentMutation::Idempotent(_)));
        assert_eq!(second.into_value(), first.clone());
        assert_eq!(document.memory_revision(), revision);
        assert_eq!(document.events().len(), event_count);

        let mut conflicting = reserve_input(
            operation("reserve-idempotent"),
            task("different-child"),
            parent_turn_id,
        );
        let conflicting_task_id = conflicting.task_id.clone();
        conflicting.child = child(&conflicting_task_id);
        assert!(matches!(
            document.reserve_child_task(conflicting),
            Err(DomainError::CollaborationConflict { .. })
        ));

        let activation = activate_input(operation("activate-idempotent"), task_id.clone());
        let first_activation = document
            .activate_child_task(activation.clone())
            .expect("activation must commit")
            .into_value();
        let second_activation = document
            .activate_child_task(activation)
            .expect("same activation must be idempotent");
        assert!(matches!(
            &second_activation,
            IdempotentMutation::Idempotent(_)
        ));
        assert_eq!(second_activation.into_value(), first_activation);

        let reserve_after_activation = document
            .reserve_child_task(reserve_input(
                operation("reserve-idempotent"),
                task_id.clone(),
                parent_turn_id,
            ))
            .expect("same reservation must retain its original receipt");
        assert!(matches!(
            &reserve_after_activation,
            IdempotentMutation::Idempotent(_)
        ));
        assert_eq!(reserve_after_activation.into_value(), first);
        assert_eq!(
            document
                .collaboration_state()
                .child_tasks
                .get(
                    &CollaborationExecutionId::try_new(format!("{}-execution", task_id.as_str()))
                        .unwrap()
                )
                .expect("current child task must remain queryable")
                .state,
            CollaborationChildTaskState::Active
        );

        let terminal = terminal_input(
            operation("terminal-idempotent"),
            task_id.clone(),
            CollaborationChildTaskTerminalOutcome::Completed,
        );
        let first_terminal = document
            .terminal_child_task(terminal.clone())
            .expect("terminal transition must commit")
            .into_value();
        let second_terminal = document
            .terminal_child_task(terminal)
            .expect("same terminal transition must be idempotent");
        assert!(matches!(
            &second_terminal,
            IdempotentMutation::Idempotent(_)
        ));
        assert_eq!(second_terminal.into_value(), first_terminal);

        let conflicting_terminal = terminal_input(
            operation("terminal-idempotent"),
            task_id,
            CollaborationChildTaskTerminalOutcome::Failed,
        );
        assert!(matches!(
            document.terminal_child_task(conflicting_terminal),
            Err(DomainError::CollaborationConflict { .. })
        ));
    }

    /// Verifies that each child-task transition requires its declared predecessor.
    /// 验证每个子任务转换都要求其声明的前置状态。
    #[test]
    fn child_task_transitions_require_the_declared_predecessor() {
        let (mut document, parent_turn_id) = document_with_open_turn();
        let task_id = task("child-transitions");
        assert!(matches!(
            document.activate_child_task(activate_input(
                operation("activate-before-reserve"),
                task_id.clone(),
            )),
            Err(DomainError::CollaborationNotFound { .. })
        ));
        assert!(matches!(
            document.terminal_child_task(terminal_input(
                operation("terminal-before-reserve"),
                task_id.clone(),
                CollaborationChildTaskTerminalOutcome::Failed,
            )),
            Err(DomainError::CollaborationNotFound { .. })
        ));

        document
            .reserve_child_task(reserve_input(
                operation("reserve-transitions"),
                task_id.clone(),
                parent_turn_id,
            ))
            .expect("reservation must commit");
        assert!(matches!(
            document.terminal_child_task(terminal_input(
                operation("terminal-completed-before-active"),
                task_id.clone(),
                CollaborationChildTaskTerminalOutcome::Completed,
            )),
            Err(DomainError::CollaborationStateViolation { .. })
        ));
        document
            .activate_child_task(activate_input(
                operation("activate-transitions"),
                task_id.clone(),
            ))
            .expect("activation must commit");
        assert!(matches!(
            document
                .activate_child_task(activate_input(operation("activate-twice"), task_id.clone(),)),
            Err(DomainError::CollaborationStateViolation { .. })
        ));
        document
            .terminal_child_task(terminal_input(
                operation("terminal-transitions"),
                task_id.clone(),
                CollaborationChildTaskTerminalOutcome::Stopped,
            ))
            .expect("terminal transition must commit");
        assert!(matches!(
            document.terminal_child_task(terminal_input(
                operation("terminal-twice"),
                task_id,
                CollaborationChildTaskTerminalOutcome::Stopped,
            )),
            Err(DomainError::CollaborationStateViolation { .. })
        ));

        let cancelled_task_id = task("child-cancelled");
        document
            .reserve_child_task(reserve_input(
                operation("reserve-cancelled"),
                cancelled_task_id.clone(),
                parent_turn_id,
            ))
            .expect("cancelled task reservation must commit");
        document
            .activate_child_task(activate_input(
                operation("activate-cancelled"),
                cancelled_task_id.clone(),
            ))
            .expect("cancelled task activation must commit");
        document
            .terminal_child_task(terminal_input(
                operation("terminal-cancelled"),
                cancelled_task_id.clone(),
                CollaborationChildTaskTerminalOutcome::Cancelled,
            ))
            .expect("cancelled task terminal transition must commit");
        assert!(matches!(
            document.activate_child_task(activate_input(
                operation("activate-after-cancel"),
                cancelled_task_id,
            )),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.activation_transition",
            })
        ));
    }

    /// Verifies activation rejects stale generations and a stopped collaboration tree.
    /// 验证激活会拒绝过时代次以及已停止的协作树。
    #[test]
    fn child_task_activation_checks_generation_and_run_control() {
        let (mut document, parent_turn_id) = document_with_open_turn();
        let task_id = task("child-activation-guards");
        document
            .reserve_child_task(reserve_input(
                operation("reserve-activation-guards"),
                task_id.clone(),
                parent_turn_id,
            ))
            .expect("reservation must commit");

        document.metadata.segment_generation += 1;
        let before_stale_generation = document.clone();
        assert!(matches!(
            document.activate_child_task(activate_input(
                operation("activate-stale-generation"),
                task_id.clone(),
            )),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_task.parent_segment_generation",
            })
        ));
        assert_eq!(document, before_stale_generation);

        let (mut stopped_document, stopped_parent_turn_id) = document_with_open_turn();
        let stopped_task_id = task("child-stopped-tree");
        stopped_document
            .reserve_child_task(reserve_input(
                operation("reserve-stopped-tree"),
                stopped_task_id.clone(),
                stopped_parent_turn_id,
            ))
            .expect("reservation must commit");
        stopped_document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: operation("stop-all-before-activation"),
                state: CollaborationRunControl::MainAndSubagents,
                changed_at_ms: T0 + 2,
            })
            .expect("run-control stop must commit");
        let before_stopped_tree = stopped_document.clone();
        assert!(matches!(
            stopped_document.activate_child_task(activate_input(
                operation("activate-stopped-tree"),
                stopped_task_id,
            )),
            Err(DomainError::CollaborationStateViolation {
                field: "collaboration.run_control.child_task_activation",
            })
        ));
        assert_eq!(stopped_document, before_stopped_tree);
    }

    /// Verifies that parent and child generations are independent bindings.
    /// 验证父代次与子代次是相互独立的绑定关系。
    #[test]
    fn child_task_activation_allows_a_different_child_generation() {
        let (mut document, parent_turn_id) = document_with_open_turn();
        document.metadata.segment_generation = 2;
        let task_id = task("child-different-generation");
        let reserved = document
            .reserve_child_task(reserve_input(
                operation("reserve-different-generation"),
                task_id.clone(),
                parent_turn_id,
            ))
            .expect("reservation must commit")
            .into_value();
        assert_eq!(reserved.task.parent_segment_generation, 2);
        assert_eq!(reserved.task.child.segment_generation, 1);

        let active = document
            .activate_child_task(activate_input(
                operation("activate-different-generation"),
                task_id,
            ))
            .expect("activation must accept the independently captured child generation")
            .into_value();
        assert_eq!(active.task.state, CollaborationChildTaskState::Active);
        assert_eq!(active.task.parent_segment_generation, 2);
        assert_eq!(active.task.child.segment_generation, 1);
    }

    /// Verifies invalid child bindings fail before any document staging.
    /// 验证无效子任务绑定会在文档暂存前失败。
    #[test]
    fn invalid_child_binding_is_rejected_before_document_staging() {
        let (mut document, parent_turn_id) = document_with_open_turn();
        let task_id = task("child-binding");
        let mut input = reserve_input(
            operation("reserve-invalid-binding"),
            task_id,
            parent_turn_id,
        );
        input.child.task_id = None;
        let before = document.clone();
        assert!(matches!(
            document.reserve_child_task(input),
            Err(DomainError::CollaborationStateViolation { .. })
        ));
        assert_eq!(document, before);
    }
}
