//! Parent publication of exact durable child non-success results.
//! 精确可靠子非成功结果的父侧发布。

use super::*;
use crate::v1::event::{DisplayNoticeKind, TurnBoundaryPurpose, TurnClosureReason, TurnOutcome};

/// Opaque source input that cannot be constructed from transport text or deserialized by callers.
/// 不透明来源输入，调用方不能从传输文本构造或反序列化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildTaskNonSuccessInput {
    /// Validated durable child source facts, private to the construction boundary.
    /// 已校验可靠子来源事实，仅构造边界可访问。
    proof: CollaborationChildTaskNonSuccessProof,
}

impl SessionDocument {
    /// Builds an exact non-success input from a fully durable child business turn.
    /// 从完全持久化的子业务回合构造精确非成功输入。
    /// `execution_id` must match the root input; `turn_id` and `outcome` come from the trusted host.
    /// `execution_id` 必须匹配根输入；`turn_id` 和 `outcome` 来自可信宿主。
    /// Returns an opaque proof, rejecting undurable, auxiliary, successful or unrelated source facts.
    /// 返回不透明凭据，拒绝未持久化、辅助、成功或无关来源事实。
    pub fn build_child_task_non_success_input(
        &self,
        execution_id: CollaborationExecutionId,
        turn_id: TurnId,
        outcome: CollaborationChildTaskTerminalOutcome,
    ) -> Result<ChildTaskNonSuccessInput, DomainError> {
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        // A unique terminal is required; the current active turn never supplies historical ownership.
        // 必须存在唯一终态；当前活动回合绝不提供历史归属。
        let terminals = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnStats(stats) if event.turn_id == Some(turn_id) => {
                    Some((event, stats))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if terminals.len() != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.unique_terminal",
            });
        }
        // Read the exact original root and reject a task execution from a different resumed run.
        // Turn-less host collaboration facts and dynamic system context synchronized during the
        // turn belong to no other run.
        // 读取精确原根输入，拒绝来自其他续跑的任务执行。回合内同步的无回合宿主协作事实及动态系统上下文
        // 不属于其他续跑。
        let (terminal_event, terminal) = terminals[0];
        if self.events.iter().any(|event| {
            event.seq >= terminal.start_seq
                && event.seq <= terminal.end_seq
                && event.turn_id != Some(turn_id)
                && !(event.turn_id.is_none()
                    && match &event.event {
                        EventBody::Collaboration(_) => {
                            event.visibility == EventVisibility::HostOnly
                        }
                        EventBody::SystemContext(_) => true,
                        _ => false,
                    })
        }) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.source_range",
            });
        }
        if terminal.end_seq != terminal_event.seq
            || terminal.start_seq.value() < self.resident_start_seq
            || !self.events.iter().any(|event| {
                event.seq == terminal.start_seq
                    && event.turn_id == Some(turn_id)
                    && matches!(&event.event, EventBody::UserInput(input)
                    if input.client_action_id.as_str() == execution_id.as_str())
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.root_execution",
            });
        }
        // The terminal boundary distinguishes business failure from a temporary question attempt.
        // 终态边界区分业务失败与临时问题处理尝试。
        let boundaries = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnBoundary(boundary)
                    if event.turn_id == Some(turn_id)
                        && event.seq > terminal.start_seq
                        && event.seq < terminal_event.seq =>
                {
                    Some(boundary)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if boundaries.len() != 1
            || boundaries[0].purpose != TurnBoundaryPurpose::Business
            || boundaries[0].outcome != terminal.outcome
            || terminal.outcome == TurnOutcome::Completed
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.business_terminal",
            });
        }
        // An explicit host stop cannot relabel a previously failed source, or the reverse.
        // 明确宿主停止不能重标已经失败的来源，反向同样禁止。
        let source_outcome = match boundaries[0].reason {
            TurnClosureReason::Failed => CollaborationChildTaskTerminalOutcome::Failed,
            TurnClosureReason::Stopped | TurnClosureReason::Abandoned => {
                CollaborationChildTaskTerminalOutcome::Stopped
            }
            TurnClosureReason::Completed => {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.terminal_reason",
                });
            }
        };
        if boundaries[0].reason.outcome() != terminal.outcome || source_outcome != outcome {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.terminal_reason",
            });
        }
        // Failure reasons come from permanent host notes, never a model's summary of an error.
        // 失败原因来自永久宿主提示，绝不使用模型对错误的总结。
        let failure_notes = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::DisplayNote(note)
                    if event.turn_id == Some(turn_id)
                        && event.seq > terminal.start_seq
                        && event.seq < terminal_event.seq
                        && note.notice_kind == DisplayNoticeKind::SystemError =>
                {
                    Some(CollaborationChildTaskFailureNote {
                        event_seq: event.seq,
                        reason_code: note.reason_code.clone(),
                        redacted_detail: note.redacted_detail.clone(),
                    })
                }
                _ => None,
            })
            .collect();
        // Freeze source generation as well as execution; a recreated physical child is not this result.
        // 同时冻结来源代次和执行；重建的物理子会话不是此结果。
        let proof = CollaborationChildTaskNonSuccessProof {
            child_session_id: self.metadata.session_id.clone(),
            child_segment_generation: self.segment_generation(),
            execution_id,
            child_turn_id: turn_id,
            child_start_seq: terminal.start_seq,
            child_terminal_seq: terminal_event.seq,
            child_turn_outcome: terminal.outcome,
            outcome,
            failure_notes,
        };
        proof.validate()?;
        Ok(ChildTaskNonSuccessInput { proof })
    }

    /// Reconstructs a non-success proof using the original execution root and persisted host reason.
    /// 使用原执行根及持久宿主原因重建非成功凭据。
    /// `execution_id` is the parent-owned identity; returns None for open or successful business work.
    /// `execution_id` 为父侧所属身份；未闭合或成功业务返回 None。
    /// Missing or ambiguous identity and auxiliary closure remain explicit errors.
    /// 缺失或歧义身份及辅助闭合保持明确错误。
    pub fn recover_child_task_non_success_input(
        &self,
        execution_id: CollaborationExecutionId,
    ) -> Result<Option<ChildTaskNonSuccessInput>, DomainError> {
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::SubagentSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        // The root action, not the current active turn, owns this physical execution.
        // 根动作而非当前活动回合拥有此物理执行。
        let roots = self.events.iter().filter(|event| matches!(&event.event,
            EventBody::UserInput(input) if input.input_source == crate::v1::event::UserInputSource::Root
                && input.client_action_id.as_str() == execution_id.as_str())).collect::<Vec<_>>();
        if roots.len() != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.recovery_root",
            });
        }
        // Retain the exact original turn even when later work has already been admitted.
        // 即使后续任务已准入，仍保留精确原回合。
        let turn_id = roots[0]
            .turn_id
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.recovery_turn",
            })?;
        // A boundary is the explicit host cause; no diagnostic scanning participates in classification.
        // 边界是明确宿主原因；分类不参与任何诊断扫描。
        let boundaries = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnBoundary(boundary) if event.turn_id == Some(turn_id) => {
                    Some(boundary)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if boundaries.is_empty() {
            return Ok(None);
        }
        if boundaries.len() != 1 || boundaries[0].purpose != TurnBoundaryPurpose::Business {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.business_terminal",
            });
        }
        // Failure and stop are intentionally distinct even when both aggregate to a stopped turn.
        // 即使两者都聚合为停止回合，失败与停止也刻意保持不同。
        let outcome = match boundaries[0].reason {
            TurnClosureReason::Completed => return Ok(None),
            TurnClosureReason::Failed => CollaborationChildTaskTerminalOutcome::Failed,
            TurnClosureReason::Stopped | TurnClosureReason::Abandoned => {
                CollaborationChildTaskTerminalOutcome::Stopped
            }
        };
        self.build_child_task_non_success_input(execution_id, turn_id, outcome)
            .map(Some)
    }

    /// Publishes a non-success result and its owned child terminal in one parent revision.
    /// 在一次父修订内发布非成功结果及其所属子任务终态。
    /// `input` is built from a durable child; `now_ms` timestamps the parent observation.
    /// `input` 来自可靠子文档；`now_ms` 为父侧观测时间。
    /// Returns the immutable original receipt on replay, or rejects any changed source proof.
    /// 重放返回不可变原回执，来源凭据发生变化则拒绝。
    pub fn record_child_task_non_success(
        &mut self,
        input: ChildTaskNonSuccessInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationChildTaskNonSuccessResult>, DomainError> {
        validate_timestamp("collaboration.child_result.recorded_at_ms", now_ms)?;
        input.proof.validate()?;
        // The original reservation supplies parent ownership; no current-turn substitution is allowed.
        // 原预留提供父归属；禁止替换为当前回合。
        let task = self
            .collaboration_state()
            .child_tasks
            .get(&input.proof.execution_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "child_task",
                identity: input.proof.execution_id.as_str().to_owned(),
            })?;
        if task.parent_segment_generation != self.segment_generation()
            || task.child.session_id != input.proof.child_session_id
            || task.child.segment_generation != input.proof.child_segment_generation
            || task
                .child
                .turn_id
                .is_some_and(|turn| turn != input.proof.child_turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.owner",
            });
        }
        if let Some(result) = &task.non_success_result {
            if result.proof != input.proof {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.conflict",
                });
            }
            return Ok(IdempotentMutation::Idempotent(result.clone()));
        }
        if task.state == CollaborationChildTaskState::Terminal
            && task.terminal_outcome != Some(input.proof.outcome)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.terminal_conflict",
            });
        }
        // A numeric local reservation event keeps operation IDs bounded without hashing away identity.
        // 本地数字预留事件使操作 ID 有界，不通过散列丢失身份可读性。
        let reservation_seq = self
            .collaboration_state()
            .operations
            .get(&task.reservation_operation_id)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.reservation",
            })?
            .event_seq;
        // Both state changes share the atomic draft and roll back together on any conflict.
        // 两项状态变更共用原子草稿，任一冲突时一起回滚。
        self.apply_atomic_transition(move |staged| {
            if task.state != CollaborationChildTaskState::Terminal {
                staged.terminal_child_task(TerminalChildTaskInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "child-result-terminal:{}",
                        reservation_seq.value()
                    ))?,
                    execution_id: task.execution_id.clone(),
                    task_id: task.task_id.clone(),
                    outcome: input.proof.outcome,
                    terminal_at_ms: now_ms,
                })?;
            }
            // Store the report operation separately from the earlier lifecycle terminal receipt.
            // 报告操作与较早生命周期终态回执分开保存。
            let operation_id = CollaborationOperationId::try_new(format!(
                "child-result:{}",
                reservation_seq.value()
            ))?;
            // Full source facts are semantic identity, so retries cannot replace a reason or outcome.
            // 完整来源事实属于语义身份，因此重试不能替换原因或结果。
            let semantic = CollaborationOperationSemantic::RecordChildTaskNonSuccess {
                execution_id: task.execution_id.clone(),
                proof: input.proof.clone(),
            };
            if staged
                .existing_collaboration_operation(&operation_id, &semantic)?
                .is_some()
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.orphan_operation",
                });
            }
            // The parent sequence is allocated only after the terminal transition is staged.
            // 仅在暂存终态转换之后分配父事件序号。
            let result = CollaborationChildTaskNonSuccessResult {
                proof: input.proof,
                operation_id: operation_id.clone(),
                event_seq: next_event_seq(staged)?,
                parent_revision: next_revision(staged)?,
            };
            // Mutate the exact retained child and commit its complete after-state.
            // 修改精确保留子记录并提交其完整后状态。
            let mut state = staged.collaboration_state().clone();
            // The record cannot disappear inside the isolated draft after its terminal was committed.
            // 终态提交后，记录不能在隔离草稿内消失。
            let child = state.child_tasks.get_mut(&task.execution_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.missing_terminal",
                },
            )?;
            child.non_success_result = Some(result.clone());
            // Freeze after-state before adding the operation to the same metadata map.
            // 在同一元数据映射加入操作前冻结后状态。
            let after = child.clone();
            state.operations.insert(
                operation_id.clone(),
                CollaborationOperationRecord {
                    operation_id: operation_id.clone(),
                    semantic,
                    event_seq: result.event_seq,
                    result: CollaborationOperationResult::ChildTask {
                        execution_id: task.execution_id,
                        task_id: task.task_id,
                        state: CollaborationChildTaskState::Terminal,
                    },
                },
            );
            staged.commit_collaboration_event(
                state,
                operation_id,
                CollaborationEventKind::ChildTaskNonSuccessRecorded,
                CollaborationAfterState::ChildTask { task: after },
                now_ms,
            )?;
            Ok(IdempotentMutation::Committed(result))
        })
    }
}
