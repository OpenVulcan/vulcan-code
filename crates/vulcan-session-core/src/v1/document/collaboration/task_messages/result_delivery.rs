//! Source receipt and target confirmation of exact immutable task results.
//! 精确不可变任务结果的来源接收与目标确认。

use super::*;

impl SessionDocument {
    /// Accepts `result` at its original requester using `operation_id` and `now_ms`.
    /// 使用 `operation_id` 和 `now_ms` 在原请求者接纳 `result`。
    /// Returns the original durable receipt on replay; no active business or model execution is created.
    /// 重放返回原始可靠回执；不创建活动业务或模型执行。
    pub fn accept_task_result(
        &mut self,
        operation_id: CollaborationOperationId,
        result: CollaborationTaskResult,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskResultAcceptance>, DomainError> {
        result.validate()?;
        self.validate_source_participant(&result.acceptance.message.source, true)?;
        validate_timestamp("collaboration.task_result.accepted_at_ms", now_ms)?;
        let semantic = CollaborationOperationSemantic::AcceptTaskResult {
            result: result.clone(),
        };
        if let Some(previous) = self.existing_collaboration_operation(&operation_id, &semantic)? {
            let CollaborationOperationResult::TaskResultAccepted { acceptance } = previous.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.accept_replay",
                });
            };
            return Ok(IdempotentMutation::Idempotent(acceptance));
        }
        if let Some(existing) = self
            .collaboration_state()
            .received_task_results
            .get(&result.effect.delivery_id)
        {
            if existing.result != result {
                return Err(DomainError::CollaborationConflict {
                    operation_id: operation_id.as_str().to_owned(),
                });
            }
            return Ok(IdempotentMutation::Idempotent(existing.clone()));
        }
        // Validate against our original outgoing intent, not whichever business turn is now active.
        // 按本会话原始出站意图校验，不以当前活动业务回合替代。
        let outbound = self
            .collaboration_state()
            .outbound_task_messages
            .get(&result.effect.delivery_id)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.outbound_missing",
            })?;
        if outbound.message != result.acceptance.message
            || outbound.acceptance.as_ref() != Some(&result.acceptance)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.outbound_binding",
            });
        }
        let acceptance = CollaborationTaskResultAcceptance {
            result,
            operation_id: operation_id.clone(),
            event_seq: next_event_seq(self)?,
            receiver_revision: next_revision(self)?,
            accepted_at_ms: now_ms,
        };
        let mut state = self.collaboration_state().clone();
        state.received_task_results.insert(
            acceptance.result.effect.delivery_id.clone(),
            acceptance.clone(),
        );
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: acceptance.event_seq,
                result: CollaborationOperationResult::TaskResultAccepted {
                    acceptance: acceptance.clone(),
                },
            },
        );
        self.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskResultAccepted,
            CollaborationAfterState::ReceivedTaskResult {
                acceptance: acceptance.clone(),
            },
            now_ms,
        )?;
        Ok(IdempotentMutation::Committed(acceptance))
    }

    /// Confirms `acceptance` at the original executor under `operation_id` at `now_ms`.
    /// 在 `now_ms` 使用 `operation_id` 于原执行者确认 `acceptance`。
    /// Returns the original confirmation across retries, without a model acknowledgement requirement.
    /// 重试返回原确认，不要求模型反馈确认。
    pub fn acknowledge_task_result(
        &mut self,
        operation_id: CollaborationOperationId,
        acceptance: CollaborationTaskResultAcceptance,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskResultAcknowledgement>, DomainError> {
        acceptance.validate()?;
        self.validate_target_participant(&acceptance.result.acceptance.message.target)?;
        validate_timestamp("collaboration.task_result.acknowledged_at_ms", now_ms)?;
        let semantic = CollaborationOperationSemantic::AcknowledgeTaskResult {
            acceptance: acceptance.clone(),
        };
        if let Some(previous) = self.existing_collaboration_operation(&operation_id, &semantic)? {
            let CollaborationOperationResult::TaskResultAcknowledged { acknowledgement } =
                previous.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.ack_replay",
                });
            };
            return Ok(IdempotentMutation::Idempotent(acknowledgement));
        }
        let delivery_id = &acceptance.result.effect.delivery_id;
        if let Some(existing) = self
            .collaboration_state()
            .task_result_acknowledgements
            .get(delivery_id)
        {
            if existing.acceptance != acceptance {
                return Err(DomainError::CollaborationConflict {
                    operation_id: operation_id.as_str().to_owned(),
                });
            }
            return Ok(IdempotentMutation::Idempotent(existing.clone()));
        }
        if self.collaboration_state().task_results.get(delivery_id) != Some(&acceptance.result) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.ack_result_binding",
            });
        }
        let acknowledgement = CollaborationTaskResultAcknowledgement {
            acceptance,
            operation_id: operation_id.clone(),
            event_seq: next_event_seq(self)?,
            target_revision: next_revision(self)?,
            acknowledged_at_ms: now_ms,
        };
        let mut state = self.collaboration_state().clone();
        state.task_result_acknowledgements.insert(
            acknowledgement.acceptance.result.effect.delivery_id.clone(),
            acknowledgement.clone(),
        );
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: acknowledgement.event_seq,
                result: CollaborationOperationResult::TaskResultAcknowledged {
                    acknowledgement: acknowledgement.clone(),
                },
            },
        );
        self.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskResultAcknowledged,
            CollaborationAfterState::TaskResultAcknowledgement {
                acknowledgement: acknowledgement.clone(),
            },
            now_ms,
        )?;
        Ok(IdempotentMutation::Committed(acknowledgement))
    }

    /// Validates result receipt ownership and every still-resident transfer event on cold hydration.
    /// 冷水合时校验结果回执归属与全部仍驻留投递事件。
    /// Returns an error for future revisions, foreign participants or missing/mismatched local events.
    /// 未来修订、外来参与者或缺失/不匹配本地事件时返回错误。
    pub(in crate::v1::document) fn validate_task_result_delivery_facts(
        &self,
    ) -> Result<(), DomainError> {
        for acceptance in self.collaboration_state().received_task_results.values() {
            self.validate_source_participant(&acceptance.result.acceptance.message.source, true)?;
            if acceptance.receiver_revision > self.memory_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.accept_future_revision",
                });
            }
            if acceptance.event_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event| {
                event.seq == acceptance.event_seq && event.turn_id.is_none()
                    && event.visibility == EventVisibility::HostOnly && event.created_at_ms == acceptance.accepted_at_ms
                    && matches!(&event.event, EventBody::Collaboration(collaboration)
                        if matches!(&collaboration.after_state, CollaborationAfterState::ReceivedTaskResult { acceptance: stored } if stored == acceptance))
            }) {
                return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.accept_resident_fact" });
            }
        }
        for acknowledgement in self
            .collaboration_state()
            .task_result_acknowledgements
            .values()
        {
            self.validate_target_participant(
                &acknowledgement.acceptance.result.acceptance.message.target,
            )?;
            if acknowledgement.target_revision > self.memory_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.ack_future_revision",
                });
            }
            if acknowledgement.event_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event| {
                event.seq == acknowledgement.event_seq && event.turn_id.is_none()
                    && event.visibility == EventVisibility::HostOnly && event.created_at_ms == acknowledgement.acknowledged_at_ms
                    && matches!(&event.event, EventBody::Collaboration(collaboration)
                        if matches!(&collaboration.after_state, CollaborationAfterState::TaskResultAcknowledgement { acknowledgement: stored } if stored == acknowledgement))
            }) {
                return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.ack_resident_fact" });
            }
        }
        Ok(())
    }
}
