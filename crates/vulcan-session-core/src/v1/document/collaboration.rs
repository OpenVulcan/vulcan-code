//! Atomic V1 collaboration mutations owned by `SessionDocument`.
//! `SessionDocument` 拥有的 V1 协作原子变更。

use super::super::collaboration::validate_timestamp;
use super::{DirtyFile, IdempotentMutation, SessionDocument};
use crate::v1::collaboration::*;
use crate::v1::errors::DomainError;
use crate::v1::event::{EventBody, EventVisibility, ToolCallStatus};
use crate::v1::ids::{Seq, TurnId};
use std::collections::BTreeMap;

mod child_result_projection;
mod child_results;
mod child_tasks;
pub use child_results::ChildTaskNonSuccessInput;
mod continuation;
#[cfg(test)]
mod execution_failure_tests;
mod grants;
mod projection;
mod question_execution;
mod task_messages;
mod wait_result;

#[cfg(test)]
mod regression_tests;

/// Returns the next document revision without changing the document.
/// 返回下一个文档修订号但不修改文档。
fn next_revision(document: &SessionDocument) -> Result<u64, DomainError> {
    document.next_mutation_revision()
}

/// Returns the next collaboration event sequence reserved by the document.
/// 返回文档将要保留的下一个协作事件序号。
fn next_event_seq(document: &SessionDocument) -> Result<Seq, DomainError> {
    Seq::try_new(document.id_allocator.frontier().1)
}

/// Returns whether a delivery transition is monotonic and legal.
/// 判断投递状态转移是否单调且合法。
fn delivery_transition_is_legal(
    current: CollaborationDeliveryState,
    next: CollaborationDeliveryState,
) -> bool {
    matches!(
        (current, next),
        (
            CollaborationDeliveryState::Recorded,
            CollaborationDeliveryState::Recorded
        ) | (
            CollaborationDeliveryState::Recorded,
            CollaborationDeliveryState::Accepted
        ) | (
            CollaborationDeliveryState::Recorded,
            CollaborationDeliveryState::Projected
        ) | (
            CollaborationDeliveryState::Recorded,
            CollaborationDeliveryState::Failed
        ) | (
            CollaborationDeliveryState::Accepted,
            CollaborationDeliveryState::Accepted
        ) | (
            CollaborationDeliveryState::Accepted,
            CollaborationDeliveryState::Projected
        ) | (
            CollaborationDeliveryState::Accepted,
            CollaborationDeliveryState::Failed
        ) | (
            CollaborationDeliveryState::Projected,
            CollaborationDeliveryState::Projected
        ) | (
            CollaborationDeliveryState::Failed,
            CollaborationDeliveryState::Failed
        )
    )
}

/// Returns whether a notification transition is monotonic and legal.
/// 判断通知状态转换是否单调且合法。
fn notification_transition_is_legal(
    current: CollaborationNotificationState,
    next: CollaborationNotificationState,
) -> bool {
    matches!(
        (current, next),
        (
            CollaborationNotificationState::Recorded,
            CollaborationNotificationState::Recorded
        ) | (
            CollaborationNotificationState::Recorded,
            CollaborationNotificationState::Accepted
        ) | (
            CollaborationNotificationState::Recorded,
            CollaborationNotificationState::Projected
        ) | (
            CollaborationNotificationState::Recorded,
            CollaborationNotificationState::Failed
        ) | (
            CollaborationNotificationState::Accepted,
            CollaborationNotificationState::Accepted
        ) | (
            CollaborationNotificationState::Accepted,
            CollaborationNotificationState::Projected
        ) | (
            CollaborationNotificationState::Accepted,
            CollaborationNotificationState::Failed
        ) | (
            CollaborationNotificationState::Projected,
            CollaborationNotificationState::Projected
        ) | (
            CollaborationNotificationState::Failed,
            CollaborationNotificationState::Failed
        )
    )
}

impl SessionDocument {
    /// Returns the complete typed collaboration state owned by this session.
    /// 返回当前会话拥有的完整强类型协作状态。
    pub fn collaboration_state(&self) -> &CollaborationState {
        &self.metadata.collaboration.state
    }

    /// Returns the complete typed collaboration metadata wrapper.
    /// 返回完整的强类型协作 metadata 包装结构。
    pub fn collaboration_metadata(&self) -> &CollaborationMetadata {
        &self.metadata.collaboration
    }

    /// Validates collaboration metadata and all resident collaboration events.
    /// 校验协作 metadata 以及当前驻留的全部协作事件。
    pub(super) fn validate_collaboration_state(&self) -> Result<(), DomainError> {
        self.metadata.collaboration.validate()?;
        self.validate_task_message_effect_projections()?;
        self.validate_task_result_facts()?;
        self.validate_task_result_delivery_facts()?;
        self.validate_task_result_projection_facts()?;
        self.validate_child_result_projection_facts()?;
        // Metadata survives sealed history; validate owner identity even without a resident event.
        // 元数据跨封存历史保留；没有驻留事件时也校验所有者身份。
        for record in self.collaboration_state().outbound_task_messages.values() {
            self.validate_source_participant(&record.message.source, true)?;
        }
        for acceptance in self.collaboration_state().inbound_task_messages.values() {
            self.validate_target_participant(&acceptance.message.target)?;
        }
        let last_event_seq = self.metadata.collaboration.last_event_seq;
        let history_boundary = self.metadata.collaboration.history_boundary_seq;
        if let (Some(boundary), Some(last)) = (history_boundary, last_event_seq)
            && last <= boundary
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.last_event_seq_after_history_boundary",
            });
        }
        let mut resident_collaboration = Vec::new();
        let mut latest_questions = BTreeMap::new();
        let mut latest_outbound_deliveries = BTreeMap::new();
        let mut latest_inbound_deliveries = BTreeMap::new();
        let mut latest_notifications = BTreeMap::new();
        let mut latest_inbound_notifications = BTreeMap::new();
        // Track each side independently: source acknowledgement is not target execution.
        // 独立追踪双方：来源确认不等于目标执行。
        let mut latest_outbound_task_messages = BTreeMap::new();
        let mut latest_inbound_task_messages = BTreeMap::new();
        // Application receipts must remain paired with their own immutable host event.
        // 应用回执必须与自身不可变宿主事件保持配对。
        let mut latest_task_message_effects = BTreeMap::new();
        // Frozen results retain their exact immutable target publication.
        // 冻结结果保留精确不可变目标发布。
        let mut latest_task_results = BTreeMap::new();
        let mut latest_answer_deliveries = BTreeMap::new();
        let mut latest_child_tasks = BTreeMap::new();
        let mut latest_executions = BTreeMap::new();
        let mut latest_grants = BTreeMap::new();
        let mut latest_run_control = None;
        for event in &self.events {
            if history_boundary.is_some_and(|boundary| event.seq <= boundary) {
                continue;
            }
            if let EventBody::Collaboration(collaboration) = &event.event {
                collaboration.validate()?;
                if last_event_seq.is_none_or(|last| event.seq > last) {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.last_event_seq",
                    });
                }
                let Some(operation) = self
                    .metadata
                    .collaboration
                    .state
                    .operations
                    .get(&collaboration.operation_id)
                else {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.event.operation",
                    });
                };
                if operation.event_seq != event.seq {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.event.operation_seq",
                    });
                }
                match &collaboration.after_state {
                    CollaborationAfterState::TaskResultProjection { .. }
                    | CollaborationAfterState::ReceivedTaskResult { .. }
                    | CollaborationAfterState::TaskResultAcknowledgement { .. } => {
                        // Immutable transfer events must equal their current receipt, even on a partial tail.
                        // 即使只有部分尾段，不可变投递事件也必须等于当前回执。
                        if !collaboration.matches_state(self.collaboration_state()) {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.resident.task_result_transfer",
                            });
                        }
                    }
                    CollaborationAfterState::TaskResult { result } => {
                        if result.acceptance.message.target.session_id != self.metadata.session_id
                            || result.event_seq != event.seq
                            || operation.result
                                != (CollaborationOperationResult::TaskResult {
                                    result: result.clone(),
                                })
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.resident.task_result",
                            });
                        }
                        latest_task_results.insert(result.effect.delivery_id.clone(), result);
                    }
                    CollaborationAfterState::TaskMessageEffect { effect } => {
                        if operation.result
                            != (CollaborationOperationResult::TaskMessageEffect {
                                effect: effect.clone(),
                            })
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.resident.task_message_effect",
                            });
                        }
                        latest_task_message_effects.insert(effect.delivery_id.clone(), effect);
                    }
                    CollaborationAfterState::OutboundTaskMessage { record } => {
                        if record.message.source.session_id != self.metadata.session_id
                            || operation.result
                                != (CollaborationOperationResult::TaskMessage {
                                    record: record.clone(),
                                })
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.resident.task_message_source",
                            });
                        }
                        latest_outbound_task_messages
                            .insert(record.message.delivery_id.clone(), record);
                    }
                    CollaborationAfterState::InboundTaskMessage { acceptance } => {
                        if acceptance.message.target.session_id != self.metadata.session_id
                            || operation.result
                                != (CollaborationOperationResult::TaskMessageAcceptance {
                                    acceptance: acceptance.clone(),
                                })
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.resident.task_message_target",
                            });
                        }
                        latest_inbound_task_messages
                            .insert(acceptance.message.delivery_id.clone(), acceptance);
                    }
                    CollaborationAfterState::Question { question, delivery } => {
                        latest_questions.insert(question.question_id.clone(), question.clone());
                        latest_outbound_deliveries.insert(delivery.delivery_id.clone(), delivery);
                    }
                    CollaborationAfterState::OutboundDelivery { delivery } => {
                        latest_outbound_deliveries.insert(delivery.delivery_id.clone(), delivery);
                    }
                    CollaborationAfterState::InboundDelivery { delivery } => {
                        latest_inbound_deliveries.insert(delivery.delivery_id.clone(), delivery);
                    }
                    CollaborationAfterState::OutboundNotification { notification } => {
                        latest_notifications
                            .insert(notification.notification_id.clone(), notification);
                    }
                    CollaborationAfterState::InboundNotification { notification } => {
                        latest_inbound_notifications
                            .insert(notification.notification_id.clone(), notification);
                    }
                    CollaborationAfterState::AnswerDelivery {
                        question_id,
                        delivery,
                    } => {
                        // Answer delivery advances a nested field after the last settlement event.
                        // 答案交付在最近结算事件之后推进问题内的嵌套字段。
                        // Replay that exact delta before comparing metadata with resident evidence.
                        // 将精确差量回放后，再比较元数据与驻留事件凭据。
                        if let Some(question) = latest_questions.get_mut(question_id) {
                            let answer = question.answer.as_mut().ok_or(
                                DomainError::CollaborationStateViolation {
                                    field: "collaboration.resident.answer_before_settlement",
                                },
                            )?;
                            answer.delivery = delivery.clone();
                        }
                        latest_answer_deliveries.insert(question_id.clone(), delivery);
                    }
                    CollaborationAfterState::ChildTask { task } => {
                        latest_child_tasks.insert(task.execution_id.clone(), task);
                    }
                    CollaborationAfterState::QuestionExecution { execution } => {
                        if operation.result
                            != (CollaborationOperationResult::QuestionExecution {
                                execution: execution.clone(),
                            })
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "collaboration.resident.execution_receipt",
                            });
                        }
                        latest_executions.insert(execution.execution_id.clone(), execution);
                    }
                    CollaborationAfterState::Grant { grant } => {
                        latest_grants.insert(grant.grant_id.clone(), grant);
                    }
                    CollaborationAfterState::RunControl { transition } => {
                        latest_run_control = Some(transition);
                    }
                    CollaborationAfterState::GenerationOwnershipCleared => {
                        latest_questions.clear();
                        latest_outbound_deliveries.clear();
                        latest_inbound_deliveries.clear();
                        latest_notifications.clear();
                        latest_inbound_notifications.clear();
                        latest_outbound_task_messages.clear();
                        latest_inbound_task_messages.clear();
                        latest_task_message_effects.clear();
                        latest_task_results.clear();
                        latest_answer_deliveries.clear();
                        latest_child_tasks.clear();
                        latest_executions.clear();
                        latest_grants.clear();
                        latest_run_control = None;
                    }
                }
                resident_collaboration.push((event.seq, collaboration));
            }
        }
        for (delivery_id, result) in latest_task_results {
            if self.collaboration_state().task_results.get(&delivery_id) != Some(result) {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.task_result_after_state",
                });
            }
        }
        for (delivery_id, effect) in latest_task_message_effects {
            if self
                .collaboration_state()
                .task_message_effects
                .get(&delivery_id)
                != Some(effect)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.task_message_effect_after_state",
                });
            }
        }
        for (delivery_id, record) in latest_outbound_task_messages {
            if self
                .collaboration_state()
                .outbound_task_messages
                .get(&delivery_id)
                != Some(record)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.task_message_outbound",
                });
            }
        }
        for (delivery_id, acceptance) in latest_inbound_task_messages {
            if self
                .collaboration_state()
                .inbound_task_messages
                .get(&delivery_id)
                != Some(acceptance)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.task_message_inbound",
                });
            }
        }
        for (question_id, question) in latest_questions {
            if self
                .metadata
                .collaboration
                .state
                .questions
                .get(&question_id)
                != Some(&question)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.question_after_state",
                });
            }
        }
        for (delivery_id, delivery) in latest_outbound_deliveries {
            if self
                .metadata
                .collaboration
                .state
                .outbound_deliveries
                .get(&delivery_id)
                != Some(delivery)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.outbound_after_state",
                });
            }
        }
        for (delivery_id, delivery) in latest_inbound_deliveries {
            if self
                .metadata
                .collaboration
                .state
                .inbound_deliveries
                .get(&delivery_id)
                != Some(delivery)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.inbound_after_state",
                });
            }
        }
        for (notification_id, notification) in latest_notifications {
            if self
                .metadata
                .collaboration
                .state
                .notifications
                .get(&notification_id)
                != Some(notification)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.notification_after_state",
                });
            }
        }
        for (notification_id, notification) in latest_inbound_notifications {
            if self
                .metadata
                .collaboration
                .state
                .inbound_notifications
                .get(&notification_id)
                != Some(notification)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.inbound_notification_after_state",
                });
            }
        }
        for (question_id, delivery) in latest_answer_deliveries {
            let Some(question) = self
                .metadata
                .collaboration
                .state
                .questions
                .get(&question_id)
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.answer_question",
                });
            };
            if question.answer.as_ref().map(|answer| &answer.delivery) != Some(delivery) {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.answer_after_state",
                });
            }
        }
        for (execution_id, task) in latest_child_tasks {
            if self
                .metadata
                .collaboration
                .state
                .child_tasks
                .get(&execution_id)
                != Some(task)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.child_after_state",
                });
            }
        }
        for (execution_id, execution) in latest_executions {
            if self
                .metadata
                .collaboration
                .state
                .question_executions
                .get(&execution_id)
                != Some(execution)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.execution_after_state",
                });
            }
        }
        for (grant_id, grant) in latest_grants {
            if self.metadata.collaboration.state.grants.get(&grant_id) != Some(grant) {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.grant_after_state",
                });
            }
        }
        if let Some(transition) = latest_run_control {
            if self.metadata.collaboration.state.run_control_history.last() != Some(transition)
                || self.metadata.collaboration.state.run_control != transition.state
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.resident.run_control_after_state",
                });
            }
        }
        if let Some(last) = last_event_seq {
            if last.value() == 0 {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.last_event_seq",
                });
            }
            let catalog_end = self
                .segment_catalog
                .iter()
                .filter_map(|segment| segment.end_seq)
                .map(Seq::value)
                .max()
                .unwrap_or(0);
            let known_end = self
                .metadata
                .id_frontier
                .next_seq
                .saturating_sub(1)
                .max(catalog_end)
                .max(self.events.last().map_or(0, |event| event.seq.value()));
            if last.value() > known_end {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.last_event_seq_coverage",
                });
            }
            if self.events.last().is_some_and(|event| event.seq < last) {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.last_event_seq_coverage",
                });
            }
            if self.events.first().is_some_and(|event| event.seq <= last)
                && self.events.last().is_some_and(|event| event.seq >= last)
            {
                let Some(event) = self.events.iter().find(|event| event.seq == last) else {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.last_event_seq_coverage",
                    });
                };
                let EventBody::Collaboration(event) = &event.event else {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.last_event_seq_kind",
                    });
                };
                if !event.matches_state(&self.metadata.collaboration.state) {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.after_state",
                    });
                }
            }
        } else if !resident_collaboration.is_empty() {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.last_event_seq_missing",
            });
        }
        Ok(())
    }

    /// Returns a prior operation when it is an exact retry, or rejects reuse with new semantics.
    /// 相同语义时返回旧操作，语义不同则拒绝复用操作标识。
    fn existing_collaboration_operation(
        &self,
        operation_id: &CollaborationOperationId,
        semantic: &CollaborationOperationSemantic,
    ) -> Result<Option<CollaborationOperationRecord>, DomainError> {
        let Some(existing) = self.collaboration_state().operations.get(operation_id) else {
            return Ok(None);
        };
        if existing.semantic != *semantic {
            return Err(DomainError::CollaborationConflict {
                operation_id: operation_id.as_str().to_string(),
            });
        }
        Ok(Some(existing.clone()))
    }

    /// Publishes one collaboration event and its complete metadata summary atomically.
    /// 原子发布一条协作事件及其完整 metadata 摘要。
    fn commit_collaboration_event(
        &mut self,
        state: CollaborationState,
        operation_id: CollaborationOperationId,
        kind: CollaborationEventKind,
        after_state: CollaborationAfterState,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        validate_timestamp("collaboration.updated_at_ms", now_ms)?;
        state.validate()?;
        let event = CollaborationEvent {
            version: COLLABORATION_METADATA_VERSION,
            operation_id,
            event_kind: kind,
            after_state,
        };
        event.validate()?;
        if !event.matches_state(&state) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.event_after_state",
            });
        }
        self.ensure_transition_capacity(1, 0)?;
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(1);
        // A task claim is the authoritative start of its new business turn.
        // 任务认领是新业务回合的权威起点。
        let event_turn_id = match &event.after_state {
            CollaborationAfterState::TaskMessageEffect { effect } => match &effect.outcome {
                CollaborationTaskMessageEffectKind::Claimed { turn_id, .. } => Some(*turn_id),
                _ => None,
            },
            _ => None,
        };
        let seq = self.stage_event(
            &mut staged_allocator,
            &mut staged_events,
            event_turn_id,
            EventVisibility::HostOnly,
            EventBody::Collaboration(event),
            now_ms,
        )?;
        let mut metadata = self.metadata.collaboration.clone();
        metadata.state = state;
        metadata.last_event_seq = Some(seq);
        metadata.validate()?;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.metadata.collaboration = metadata;
        self.bump_revision(&[DirtyFile::ActiveTail], now_ms, &[seq]);
        Ok(seq)
    }

    /// Validates a source participant against this source-owned document.
    /// 校验来源参与者是否属于当前来源会话。
    fn validate_source_participant(
        &self,
        participant: &CollaborationParticipant,
        require_turn: bool,
    ) -> Result<(), DomainError> {
        participant.validate("collaboration.source", require_turn)?;
        if participant.session_id != self.metadata.session_id
            || participant.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "source_mutation",
                identity: participant.session_id.clone(),
            });
        }
        Ok(())
    }

    /// Validates a target participant against this target-owned document.
    /// 校验目标参与者是否属于当前目标会话与项目。
    fn validate_target_participant(
        &self,
        participant: &CollaborationParticipant,
    ) -> Result<(), DomainError> {
        participant.validate("collaboration.target", false)?;
        if participant.session_id != self.metadata.session_id
            || participant.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "target_mutation",
                identity: participant.session_id.clone(),
            });
        }
        Ok(())
    }

    /// Creates a source-owned question and its initial sender delivery record.
    /// 创建来源拥有的问题及其初始发送投递记录。
    pub fn create_question(
        &mut self,
        input: CreateQuestionInput,
    ) -> Result<IdempotentMutation<CollaborationQuestionReceipt>, DomainError> {
        self.validate_source_participant(&input.source, false)?;
        input.target.validate("collaboration.target", false)?;
        let Some(source_turn_id) = input.source.turn_id else {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.source.turn_id",
            });
        };
        validate_body("collaboration.question.body", &input.body)?;
        validate_timestamp("collaboration.question.created_at_ms", input.created_at_ms)?;
        if input.execution_purpose == CollaborationExecutionPurpose::NormalTask {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.question.execution_purpose",
            });
        }
        let semantic = CollaborationOperationSemantic::CreateQuestion {
            question_id: input.question_id.clone(),
            delivery_id: input.delivery_id.clone(),
            source: input.source.clone(),
            target: input.target.clone(),
            body: input.body.clone(),
            execution_purpose: input.execution_purpose,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Question {
                question_id,
                delivery_id,
            } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let question = self
                .collaboration_state()
                .questions
                .get(&question_id)
                .cloned()
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "question",
                    identity: question_id.as_str().to_string(),
                })?;
            let delivery = self
                .collaboration_state()
                .outbound_deliveries
                .get(&delivery_id)
                .cloned()
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationQuestionReceipt {
                    question,
                    delivery,
                    event_seq: existing.event_seq,
                },
            ));
        }
        if self
            .active_turn()
            .is_none_or(|turn| turn.closed || turn.turn_id != source_turn_id)
        {
            return Err(DomainError::ReferenceViolation {
                kind: "source_turn_id",
                value: source_turn_id.value(),
            });
        }
        if self
            .collaboration_state()
            .questions
            .contains_key(&input.question_id)
            || self
                .collaboration_state()
                .outbound_deliveries
                .contains_key(&input.delivery_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }
        let question = CollaborationQuestionRecord {
            operation_id: input.operation_id.clone(),
            question_id: input.question_id.clone(),
            delivery_id: input.delivery_id.clone(),
            source: input.source.clone(),
            target: input.target.clone(),
            body: input.body.clone(),
            execution_purpose: input.execution_purpose,
            state: CollaborationQuestionState::Pending,
            answer: None,
            cancellation: None,
            created_at_ms: input.created_at_ms,
        };
        let delivery = CollaborationDeliveryRecord {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            operation_id: input.operation_id.clone(),
            source: input.source,
            target: input.target,
            state: CollaborationDeliveryState::Recorded,
            failure: None,
            target_revision: None,
            target_event_seq: None,
            settlement_ack: None,
            updated_at_ms: input.created_at_ms,
        };
        let mut state = self.collaboration_state().clone();
        state
            .questions
            .insert(input.question_id.clone(), question.clone());
        state
            .outbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Question {
                    question_id: input.question_id.clone(),
                    delivery_id: input.delivery_id.clone(),
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::QuestionCreated,
            CollaborationAfterState::Question {
                question: question.clone(),
                delivery: delivery.clone(),
            },
            input.created_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationQuestionReceipt {
                question,
                delivery,
                event_seq,
            },
        ))
    }

    /// Records one typed one-way notification in the source outbox.
    /// 将一条强类型单向通知记录到来源出站箱。
    pub fn send_notification(
        &mut self,
        input: SendNotificationInput,
    ) -> Result<IdempotentMutation<CollaborationNotificationReceipt>, DomainError> {
        self.validate_source_participant(&input.source, false)?;
        input
            .target
            .validate("collaboration.notification.target", false)?;
        validate_body("collaboration.notification.body", &input.body)?;
        validate_timestamp(
            "collaboration.notification.created_at_ms",
            input.created_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::SendNotification {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id.clone(),
            source: input.source.clone(),
            target: input.target.clone(),
            body: input.body.clone(),
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Notification { notification } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationNotificationReceipt {
                    notification,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let Some(source_turn_id) = input.source.turn_id else {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.source.turn_id",
            });
        };
        if self
            .active_turn()
            .is_none_or(|turn| turn.closed || turn.turn_id != source_turn_id)
        {
            return Err(DomainError::ReferenceViolation {
                kind: "notification.source_turn_id",
                value: source_turn_id.value(),
            });
        }
        if self
            .collaboration_state()
            .notifications
            .contains_key(&input.notification_id)
            || self
                .collaboration_state()
                .notifications
                .values()
                .any(|notification| notification.delivery_id == input.delivery_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }
        let notification = CollaborationNotificationRecord {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id.clone(),
            operation_id: input.operation_id.clone(),
            source: input.source,
            target: input.target,
            body: input.body,
            state: CollaborationNotificationState::Recorded,
            failure: None,
            target_revision: None,
            target_event_seq: None,
            created_at_ms: input.created_at_ms,
            updated_at_ms: input.created_at_ms,
        };
        validate_notification_record(&notification)?;
        let mut state = self.collaboration_state().clone();
        state
            .notifications
            .insert(input.notification_id.clone(), notification.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Notification {
                    notification: notification.clone(),
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::NotificationSent,
            CollaborationAfterState::OutboundNotification {
                notification: notification.clone(),
            },
            input.created_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationNotificationReceipt {
                notification,
                event_seq,
            },
        ))
    }

    /// Records target-side notification progress in the source outbox.
    /// 将目标侧通知进度记录到来源出站箱。
    pub fn record_notification_progress(
        &mut self,
        input: RecordNotificationProgressInput,
    ) -> Result<IdempotentMutation<CollaborationNotificationReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.notification.updated_at_ms",
            input.updated_at_ms,
        )?;
        if input.state == CollaborationNotificationState::Failed && input.failure.is_none() {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.failure",
            });
        }
        if input.state != CollaborationNotificationState::Failed && input.failure.is_some() {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.failure_without_failed_state",
            });
        }
        if input.target_revision.is_some_and(|revision| revision == 0)
            || input.target_event_seq.is_some_and(|seq| seq.value() == 0)
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.progress_revision",
            });
        }
        let semantic = CollaborationOperationSemantic::NotificationProgress {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id.clone(),
            state: input.state,
            failure: input.failure,
            target_revision: input.target_revision,
            target_event_seq: input.target_event_seq,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::NotificationDelivery {
                snapshot: CollaborationNotificationOperationSnapshot::Outbound { notification },
            } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationNotificationReceipt {
                    notification,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .notifications
            .get(&input.notification_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "notification",
                identity: input.notification_id.as_str().to_string(),
            })?;
        if current.delivery_id != input.delivery_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.delivery_binding",
            });
        }
        if current.source.session_id != self.metadata.session_id
            || current.source.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "notification_progress",
                identity: current.source.session_id,
            });
        }
        if !notification_transition_is_legal(current.state, input.state) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.transition",
            });
        }
        if input.state == CollaborationNotificationState::Accepted
            && input.target_revision.is_none()
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.accepted_revision",
            });
        }
        if input.state == CollaborationNotificationState::Projected
            && (input.target_revision.is_none() || input.target_event_seq.is_none())
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.projected_revision",
            });
        }
        let mut notification = current;
        notification.state = input.state;
        notification.failure = input.failure;
        notification.target_revision = input.target_revision;
        notification.target_event_seq = input.target_event_seq;
        notification.updated_at_ms = input.updated_at_ms;
        validate_notification_record(&notification)?;
        let mut state = self.collaboration_state().clone();
        state
            .notifications
            .insert(input.notification_id.clone(), notification.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::NotificationDelivery {
                    snapshot: CollaborationNotificationOperationSnapshot::Outbound {
                        notification: notification.clone(),
                    },
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::NotificationProgress,
            CollaborationAfterState::OutboundNotification {
                notification: notification.clone(),
            },
            input.updated_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationNotificationReceipt {
                notification,
                event_seq,
            },
        ))
    }

    /// Accepts one delivery on the target-owned document.
    /// 在目标会话文档上接纳一条投递。
    pub fn accept_delivery(
        &mut self,
        input: AcceptDeliveryInput,
    ) -> Result<IdempotentMutation<CollaborationDeliveryReceipt>, DomainError> {
        self.validate_target_participant(&input.target)?;
        input.source.validate("collaboration.source", false)?;
        validate_timestamp(
            "collaboration.delivery.accepted_at_ms",
            input.accepted_at_ms,
        )?;
        if input.execution_purpose == CollaborationExecutionPurpose::NormalTask {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.execution_purpose",
            });
        }
        match input.execution_purpose {
            CollaborationExecutionPurpose::InTurnQuestion => {
                let bound_turn_id = input.bound_turn_id.ok_or(DomainError::InvalidStatistics {
                    field: "collaboration.delivery.bound_turn_id",
                })?;
                if self
                    .active_turn()
                    .is_none_or(|turn| turn.closed || turn.turn_id != bound_turn_id)
                {
                    return Err(DomainError::ReferenceViolation {
                        kind: "target_bound_turn_id",
                        value: bound_turn_id.value(),
                    });
                }
            }
            CollaborationExecutionPurpose::NaturallyIdleQuestion
                if input.bound_turn_id.is_some() =>
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.delivery.naturally_idle_bound_turn",
                });
            }
            CollaborationExecutionPurpose::NaturallyIdleQuestion => {}
            CollaborationExecutionPurpose::NormalTask => unreachable!(),
        }
        let semantic = CollaborationOperationSemantic::AcceptDelivery {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            source: input.source.clone(),
            target: input.target.clone(),
            bound_turn_id: input.bound_turn_id,
            execution_purpose: input.execution_purpose,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Delivery { delivery_id, .. } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .inbound_deliveries
                .get(&delivery_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "inbound_delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliveryReceipt {
                    delivery_id,
                    question_id: delivery.question_id.clone(),
                    state: delivery.state,
                    revision: delivery.target_revision,
                    event_seq: existing.event_seq,
                },
            ));
        }
        if self
            .collaboration_state()
            .inbound_deliveries
            .contains_key(&input.delivery_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }
        let revision = next_revision(self)?;
        let delivery = CollaborationInboundDeliveryRecord {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            operation_id: input.operation_id.clone(),
            source: input.source,
            target: input.target,
            bound_turn_id: input.bound_turn_id,
            execution_purpose: input.execution_purpose,
            state: CollaborationDeliveryState::Accepted,
            failure: None,
            target_revision: revision,
            projected_seq: None,
            projected_input_id: None,
            projected_turn_id: None,
            settlement: None,
            updated_at_ms: input.accepted_at_ms,
        };
        let mut state = self.collaboration_state().clone();
        state
            .inbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Delivery {
                    delivery_id: input.delivery_id.clone(),
                    state: CollaborationDeliveryState::Accepted,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::DeliveryAccepted,
            CollaborationAfterState::InboundDelivery {
                delivery: delivery.clone(),
            },
            input.accepted_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationDeliveryReceipt {
                delivery_id: delivery.delivery_id,
                question_id: delivery.question_id,
                state: delivery.state,
                revision,
                event_seq,
            },
        ))
    }

    /// Accepts one typed one-way notification without creating a task or question execution.
    /// 接纳一条强类型单向通知，不创建任务或问题执行。
    pub fn accept_notification(
        &mut self,
        input: AcceptNotificationInput,
    ) -> Result<IdempotentMutation<CollaborationInboundNotificationReceipt>, DomainError> {
        self.validate_target_participant(&input.target)?;
        input
            .source
            .validate("collaboration.notification.source", false)?;
        validate_body("collaboration.notification.body", &input.body)?;
        validate_timestamp(
            "collaboration.notification.accepted_at_ms",
            input.accepted_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::AcceptNotification {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id.clone(),
            source_operation_id: input.source_operation_id.clone(),
            source: input.source.clone(),
            target: input.target.clone(),
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::NotificationDelivery {
                snapshot: CollaborationNotificationOperationSnapshot::Inbound { notification },
            } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationInboundNotificationReceipt {
                    notification,
                    event_seq: existing.event_seq,
                },
            ));
        }
        if matches!(
            self.collaboration_run_control(),
            CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
        ) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.run_control.notification_acceptance",
            });
        }
        if let Some(turn_id) = input.target.turn_id {
            if self
                .active_turn()
                .is_none_or(|turn| turn.closed || turn.turn_id != turn_id)
            {
                return Err(DomainError::ReferenceViolation {
                    kind: "notification.target_turn_id",
                    value: turn_id.value(),
                });
            }
        } else if self.active_turn().is_some_and(|turn| !turn.closed) {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.notification.target.turn_id",
            });
        }
        if self
            .collaboration_state()
            .inbound_notifications
            .contains_key(&input.notification_id)
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }
        let revision = next_revision(self)?;
        let notification = CollaborationInboundNotificationRecord {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id,
            source_operation_id: input.source_operation_id,
            operation_id: input.operation_id.clone(),
            source: input.source,
            target: input.target,
            body: input.body,
            state: CollaborationNotificationState::Accepted,
            failure: None,
            target_revision: revision,
            projected_seq: None,
            projected_input_id: None,
            projected_turn_id: None,
            accepted_at_ms: input.accepted_at_ms,
            updated_at_ms: input.accepted_at_ms,
        };
        validate_inbound_notification_record(&notification)?;
        let mut state = self.collaboration_state().clone();
        state
            .inbound_notifications
            .insert(input.notification_id.clone(), notification.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::NotificationDelivery {
                    snapshot: CollaborationNotificationOperationSnapshot::Inbound {
                        notification: notification.clone(),
                    },
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::NotificationAccepted,
            CollaborationAfterState::InboundNotification {
                notification: notification.clone(),
            },
            input.accepted_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationInboundNotificationReceipt {
                notification,
                event_seq,
            },
        ))
    }

    /// Marks one accepted notification projected by a real Provider UserInput event.
    /// 将一条已接纳通知标记为由真实 Provider UserInput 事件投影。
    pub fn mark_notification_projected(
        &mut self,
        input: ProjectNotificationInput,
    ) -> Result<IdempotentMutation<CollaborationInboundNotificationReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.notification.projected_at_ms",
            input.projected_at_ms,
        )?;
        let event = self
            .events
            .iter()
            .find(|event| event.seq == input.projected_seq)
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "notification_projection_event",
                identity: input.projected_seq.value().to_string(),
            })?;
        if event.visibility != EventVisibility::Provider
            || event.turn_id != Some(input.projected_turn_id)
            || !matches!(
                &event.event,
            EventBody::UserInput(user_input)
                    if user_input.input_id == input.projected_input_id
                        && user_input.input_source == crate::v1::event::UserInputSource::AgentMessage
                        && user_input.client_action_id.as_str() == format!("agent-message:{}", input.delivery_id.as_str())
            )
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.projection_proof",
            });
        }
        let semantic = CollaborationOperationSemantic::ProjectNotification {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id.clone(),
            projected_seq: input.projected_seq,
            projected_input_id: input.projected_input_id,
            projected_turn_id: input.projected_turn_id,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::NotificationDelivery {
                snapshot: CollaborationNotificationOperationSnapshot::Inbound { notification },
            } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationInboundNotificationReceipt {
                    notification,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current_turn = self.active_turn().ok_or(DomainError::NoActiveTurn)?;
        if current_turn.closed || current_turn.turn_id != input.projected_turn_id {
            return Err(DomainError::ReferenceViolation {
                kind: "notification.projected_turn_id",
                value: input.projected_turn_id.value(),
            });
        }
        let current = self
            .collaboration_state()
            .inbound_notifications
            .get(&input.notification_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_notification",
                identity: input.notification_id.as_str().to_string(),
            })?;
        if current.delivery_id != input.delivery_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.delivery_binding",
            });
        }
        if current.target.session_id != self.metadata.session_id
            || current.target.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "notification_projection",
                identity: current.target.session_id,
            });
        }
        if !notification_transition_is_legal(
            current.state,
            CollaborationNotificationState::Projected,
        ) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.projection_transition",
            });
        }
        let revision = next_revision(self)?;
        let mut notification = current;
        notification.state = CollaborationNotificationState::Projected;
        notification.failure = None;
        notification.target_revision = revision;
        notification.projected_seq = Some(input.projected_seq);
        notification.projected_input_id = Some(input.projected_input_id);
        notification.projected_turn_id = Some(input.projected_turn_id);
        notification.updated_at_ms = input.projected_at_ms;
        validate_inbound_notification_record(&notification)?;
        let mut state = self.collaboration_state().clone();
        state
            .inbound_notifications
            .insert(input.notification_id.clone(), notification.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::NotificationDelivery {
                    snapshot: CollaborationNotificationOperationSnapshot::Inbound {
                        notification: notification.clone(),
                    },
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::NotificationProjected,
            CollaborationAfterState::InboundNotification {
                notification: notification.clone(),
            },
            input.projected_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationInboundNotificationReceipt {
                notification,
                event_seq,
            },
        ))
    }

    /// Records an explicit target-side one-way notification failure.
    /// 记录一条明确的目标侧单向通知失败。
    pub fn fail_notification(
        &mut self,
        input: FailNotificationInput,
    ) -> Result<IdempotentMutation<CollaborationInboundNotificationReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.notification.failed_at_ms",
            input.failed_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::FailNotification {
            notification_id: input.notification_id.clone(),
            delivery_id: input.delivery_id.clone(),
            reason: input.reason,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::NotificationDelivery {
                snapshot: CollaborationNotificationOperationSnapshot::Inbound { notification },
            } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationInboundNotificationReceipt {
                    notification,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .inbound_notifications
            .get(&input.notification_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_notification",
                identity: input.notification_id.as_str().to_string(),
            })?;
        if current.delivery_id != input.delivery_id {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.delivery_binding",
            });
        }
        if current.target.session_id != self.metadata.session_id
            || current.target.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "notification_failure",
                identity: current.target.session_id,
            });
        }
        if !notification_transition_is_legal(current.state, CollaborationNotificationState::Failed)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.failure_transition",
            });
        }
        let revision = next_revision(self)?;
        let mut notification = current;
        notification.state = CollaborationNotificationState::Failed;
        notification.failure = Some(input.reason);
        notification.target_revision = revision;
        notification.projected_seq = None;
        notification.projected_input_id = None;
        notification.projected_turn_id = None;
        notification.updated_at_ms = input.failed_at_ms;
        validate_inbound_notification_record(&notification)?;
        let mut state = self.collaboration_state().clone();
        state
            .inbound_notifications
            .insert(input.notification_id.clone(), notification.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::NotificationDelivery {
                    snapshot: CollaborationNotificationOperationSnapshot::Inbound {
                        notification: notification.clone(),
                    },
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::NotificationFailed,
            CollaborationAfterState::InboundNotification {
                notification: notification.clone(),
            },
            input.failed_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationInboundNotificationReceipt {
                notification,
                event_seq,
            },
        ))
    }

    /// Records source-side acknowledgement of an independent delivery state.
    /// 记录来源侧对独立投递状态的确认。
    pub fn record_delivery_progress(
        &mut self,
        input: RecordDeliveryProgressInput,
    ) -> Result<IdempotentMutation<CollaborationDeliveryReceipt>, DomainError> {
        validate_timestamp("collaboration.delivery.updated_at_ms", input.updated_at_ms)?;
        if input.state == CollaborationDeliveryState::Failed && input.failure.is_none() {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.failure",
            });
        }
        if input.state != CollaborationDeliveryState::Failed && input.failure.is_some() {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.failure_without_failed_state",
            });
        }
        let semantic = CollaborationOperationSemantic::DeliveryProgress {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            state: input.state,
            failure: input.failure,
            target_revision: input.target_revision,
            target_event_seq: input.target_event_seq,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Delivery { delivery_id, .. } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .outbound_deliveries
                .get(&delivery_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliveryReceipt {
                    delivery_id,
                    question_id: delivery.question_id.clone(),
                    state: delivery.state,
                    revision: delivery.target_revision.unwrap_or(self.memory_revision),
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .outbound_deliveries
            .get(&input.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "delivery",
                identity: input.delivery_id.as_str().to_string(),
            })?;
        if current.source.session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationUnauthorized {
                action: "delivery_progress",
                identity: current.source.session_id,
            });
        }
        if current.question_id != input.question_id
            || !delivery_transition_is_legal(current.state, input.state)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.delivery.transition",
            });
        }
        let mut delivery = current;
        delivery.state = input.state;
        delivery.failure = input.failure;
        delivery.target_revision = input.target_revision;
        delivery.target_event_seq = input.target_event_seq;
        delivery.updated_at_ms = input.updated_at_ms;
        validate_delivery_record(&delivery)?;
        let mut state = self.collaboration_state().clone();
        state
            .outbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Delivery {
                    delivery_id: input.delivery_id.clone(),
                    state: input.state,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::DeliveryRecorded,
            CollaborationAfterState::OutboundDelivery {
                delivery: delivery.clone(),
            },
            input.updated_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationDeliveryReceipt {
                delivery_id: delivery.delivery_id,
                question_id: delivery.question_id,
                state: delivery.state,
                revision: self.memory_revision,
                event_seq,
            },
        ))
    }

    /// Marks one accepted target delivery as projected.
    /// 将一条已接纳的目标投递标记为已投影。
    pub fn mark_delivery_projected(
        &mut self,
        input: ProjectDeliveryInput,
    ) -> Result<IdempotentMutation<CollaborationDeliveryReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.delivery.projected_at_ms",
            input.projected_at_ms,
        )?;
        if input.projected_seq.value() == 0 {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.projected_seq",
            });
        }
        let projection_event = self
            .events
            .iter()
            .find(|event| event.seq == input.projected_seq)
            .ok_or(DomainError::ReferenceViolation {
                kind: "projected_event_seq",
                value: input.projected_seq.value(),
            })?;
        let projection_matches = projection_event.visibility == EventVisibility::Provider
            && projection_event.turn_id == Some(input.projected_turn_id)
            && matches!(
                &projection_event.event,
                EventBody::UserInput(user_input)
                    if user_input.input_id == input.projected_input_id
                        && user_input.input_source == crate::v1::event::UserInputSource::AgentMessage
                        && user_input.client_action_id.as_str() == format!("agent-message:{}", input.delivery_id.as_str())
            );
        if !projection_matches {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.delivery.projection_fact",
            });
        }
        let semantic = CollaborationOperationSemantic::ProjectDelivery {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            projected_seq: input.projected_seq,
            projected_input_id: input.projected_input_id,
            projected_turn_id: input.projected_turn_id,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Delivery { delivery_id, .. } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .inbound_deliveries
                .get(&delivery_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "inbound_delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliveryReceipt {
                    delivery_id,
                    question_id: delivery.question_id.clone(),
                    state: delivery.state,
                    revision: delivery.target_revision,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .inbound_deliveries
            .get(&input.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_delivery",
                identity: input.delivery_id.as_str().to_string(),
            })?;
        if current.question_id != input.question_id
            || current.state != CollaborationDeliveryState::Accepted
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.inbound.transition",
            });
        }
        if current.execution_purpose == CollaborationExecutionPurpose::InTurnQuestion
            && current.bound_turn_id != Some(input.projected_turn_id)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.inbound.projection_turn",
            });
        }
        self.validate_target_participant(&current.target)?;
        let active_turn = self.require_open_turn()?;
        if active_turn.turn_id != input.projected_turn_id
            || !self
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| {
                    execution.question_id == input.question_id
                        && execution.state == CollaborationQuestionExecutionState::Running
                        && execution.purpose == current.execution_purpose
                        && execution.execution_turn_id.or(execution.parent_turn_id)
                            == Some(input.projected_turn_id)
                })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.inbound.projection_execution",
            });
        }
        if self
            .collaboration_state()
            .inbound_deliveries
            .values()
            .any(|delivery| {
                delivery.delivery_id != current.delivery_id
                    && delivery.projected_input_id == Some(input.projected_input_id)
            })
        {
            return Err(DomainError::CollaborationConflict {
                operation_id: input.operation_id.as_str().to_string(),
            });
        }
        let revision = next_revision(self)?;
        let mut delivery = current;
        delivery.state = CollaborationDeliveryState::Projected;
        delivery.projected_seq = Some(input.projected_seq);
        delivery.projected_input_id = Some(input.projected_input_id);
        delivery.projected_turn_id = Some(input.projected_turn_id);
        delivery.target_revision = revision;
        delivery.updated_at_ms = input.projected_at_ms;
        let mut state = self.collaboration_state().clone();
        state
            .inbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Delivery {
                    delivery_id: input.delivery_id.clone(),
                    state: CollaborationDeliveryState::Projected,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::DeliveryProjected,
            CollaborationAfterState::InboundDelivery {
                delivery: delivery.clone(),
            },
            input.projected_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationDeliveryReceipt {
                delivery_id: delivery.delivery_id,
                question_id: delivery.question_id,
                state: delivery.state,
                revision,
                event_seq,
            },
        ))
    }

    /// Records a target-side delivery failure while retaining the source question.
    /// 记录目标侧投递失败并保留来源问题以便撤销。
    pub fn fail_delivery(
        &mut self,
        input: FailDeliveryInput,
    ) -> Result<IdempotentMutation<CollaborationDeliveryReceipt>, DomainError> {
        validate_timestamp("collaboration.delivery.failed_at_ms", input.failed_at_ms)?;
        let semantic = CollaborationOperationSemantic::FailDelivery {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            reason: input.reason,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Delivery { delivery_id, .. } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .inbound_deliveries
                .get(&delivery_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "inbound_delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliveryReceipt {
                    delivery_id,
                    question_id: delivery.question_id.clone(),
                    state: delivery.state,
                    revision: delivery.target_revision,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .inbound_deliveries
            .get(&input.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_delivery",
                identity: input.delivery_id.as_str().to_string(),
            })?;
        if current.question_id != input.question_id
            || current.state == CollaborationDeliveryState::Projected
            || current.state == CollaborationDeliveryState::Failed
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.inbound.failure_transition",
            });
        }
        let revision = next_revision(self)?;
        let mut delivery = current;
        delivery.state = CollaborationDeliveryState::Failed;
        delivery.failure = Some(input.reason);
        delivery.target_revision = revision;
        delivery.updated_at_ms = input.failed_at_ms;
        validate_inbound_delivery_record(&delivery)?;
        let mut state = self.collaboration_state().clone();
        state
            .inbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Delivery {
                    delivery_id: input.delivery_id.clone(),
                    state: CollaborationDeliveryState::Failed,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::DeliveryFailed,
            CollaborationAfterState::InboundDelivery {
                delivery: delivery.clone(),
            },
            input.failed_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationDeliveryReceipt {
                delivery_id: delivery.delivery_id,
                question_id: delivery.question_id,
                state: delivery.state,
                revision,
                event_seq,
            },
        ))
    }

    /// Records the source question's winning terminal settlement on the target document.
    /// 鍦ㄧ洰鏍囨枃妗ｄ笂璁板綍鏉ユ簮闂鐨勭粓鎬佺粨绠楃‘璁ゃ€?
    pub fn settle_inbound_delivery(
        &mut self,
        input: SettleInboundDeliveryInput,
    ) -> Result<IdempotentMutation<CollaborationDeliverySettlementReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.inbound.settlement.acknowledged_at_ms",
            input.acknowledged_at_ms,
        )?;
        if input.source_event_seq.value() == 0 {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.inbound.settlement.source_event_seq",
            });
        }
        let semantic = CollaborationOperationSemantic::SettleInboundDelivery {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            source_operation_id: input.source_operation_id.clone(),
            settlement: input.settlement,
            source_event_seq: input.source_event_seq,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::DeliverySettlement { delivery_id, .. } =
                existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .inbound_deliveries
                .get(&delivery_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "inbound_delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliverySettlementReceipt {
                    delivery_id,
                    question_id: delivery.question_id.clone(),
                    settlement: delivery.settlement.clone(),
                    settlement_ack: None,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .inbound_deliveries
            .get(&input.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_delivery",
                identity: input.delivery_id.as_str().to_string(),
            })?;
        if current.question_id != input.question_id
            || current.target.session_id != self.metadata.session_id
            || current.target.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "settle_inbound_delivery",
                identity: current.target.session_id,
            });
        }
        if current.state == CollaborationDeliveryState::Failed {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.inbound.settlement.failed_delivery",
            });
        }
        if current.settlement.is_some() {
            return Err(DomainError::CollaborationAlreadySettled {
                question_id: input.question_id.as_str().to_string(),
                state: "target_settlement",
            });
        }
        let revision = next_revision(self)?;
        let settlement = CollaborationInboundSettlementRecord {
            state: input.settlement,
            source_event_seq: input.source_event_seq,
            source_operation_id: input.source_operation_id,
            target_operation_id: input.operation_id.clone(),
            acknowledged_at_ms: input.acknowledged_at_ms,
        };
        validate_inbound_settlement(&settlement)?;
        let mut delivery = current;
        delivery.settlement = Some(settlement.clone());
        delivery.target_revision = revision;
        delivery.updated_at_ms = input.acknowledged_at_ms;
        validate_inbound_delivery_record(&delivery)?;
        let mut state = self.collaboration_state().clone();
        state
            .inbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::DeliverySettlement {
                    delivery_id: input.delivery_id.clone(),
                    state: input.settlement,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::DeliverySettled,
            CollaborationAfterState::InboundDelivery {
                delivery: delivery.clone(),
            },
            input.acknowledged_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationDeliverySettlementReceipt {
                delivery_id: delivery.delivery_id,
                question_id: delivery.question_id,
                settlement: delivery.settlement,
                settlement_ack: None,
                event_seq,
            },
        ))
    }

    /// Persists the target's terminal settlement acknowledgement on the source document.
    /// 鍦ㄦ潵婧愭枃妗ｄ笂鎸佷箙鍖栫洰鏍囩粓鎬佺粨绠楃‘璁ゃ€?
    pub fn acknowledge_delivery_settlement(
        &mut self,
        input: AcknowledgeDeliverySettlementInput,
    ) -> Result<IdempotentMutation<CollaborationDeliverySettlementReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.delivery.settlement_ack.acknowledged_at_ms",
            input.acknowledged_at_ms,
        )?;
        if input.source_event_seq.value() == 0
            || input.target_event_seq.value() == 0
            || input.target_revision == 0
        {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.delivery.settlement_ack.identity",
            });
        }
        let semantic = CollaborationOperationSemantic::AcknowledgeDeliverySettlement {
            delivery_id: input.delivery_id.clone(),
            question_id: input.question_id.clone(),
            settlement: input.settlement,
            source_event_seq: input.source_event_seq,
            target_event_seq: input.target_event_seq,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::DeliverySettlement { delivery_id, .. } =
                existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .outbound_deliveries
                .get(&delivery_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "delivery",
                    identity: delivery_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliverySettlementReceipt {
                    delivery_id,
                    question_id: delivery.question_id.clone(),
                    settlement: None,
                    settlement_ack: delivery.settlement_ack.clone(),
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .outbound_deliveries
            .get(&input.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "delivery",
                identity: input.delivery_id.as_str().to_string(),
            })?;
        if current.question_id != input.question_id
            || current.source.session_id != self.metadata.session_id
            || current.source.segment_generation != self.metadata.segment_generation
        {
            return Err(DomainError::CollaborationUnauthorized {
                action: "acknowledge_delivery_settlement",
                identity: current.source.session_id,
            });
        }
        if current.state == CollaborationDeliveryState::Failed {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.delivery.settlement_ack.failed_delivery",
            });
        }
        let question = self
            .collaboration_state()
            .questions
            .get(&input.question_id)
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "question",
                identity: input.question_id.as_str().to_string(),
            })?;
        let expected = match input.settlement {
            CollaborationTargetSettlementState::Answered => CollaborationQuestionState::Answered,
            CollaborationTargetSettlementState::Cancelled => CollaborationQuestionState::Cancelled,
        };
        if question.state != expected {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.delivery.settlement_ack.question_state",
            });
        }
        if current.settlement_ack.is_some() {
            return Err(DomainError::CollaborationAlreadySettled {
                question_id: input.question_id.as_str().to_string(),
                state: "source_settlement_ack",
            });
        }
        let ack = CollaborationTargetSettlementAck {
            state: input.settlement,
            source_event_seq: input.source_event_seq,
            target_event_seq: input.target_event_seq,
            target_revision: input.target_revision,
            acknowledged_at_ms: input.acknowledged_at_ms,
        };
        validate_target_settlement_ack(&ack)?;
        let mut delivery = current;
        delivery.settlement_ack = Some(ack.clone());
        delivery.updated_at_ms = input.acknowledged_at_ms;
        validate_delivery_record(&delivery)?;
        let mut state = self.collaboration_state().clone();
        state
            .outbound_deliveries
            .insert(input.delivery_id.clone(), delivery.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::DeliverySettlement {
                    delivery_id: input.delivery_id.clone(),
                    state: input.settlement,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::DeliverySettlementAcknowledged,
            CollaborationAfterState::OutboundDelivery {
                delivery: delivery.clone(),
            },
            input.acknowledged_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationDeliverySettlementReceipt {
                delivery_id: delivery.delivery_id,
                question_id: delivery.question_id,
                settlement: None,
                settlement_ack: delivery.settlement_ack,
                event_seq,
            },
        ))
    }

    /// Settles one pending question with the first authorized target answer.
    /// 由首个获授权的目标回答结算一个待处理问题。
    pub fn reply_question(
        &mut self,
        input: ReplyQuestionInput,
    ) -> Result<IdempotentMutation<CollaborationSettlementReceipt>, DomainError> {
        input.responder.validate("collaboration.responder", false)?;
        validate_body("collaboration.answer.body", &input.body)?;
        validate_timestamp("collaboration.answer.answered_at_ms", input.answered_at_ms)?;
        let question = self
            .collaboration_state()
            .questions
            .get(&input.question_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "question",
                identity: input.question_id.as_str().to_string(),
            })?;
        if question.source.session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationUnauthorized {
                action: "reply_question",
                identity: question.source.session_id,
            });
        }
        if question.target != input.responder {
            return Err(DomainError::CollaborationUnauthorized {
                action: "reply_question",
                identity: input.responder.session_id.clone(),
            });
        }
        let semantic = CollaborationOperationSemantic::ReplyQuestion {
            question_id: input.question_id.clone(),
            responder: input.responder.clone(),
            body: input.body.clone(),
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Settlement { question_id, state } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let question = self
                .collaboration_state()
                .questions
                .get(&question_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "question",
                    identity: question_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationSettlementReceipt {
                    question_id,
                    state,
                    answer_delivery: question
                        .answer
                        .as_ref()
                        .map(|answer| answer.delivery.clone()),
                    event_seq: existing.event_seq,
                },
            ));
        }
        if question.state != CollaborationQuestionState::Pending {
            return Err(DomainError::CollaborationAlreadySettled {
                question_id: input.question_id.as_str().to_string(),
                state: match question.state {
                    CollaborationQuestionState::Pending => "pending",
                    CollaborationQuestionState::Answered => "answered",
                    CollaborationQuestionState::Cancelled => "cancelled",
                },
            });
        }
        let answer = CollaborationAnswerRecord {
            operation_id: input.operation_id.clone(),
            responder: input.responder,
            body: input.body,
            settled_at_ms: input.answered_at_ms,
            delivery: CollaborationAnswerDeliveryRecord {
                state: CollaborationAnswerDeliveryState::Undelivered,
                delivery_revision: 0,
                kind: None,
                projected_seq: None,
                projected_input_id: None,
                tool_call_id: None,
                tool_result_seq: None,
                last_operation_id: input.operation_id.clone(),
                updated_at_ms: input.answered_at_ms,
            },
        };
        let mut question = question;
        question.state = CollaborationQuestionState::Answered;
        question.answer = Some(answer.clone());
        let delivery = self
            .collaboration_state()
            .outbound_deliveries
            .get(&question.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "delivery",
                identity: question.delivery_id.as_str().to_string(),
            })?;
        if delivery.state == CollaborationDeliveryState::Failed {
            // No target execution may answer a source question that never reached its target.
            // 未送达目标的问题不允许任何目标执行写入回答。
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.answer.failed_delivery",
            });
        }
        let mut state = self.collaboration_state().clone();
        state
            .questions
            .insert(input.question_id.clone(), question.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Settlement {
                    question_id: input.question_id.clone(),
                    state: CollaborationQuestionState::Answered,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::QuestionAnswered,
            CollaborationAfterState::Question {
                question: question.clone(),
                delivery,
            },
            input.answered_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationSettlementReceipt {
                question_id: question.question_id,
                state: question.state,
                answer_delivery: question.answer.map(|answer| answer.delivery),
                event_seq,
            },
        ))
    }

    /// Cancels one pending question by its authorized source participant.
    /// 由获授权的来源参与者撤销一个待处理问题。
    pub fn cancel_question(
        &mut self,
        input: CancelQuestionInput,
    ) -> Result<IdempotentMutation<CollaborationSettlementReceipt>, DomainError> {
        self.validate_source_participant(&input.requester, true)?;
        validate_reason("collaboration.cancellation.reason", &input.reason)?;
        validate_timestamp(
            "collaboration.cancellation.cancelled_at_ms",
            input.cancelled_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::CancelQuestion {
            question_id: input.question_id.clone(),
            requester: input.requester.clone(),
            reason: input.reason.clone(),
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::Settlement { question_id, state } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let question = self
                .collaboration_state()
                .questions
                .get(&question_id)
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "question",
                    identity: question_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationSettlementReceipt {
                    question_id,
                    state,
                    answer_delivery: question
                        .answer
                        .as_ref()
                        .map(|answer| answer.delivery.clone()),
                    event_seq: existing.event_seq,
                },
            ));
        }
        let current = self
            .collaboration_state()
            .questions
            .get(&input.question_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "question",
                identity: input.question_id.as_str().to_string(),
            })?;
        if current.source.session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationUnauthorized {
                action: "cancel_question",
                identity: current.source.session_id,
            });
        }
        if current.source != input.requester {
            return Err(DomainError::CollaborationUnauthorized {
                action: "cancel_question",
                identity: input.requester.session_id.clone(),
            });
        }
        if current.state != CollaborationQuestionState::Pending {
            return Err(DomainError::CollaborationAlreadySettled {
                question_id: input.question_id.as_str().to_string(),
                state: match current.state {
                    CollaborationQuestionState::Pending => "pending",
                    CollaborationQuestionState::Answered => "answered",
                    CollaborationQuestionState::Cancelled => "cancelled",
                },
            });
        }
        let mut question = current;
        question.state = CollaborationQuestionState::Cancelled;
        question.cancellation = Some(CollaborationCancellationRecord {
            operation_id: input.operation_id.clone(),
            requester: input.requester,
            reason: input.reason,
            cancelled_at_ms: input.cancelled_at_ms,
        });
        let delivery = self
            .collaboration_state()
            .outbound_deliveries
            .get(&question.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "delivery",
                identity: question.delivery_id.as_str().to_string(),
            })?;
        let mut state = self.collaboration_state().clone();
        state
            .questions
            .insert(input.question_id.clone(), question.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::Settlement {
                    question_id: input.question_id.clone(),
                    state: CollaborationQuestionState::Cancelled,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::QuestionCancelled,
            CollaborationAfterState::Question {
                question: question.clone(),
                delivery,
            },
            input.cancelled_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationSettlementReceipt {
                question_id: question.question_id,
                state: question.state,
                answer_delivery: None,
                event_seq,
            },
        ))
    }

    /// Delivers an already settled answer without changing question settlement.
    /// 在不改变问题结算状态的前提下交付已结算答案。
    pub fn deliver_answer(
        &mut self,
        input: DeliverAnswerInput,
    ) -> Result<IdempotentMutation<CollaborationAnswerDeliveryReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.answer_delivery.delivered_at_ms",
            input.delivered_at_ms,
        )?;
        match input.delivery_kind {
            CollaborationAnswerDeliveryKind::WaitTool
                if input.projected_seq.is_some()
                    || input.projected_input_id.is_some()
                    || input.tool_call_id.is_none()
                    || input.tool_result_seq.is_none() =>
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.answer_delivery.wait_tool_fact",
                });
            }
            CollaborationAnswerDeliveryKind::UserEvent
                if input.projected_seq.is_none()
                    || input.projected_input_id.is_none()
                    || input.tool_call_id.is_some()
                    || input.tool_result_seq.is_some() =>
            {
                return Err(DomainError::InvalidStatistics {
                    field: "collaboration.answer_delivery.user_event_fact",
                });
            }
            _ => {}
        }
        let semantic = CollaborationOperationSemantic::DeliverAnswer {
            question_id: input.question_id.clone(),
            delivery_kind: input.delivery_kind,
            projected_seq: input.projected_seq,
            projected_input_id: input.projected_input_id,
            tool_call_id: input.tool_call_id,
            tool_result_seq: input.tool_result_seq,
        };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::AnswerDelivery { question_id, .. } = existing.result
            else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            let delivery = self
                .collaboration_state()
                .questions
                .get(&question_id)
                .and_then(|question| question.answer.as_ref())
                .map(|answer| answer.delivery.clone())
                .ok_or_else(|| DomainError::CollaborationNotFound {
                    kind: "answer_delivery",
                    identity: question_id.as_str().to_string(),
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationAnswerDeliveryReceipt {
                    question_id,
                    delivery_revision: delivery.delivery_revision,
                    state: delivery.state,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let mut question = self
            .collaboration_state()
            .questions
            .get(&input.question_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "question",
                identity: input.question_id.as_str().to_string(),
            })?;
        if question.source.session_id != self.metadata.session_id {
            return Err(DomainError::CollaborationUnauthorized {
                action: "deliver_answer",
                identity: question.source.session_id,
            });
        }
        // A nonblocking question can outlive its source turn; delivery belongs to the current execution.
        // 非阻塞问题可以存续至来源回合结束之后；交付归属于当前执行。
        let source_turn_id = self
            .active_turn()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id);
        match input.delivery_kind {
            CollaborationAnswerDeliveryKind::UserEvent => {
                let projected_seq = input.projected_seq.expect("validated user event seq");
                // Correlate the exact host-owned occurrence, not any User event in this turn.
                // 关联精确宿主发生点，而不是当前回合中的任意 User 事件。
                let expected_action =
                    format!("agent-message:{}:answer", question.delivery_id.as_str());
                let projected_input_id = input
                    .projected_input_id
                    .as_ref()
                    .expect("validated user event input id");
                let valid = self.events.iter().any(|event| {
                    event.seq == projected_seq
                        && event.visibility == EventVisibility::Provider
                        && event.turn_id == source_turn_id
                        && matches!(
                            &event.event,
                            EventBody::UserInput(user_input)
                                if &user_input.input_id == projected_input_id
                                    && user_input.input_source == crate::v1::event::UserInputSource::AgentMessage
                                    && user_input.client_action_id.as_str() == expected_action
                        )
                });
                if !valid {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.answer_delivery.user_event_fact",
                    });
                }
            }
            CollaborationAnswerDeliveryKind::WaitTool => {
                let tool_result_seq = input.tool_result_seq.expect("validated tool result seq");
                let tool_call_id = input.tool_call_id.as_ref().expect("validated tool call id");
                let valid = self.events.iter().any(|event| {
                    event.seq == tool_result_seq
                        && event.visibility == EventVisibility::Provider
                        && event.turn_id == source_turn_id
                        && matches!(
                            &event.event,
                            EventBody::ToolCall(tool)
                                if &tool.tool_call_id == tool_call_id
                                    && wait_result::proves_wait_answer(tool, &question)
                        )
                });
                if !valid {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.answer_delivery.tool_result_fact",
                    });
                }
            }
        }
        let answer =
            question
                .answer
                .as_mut()
                .ok_or_else(|| DomainError::CollaborationStateViolation {
                    field: "collaboration.answer_delivery.answer_missing",
                })?;
        if question.state != CollaborationQuestionState::Answered {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.answer_delivery.question_state",
            });
        }
        if answer.delivery.state == CollaborationAnswerDeliveryState::Delivered {
            return Err(DomainError::CollaborationAlreadySettled {
                question_id: input.question_id.as_str().to_string(),
                state: "answer_delivered",
            });
        }
        answer.delivery.state = CollaborationAnswerDeliveryState::Delivered;
        answer.delivery.delivery_revision = answer
            .delivery
            .delivery_revision
            .checked_add(1)
            .ok_or_else(|| DomainError::invalid_id("answer_delivery_revision", u64::MAX))?;
        answer.delivery.kind = Some(input.delivery_kind);
        answer.delivery.projected_seq = input.projected_seq;
        answer.delivery.projected_input_id = input.projected_input_id;
        answer.delivery.tool_call_id = input.tool_call_id;
        answer.delivery.tool_result_seq = input.tool_result_seq;
        answer.delivery.last_operation_id = input.operation_id.clone();
        answer.delivery.updated_at_ms = input.delivered_at_ms;
        let delivery = answer.delivery.clone();
        let revision = delivery.delivery_revision;
        let mut state = self.collaboration_state().clone();
        state.questions.insert(input.question_id.clone(), question);
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::AnswerDelivery {
                    question_id: input.question_id.clone(),
                    delivery_revision: revision,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::AnswerDelivered,
            CollaborationAfterState::AnswerDelivery {
                question_id: input.question_id.clone(),
                delivery: delivery.clone(),
            },
            input.delivered_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationAnswerDeliveryReceipt {
                question_id: input.question_id,
                delivery_revision: revision,
                state: CollaborationAnswerDeliveryState::Delivered,
                event_seq,
            },
        ))
    }

    /// Returns one source-owned question snapshot.
    /// 返回一个来源拥有的问题快照。
    pub fn question_snapshot(
        &self,
        question_id: &CollaborationQuestionId,
    ) -> Option<&CollaborationQuestionRecord> {
        self.collaboration_state().questions.get(question_id)
    }

    /// Returns one source-owned delivery snapshot.
    /// 返回一个来源拥有的投递快照。
    pub fn delivery_snapshot(
        &self,
        delivery_id: &CollaborationDeliveryId,
    ) -> Option<&CollaborationDeliveryRecord> {
        self.collaboration_state()
            .outbound_deliveries
            .get(delivery_id)
    }

    /// Returns one target-owned delivery snapshot.
    /// 返回一个目标拥有的投递快照。
    pub fn inbound_delivery_snapshot(
        &self,
        delivery_id: &CollaborationDeliveryId,
    ) -> Option<&CollaborationInboundDeliveryRecord> {
        self.collaboration_state()
            .inbound_deliveries
            .get(delivery_id)
    }

    /// Returns the current explicit run-control state.
    /// 返回当前明确的运行控制状态。
    pub fn collaboration_run_control(&self) -> CollaborationRunControl {
        self.collaboration_state().run_control
    }

    /// Returns whether this turn still has a completion-blocking child-task obligation.
    /// 判断指定回合是否仍有会阻止完成的子任务义务。
    pub fn has_unsettled_collaboration(&self, turn_id: Option<TurnId>) -> bool {
        if let Some(turn_id) = turn_id {
            return !self
                .completion_obligations(turn_id)
                .open_child_task_ids
                .is_empty();
        }
        self.collaboration_state().child_tasks.values().any(|task| {
            matches!(
                task.state,
                CollaborationChildTaskState::Reserved | CollaborationChildTaskState::Active
            )
        })
    }

    /// Returns whether edit or delete must preserve unresolved collaboration facts.
    /// 判断编辑或删除是否必须保留未解决的协作事实。
    ///
    /// # Returns
    /// 返回值
    /// Delegates to the single state-level definition so a warm document and a cold member read
    /// through its locator can never disagree about what must be settled.
    /// 委托给唯一的状态级定义，使驻留文档与经定位器读取的冷态成员不会对"必须结清什么"产生分歧。
    pub fn has_unsettled_collaboration_for_edit_or_delete(&self) -> bool {
        self.collaboration_state()
            .requires_edit_or_delete_settlement()
    }

    /// Computes all bounded completion obligations associated with one parent turn.
    /// 计算与一个父回合关联的全部有界完成义务。
    pub fn completion_obligations(&self, turn_id: TurnId) -> CollaborationCompletionObligations {
        let mut obligations = CollaborationCompletionObligations {
            turn_id,
            outgoing_question_ids: Vec::new(),
            incoming_question_ids: Vec::new(),
            undelivered_answer_question_ids: Vec::new(),
            unacknowledged_settlement_question_ids: Vec::new(),
            open_child_task_ids: Vec::new(),
            pending_delivery_ids: Vec::new(),
        };
        for task in self.collaboration_state().child_tasks.values() {
            if task.parent_turn_id == turn_id
                && matches!(
                    task.state,
                    CollaborationChildTaskState::Reserved | CollaborationChildTaskState::Active
                )
            {
                obligations.open_child_task_ids.push(task.task_id.clone());
            }
        }
        obligations
    }
}

impl SessionDocument {
    /// Begins a dedicated question execution through the auxiliary-turn implementation.
    /// 通过辅助回合实现开始专用问题执行。
    pub fn begin_question_execution(
        &mut self,
        input: BeginQuestionExecutionInput,
    ) -> Result<IdempotentMutation<CollaborationQuestionExecutionReceipt>, DomainError> {
        question_execution::begin_question_execution(self, input)
    }

    /// Completes a dedicated question execution through the auxiliary-turn implementation.
    /// 通过辅助回合实现完成专用问题执行。
    pub fn complete_question_execution(
        &mut self,
        input: CompleteQuestionExecutionInput,
    ) -> Result<IdempotentMutation<CollaborationQuestionExecutionReceipt>, DomainError> {
        question_execution::complete_question_execution(self, input)
    }

    /// Records one explicit run-control transition.
    /// 记录一条明确的运行控制转移。
    pub fn set_collaboration_run_control(
        &mut self,
        input: SetRunControlInput,
    ) -> Result<IdempotentMutation<CollaborationRunControlReceipt>, DomainError> {
        validate_timestamp(
            "collaboration.run_control.changed_at_ms",
            input.changed_at_ms,
        )?;
        let semantic = CollaborationOperationSemantic::SetRunControl { state: input.state };
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            let CollaborationOperationResult::RunControl { state } = existing.result else {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.operation.result",
                });
            };
            return Ok(IdempotentMutation::Idempotent(
                CollaborationRunControlReceipt {
                    state,
                    event_seq: existing.event_seq,
                },
            ));
        }
        let transition = CollaborationRunControlRecord {
            operation_id: input.operation_id.clone(),
            state: input.state,
            changed_at_ms: input.changed_at_ms,
        };
        let mut state = self.collaboration_state().clone();
        state.run_control = input.state;
        state.run_control_history.push(transition.clone());
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::RunControl { state: input.state },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::RunControlChanged,
            CollaborationAfterState::RunControl { transition },
            input.changed_at_ms,
        )?;
        Ok(IdempotentMutation::Committed(
            CollaborationRunControlReceipt {
                state: input.state,
                event_seq,
            },
        ))
    }

    /// Clears executable collaboration state for a new generation boundary.
    /// 为新代次边界清除可执行协作状态。
    pub fn clear_executable_collaboration_ownership(
        &mut self,
        input: ClearGenerationOwnershipInput,
    ) -> Result<IdempotentMutation<Seq>, DomainError> {
        validate_timestamp("collaboration.generation_reset_at_ms", input.reset_at_ms)?;
        let semantic = CollaborationOperationSemantic::ClearGenerationOwnership;
        if let Some(existing) =
            self.existing_collaboration_operation(&input.operation_id, &semantic)?
        {
            return Ok(IdempotentMutation::Idempotent(existing.event_seq));
        }
        // Cleared operations remain readable history and must not be validated as live owners on reload.
        // 清除的操作仍是可读历史，重载时不能再按活动所有者校验。
        let history_boundary = self
            .metadata
            .collaboration
            .last_event_seq
            .max(self.metadata.collaboration.history_boundary_seq);
        let mut state = self.collaboration_state().clone();
        state.questions.clear();
        state.outbound_deliveries.clear();
        state.inbound_deliveries.clear();
        state.notifications.clear();
        state.inbound_notifications.clear();
        state.outbound_task_messages.clear();
        state.inbound_task_messages.clear();
        state.task_message_effects.clear();
        state.task_results.clear();
        state.received_task_results.clear();
        state.task_result_acknowledgements.clear();
        state.task_result_projections.clear();
        state.child_tasks.clear();
        state.question_executions.clear();
        state.grants.clear();
        state.run_control = CollaborationRunControl::Running;
        state.run_control_history.clear();
        state.operations.clear();
        let event_seq = next_event_seq(self)?;
        state.operations.insert(
            input.operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: input.operation_id.clone(),
                semantic,
                event_seq,
                result: CollaborationOperationResult::RunControl {
                    state: CollaborationRunControl::Running,
                },
            },
        );
        let event_seq = self.commit_collaboration_event(
            state,
            input.operation_id,
            CollaborationEventKind::GenerationOwnershipCleared,
            CollaborationAfterState::GenerationOwnershipCleared,
            input.reset_at_ms,
        )?;
        // Clear creation sources in the same revision that removes their reservation owners.
        // 在移除预留所有者的同一修订中清除创建来源。
        self.metadata.child_creation_intents.clear();
        self.metadata.collaboration.history_boundary_seq = history_boundary;
        Ok(IdempotentMutation::Committed(event_seq))
    }
}
