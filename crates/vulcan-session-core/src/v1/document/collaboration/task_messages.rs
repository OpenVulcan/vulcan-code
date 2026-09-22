//! Atomic task-intent recording and acceptance before queue or steering execution.
//! 排队或方向执行之前的原子任务意图记录及接纳。

use super::*;

mod claim;
mod effects;
mod result_delivery;
mod result_projection;
mod results;

impl SessionDocument {
    /// Records `message` at `now_ms` in the source outbox and returns the original operation receipt.
    /// 在来源出站箱按 `now_ms` 记录 `message`，返回原始操作回执。
    /// The source must still own its open turn; a replay never rewrites creation time or intent.
    /// 来源必须仍拥有开放回合；重放不改写创建时间或意图。
    pub fn send_task_message(
        &mut self,
        message: CollaborationTaskMessage,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskMessageReceipt>, DomainError> {
        message.validate()?;
        self.validate_source_participant(&message.source, true)?;
        validate_timestamp("collaboration.task_message.created_at_ms", now_ms)?;
        let semantic = CollaborationOperationSemantic::SendTaskMessage {
            message: message.clone(),
        };
        if let Some(previous) =
            self.existing_collaboration_operation(&message.operation_id, &semantic)?
        {
            let CollaborationOperationResult::TaskMessage { record } = previous.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.send_result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationTaskMessageReceipt {
                    record,
                    event_seq: previous.event_seq,
                },
            ));
        }
        if self
            .active_turn()
            .is_none_or(|turn| turn.closed || Some(turn.turn_id) != message.source.turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.source_turn",
            });
        }
        if self
            .collaboration_state()
            .outbound_task_messages
            .contains_key(&message.delivery_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: message.operation_id.as_str().to_owned(),
            });
        }
        // Source intent is committed independently of target admission; there is no cross-file transaction.
        // 来源意图独立于目标准入提交；不存在跨文件事务。
        let record = CollaborationTaskMessageRecord {
            message: message.clone(),
            created_at_ms: now_ms,
            acceptance: None,
        };
        let mut state = self.collaboration_state().clone();
        state
            .outbound_task_messages
            .insert(message.delivery_id.clone(), record.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            message.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: message.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::TaskMessage {
                    record: record.clone(),
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            message.operation_id,
            CollaborationEventKind::TaskMessageSent,
            CollaborationAfterState::OutboundTaskMessage {
                record: record.clone(),
            },
            now_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationTaskMessageReceipt { record, event_seq },
        ))
    }

    /// Accepts the exact source `message` under target `operation_id` at `now_ms`.
    /// 在 `now_ms` 使用目标 `operation_id` 接纳精确来源 `message`。
    /// Returns immutable acceptance coordinates without starting a task or inserting provider text.
    /// 返回不可变接纳坐标，不启动任务，也不插入供应商文本。
    pub fn accept_task_message(
        &mut self,
        operation_id: CollaborationOperationId,
        message: CollaborationTaskMessage,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskMessageAcceptance>, DomainError> {
        message.validate()?;
        self.validate_target_participant(&message.target)?;
        validate_timestamp("collaboration.task_message.accepted_at_ms", now_ms)?;
        let semantic = CollaborationOperationSemantic::AcceptTaskMessage {
            message: message.clone(),
        };
        if let Some(previous) = self.existing_collaboration_operation(&operation_id, &semantic)? {
            let CollaborationOperationResult::TaskMessageAcceptance { acceptance } =
                previous.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.accept_result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(acceptance));
        }
        // One source delivery has one acceptance even if recovery uses a different local operation id.
        // 即使恢复使用不同本地操作 ID，一次来源投递也只能具有一次接纳。
        if let Some(previous) = self
            .collaboration_state()
            .inbound_task_messages
            .get(&message.delivery_id)
        {
            if previous.message != message {
                return Err(DomainError::CollaborationConflict {
                    operation_id: operation_id.as_str().to_owned(),
                });
            }
            return Ok(IdempotentMutation::Idempotent(previous.clone()));
        }
        if self.metadata.archived_at_ms.is_some()
            || matches!(
                self.collaboration_run_control(),
                CollaborationRunControl::Stopped
                    | CollaborationRunControl::MainOnly
                    | CollaborationRunControl::MainAndSubagents
            )
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.target_stopped",
            });
        }
        if message.kind == CollaborationTaskMessageKind::TaskSteer
            && self.active_turn().is_none_or(|turn| {
                turn.closed
                    || Some(turn.turn_id) != message.target.turn_id
                    || turn.origin == crate::v1::document::TurnOrigin::QuestionExecution
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.target_turn",
            });
        }
        // A queued start remains unbound to the current target turn; its own execution is assigned later.
        // 排队启动保持不绑定当前目标回合；之后为它分配自己的执行。
        let acceptance = CollaborationTaskMessageAcceptance {
            message: message.clone(),
            operation_id: operation_id.clone(),
            target_revision: next_revision(self)?,
            event_seq: next_event_seq(self)?,
            accepted_at_ms: now_ms,
        };
        let mut state = self.collaboration_state().clone();
        state
            .inbound_task_messages
            .insert(message.delivery_id, acceptance.clone());
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: acceptance.event_seq,
                result: CollaborationOperationResult::TaskMessageAcceptance {
                    acceptance: acceptance.clone(),
                },
            },
        );
        self.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskMessageAccepted,
            CollaborationAfterState::InboundTaskMessage {
                acceptance: acceptance.clone(),
            },
            now_ms,
        )?;
        Ok(IdempotentMutation::Committed(acceptance))
    }

    /// Records exact target `acceptance` on its source under `operation_id` at `now_ms`.
    /// 在 `now_ms` 使用 `operation_id` 将精确目标 `acceptance` 记录到来源。
    /// Returns the original acknowledgement on replay, even after the source turn has closed.
    /// 即使来源回合已结束，重放仍返回原始确认。
    pub fn acknowledge_task_message(
        &mut self,
        operation_id: CollaborationOperationId,
        acceptance: CollaborationTaskMessageAcceptance,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskMessageReceipt>, DomainError> {
        acceptance.validate()?;
        self.validate_source_participant(&acceptance.message.source, true)?;
        validate_timestamp("collaboration.task_message.acknowledged_at_ms", now_ms)?;
        let semantic = CollaborationOperationSemantic::AcknowledgeTaskMessage {
            acceptance: acceptance.clone(),
        };
        if let Some(previous) = self.existing_collaboration_operation(&operation_id, &semantic)? {
            let CollaborationOperationResult::TaskMessage { record } = previous.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.ack_result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationTaskMessageReceipt {
                    record,
                    event_seq: previous.event_seq,
                },
            ));
        }
        let mut record = self
            .collaboration_state()
            .outbound_task_messages
            .get(&acceptance.message.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "task_message",
                identity: acceptance.message.delivery_id.as_str().to_owned(),
            })?;
        if record.message != acceptance.message
            || record
                .acceptance
                .as_ref()
                .is_some_and(|current| *current != acceptance)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: operation_id.as_str().to_owned(),
            });
        }
        record.acceptance = Some(acceptance);
        let mut state = self.collaboration_state().clone();
        state
            .outbound_task_messages
            .insert(record.message.delivery_id.clone(), record.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::TaskMessage {
                    record: record.clone(),
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskMessageAcknowledged,
            CollaborationAfterState::OutboundTaskMessage {
                record: record.clone(),
            },
            now_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationTaskMessageReceipt { record, event_seq },
        ))
    }
}

#[cfg(test)]
mod tests;
