//! Atomic collaboration projection at a closed physical API boundary.
//! 物理 API 闭合边界上的原子协作投影。

use super::*;
use crate::v1::ids::{ClientActionId, PendingInputId};
use vulcan_ai_contract::{HostMessageEnvelope, HostMessageKind};

/// Appends one authenticated envelope and returns its exact provider-input proof.
/// 追加一条已认证信封，并返回其精确 Provider 输入凭据。
/// The caller stages a document clone so any later proof failure leaves the live document intact.
/// 调用方暂存文档副本，保证后续凭据失败不改变当前文档。
pub(super) fn append_envelope(
    document: &mut SessionDocument,
    envelope: HostMessageEnvelope,
    now_ms: i64,
) -> Result<(Seq, PendingInputId, TurnId), DomainError> {
    // Rendering is a pure representation boundary; IDs come exclusively from durable records.
    // 渲染是纯表示边界；所有身份仅来自持久记录。
    let action = ClientActionId::try_new(format!("agent-message:{}", envelope.event_id))?;
    let text = envelope
        .render()
        .map_err(|_| DomainError::CollaborationStateViolation {
            field: "collaboration.projection.envelope",
        })?;
    let seq = document
        .append_in_turn_guidance_before_api(action, text, now_ms)?
        .into_value();
    let event = document
        .events
        .iter()
        .find(|event| event.seq == seq)
        .ok_or(DomainError::ReferenceViolation {
            kind: "agent_message.seq",
            value: seq.value(),
        })?;
    let EventBody::UserInput(input) = &event.event else {
        return Err(DomainError::CollaborationStateViolation {
            field: "collaboration.projection.user_input",
        });
    };
    let turn_id = event.turn_id.ok_or(DomainError::NoActiveTurn)?;
    Ok((seq, input.input_id, turn_id))
}

impl SessionDocument {
    /// Starts or resumes a question execution and appends its input in one publication revision.
    /// 在同一次发布修订中启动或恢复答疑执行，并追加问题输入。
    /// `question` is the authoritative source snapshot; `now_ms` timestamps newly committed facts.
    /// `question` 是来源权威快照；`now_ms` 为新提交事实提供时间。
    /// Returns the running execution receipt, or an error without changing the document.
    /// 返回运行中执行回执；出错时文档保持不变。
    /// The service must separately retain admission and prove any MainOnly child authorization.
    /// 服务必须另外持有准入预留，并证明 MainOnly 的子代理授权。
    pub fn prepare_question_execution_before_api(
        &mut self,
        question: CollaborationQuestionRecord,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationQuestionExecutionReceipt>, DomainError> {
        // A historical begin receipt never authorizes a fresh launch after stop or settlement.
        // 历史启动回执不能授权停止或结算之后的新启动。
        if question.state != CollaborationQuestionState::Pending
            || !matches!(
                (self.collaboration_run_control(), question.execution_purpose),
                (
                    CollaborationRunControl::Running | CollaborationRunControl::MainOnly,
                    CollaborationExecutionPurpose::InTurnQuestion
                ) | (
                    CollaborationRunControl::NaturallyIdle,
                    CollaborationExecutionPurpose::NaturallyIdleQuestion
                )
            )
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.question_preparation.admission",
            });
        }
        if self.active_api().is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.question_preparation.active_api",
            });
        }
        // Read the exact delivery rather than selecting another record for the same participant.
        // 读取精确投递，不能选用相同参与者的另一条记录。
        let inbound = self
            .inbound_delivery_snapshot(&question.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_delivery",
                identity: question.delivery_id.as_str().to_owned(),
            })?;
        // Nested projection inherits this revision; any later envelope error rolls back the start.
        // 嵌套投影继承此修订；后续信封错误会回滚启动。
        let mut staged = self.begin_atomic_mutation()?;
        let started = staged.begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "{}:begin",
                question.delivery_id.as_str()
            ))?,
            execution_id: CollaborationExecutionId::try_new(format!(
                "{}:execution",
                question.delivery_id.as_str()
            ))?,
            question_id: question.question_id.clone(),
            participant: question.target.clone(),
            purpose: question.execution_purpose,
            parent_turn_id: inbound.bound_turn_id,
            started_at_ms: now_ms,
        })?;
        // Begin replays its original receipt, so live ownership must be read separately.
        // 启动会重放原始回执，因此必须单独读取当前所有权。
        let started_changed = matches!(&started, IdempotentMutation::Committed(_));
        let mut receipt = started.into_value();
        let execution = staged
            .collaboration_state()
            .question_executions
            .get(&receipt.execution.execution_id)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.question_preparation.execution",
            })?;
        if execution.state != CollaborationQuestionExecutionState::Running
            || !staged.active_turn().is_some_and(|turn| {
                !turn.closed
                    && match execution.purpose {
                        CollaborationExecutionPurpose::InTurnQuestion => {
                            execution.parent_turn_id == Some(turn.turn_id)
                        }
                        CollaborationExecutionPurpose::NaturallyIdleQuestion => {
                            execution.execution_turn_id == Some(turn.turn_id)
                        }
                        CollaborationExecutionPurpose::NormalTask => false,
                    }
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.question_preparation.running_owner",
            });
        }
        receipt.execution = execution.clone();
        let projected = staged.project_question_before_api(question, now_ms)?;
        if started_changed || matches!(projected, IdempotentMutation::Committed(_)) {
            self.commit_atomic_mutation(staged);
            Ok(IdempotentMutation::Committed(receipt))
        } else {
            Ok(IdempotentMutation::Idempotent(receipt))
        }
    }

    /// Ends in-turn question attempts when their business turn ends, without settling unanswered questions.
    /// 业务回合结束时结束其回合内答疑尝试，不结算未回答问题。
    /// The enclosing staged turn mutation owns rollback if any later closure step fails.
    /// 外层暂存回合修改负责在后续闭合步骤失败时回滚。
    pub(in crate::v1::document) fn finish_in_turn_question_executions(
        &mut self,
        turn_id: TurnId,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        let attempts = self
            .collaboration_state()
            .question_executions
            .values()
            .filter(|execution| {
                execution.purpose == CollaborationExecutionPurpose::InTurnQuestion
                    && execution.parent_turn_id == Some(turn_id)
                    && execution.state == CollaborationQuestionExecutionState::Running
            })
            .cloned()
            .collect::<Vec<_>>();
        for execution in attempts {
            let settled = self
                .collaboration_state()
                .inbound_deliveries
                .values()
                .any(|delivery| {
                    delivery.question_id == execution.question_id && delivery.settlement.is_some()
                });
            self.complete_question_execution(CompleteQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "{}:turn-ended",
                    execution.execution_id.as_str()
                ))?,
                execution_id: execution.execution_id,
                outcome: if settled {
                    CollaborationQuestionExecutionOutcome::Settled
                } else {
                    CollaborationQuestionExecutionOutcome::Unanswered
                },
                completed_at_ms: now_ms,
            })?;
        }
        Ok(())
    }

    /// Delivers a late answer in the current execution without reopening its original source turn.
    /// 在当前执行中交付晚到答案，不重新打开其原始来源回合。
    /// Returns the delivery receipt; a previously delivered answer creates no second input.
    /// 返回交付回执；已交付答案不产生第二条输入。
    pub fn project_answer_before_api(
        &mut self,
        question_id: CollaborationQuestionId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationAnswerDeliveryReceipt>, DomainError> {
        let question = self
            .question_snapshot(&question_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "question",
                identity: question_id.as_str().to_owned(),
            })?;
        self.validate_source_participant(&question.source, false)?;
        let answer = question
            .answer
            .as_ref()
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.answer_projection.missing_answer",
            })?;
        if answer.delivery.state == CollaborationAnswerDeliveryState::Delivered {
            let operation = self
                .collaboration_state()
                .operations
                .get(&answer.delivery.last_operation_id)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.answer_projection.receipt",
                })?;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationAnswerDeliveryReceipt {
                    question_id,
                    delivery_revision: answer.delivery.delivery_revision,
                    state: answer.delivery.state,
                    event_seq: operation.event_seq,
                },
            ));
        }
        let mut staged = self.begin_atomic_mutation()?;
        let event_id = format!("{}:answer", question.delivery_id.as_str());
        let envelope = HostMessageEnvelope {
            event_id: event_id.clone(),
            kind: HostMessageKind::QuestionAnswer,
            source: answer.responder.session_id.clone(),
            target: question.source.session_id.clone(),
            task_id: question
                .source
                .task_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            question_id: Some(question_id.as_str().to_owned()),
            scope: String::from("question"),
            body: answer.body.clone(),
        };
        let (projected_seq, projected_input_id, _) =
            append_envelope(&mut staged, envelope, now_ms)?;
        let receipt = staged.deliver_answer(DeliverAnswerInput {
            operation_id: CollaborationOperationId::try_new(format!("{event_id}:project"))?,
            question_id,
            delivery_kind: CollaborationAnswerDeliveryKind::UserEvent,
            projected_seq: Some(projected_seq),
            projected_input_id: Some(projected_input_id),
            tool_call_id: None,
            tool_result_seq: None,
            delivered_at_ms: now_ms,
        })?;
        self.commit_atomic_mutation(staged);
        Ok(receipt)
    }

    /// Projects a source-verified pending question and its execution binding as one atomic mutation.
    /// 将经来源验证的待处理问题及其执行绑定作为一次原子修改投影。
    /// Returns the durable target receipt; exact projected retries append nothing.
    /// 返回目标持久回执；已经投影的精确重试不追加内容。
    pub fn project_question_before_api(
        &mut self,
        question: CollaborationQuestionRecord,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationDeliveryReceipt>, DomainError> {
        let inbound = self
            .collaboration_state()
            .inbound_deliveries
            .get(&question.delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_delivery",
                identity: question.delivery_id.as_str().to_owned(),
            })?;
        if inbound.question_id != question.question_id
            || inbound.source != question.source
            || inbound.target != question.target
            || inbound.execution_purpose != question.execution_purpose
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.projection.source_binding",
            });
        }
        if inbound.state == CollaborationDeliveryState::Projected {
            let event_seq = self
                .collaboration_state()
                .operations
                .get(&CollaborationOperationId::try_new(format!(
                    "{}:project",
                    inbound.delivery_id.as_str()
                ))?)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.projection.receipt",
                })?
                .event_seq;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationDeliveryReceipt {
                    delivery_id: inbound.delivery_id,
                    question_id: inbound.question_id,
                    state: inbound.state,
                    revision: inbound.target_revision,
                    event_seq,
                },
            ));
        }
        if inbound.state != CollaborationDeliveryState::Accepted
            || question.state != CollaborationQuestionState::Pending
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.projection.pending_question",
            });
        }
        self.validate_target_participant(&inbound.target)?;
        let turn_id = self.require_open_turn()?.turn_id;
        let mut staged = self.begin_atomic_mutation()?;
        if inbound.execution_purpose == CollaborationExecutionPurpose::InTurnQuestion {
            staged.begin_question_execution(BeginQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "{}:begin",
                    inbound.delivery_id.as_str()
                ))?,
                execution_id: CollaborationExecutionId::try_new(format!(
                    "{}:execution",
                    inbound.delivery_id.as_str()
                ))?,
                question_id: inbound.question_id.clone(),
                participant: inbound.target.clone(),
                purpose: inbound.execution_purpose,
                parent_turn_id: inbound.bound_turn_id,
                started_at_ms: now_ms,
            })?;
        } else if !staged
            .collaboration_state()
            .question_executions
            .values()
            .any(|execution| {
                execution.question_id == inbound.question_id
                    && execution.state == CollaborationQuestionExecutionState::Running
                    && execution.execution_turn_id == Some(turn_id)
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.projection.idle_execution_owner",
            });
        }
        let envelope = HostMessageEnvelope {
            event_id: inbound.delivery_id.as_str().to_owned(),
            kind: HostMessageKind::Question,
            source: inbound.source.session_id.clone(),
            target: inbound.target.session_id.clone(),
            task_id: inbound
                .target
                .task_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            question_id: Some(inbound.question_id.as_str().to_owned()),
            scope: String::from("question"),
            body: question.body,
        };
        let (projected_seq, projected_input_id, projected_turn_id) =
            append_envelope(&mut staged, envelope, now_ms)?;
        let receipt = staged.mark_delivery_projected(ProjectDeliveryInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "{}:project",
                inbound.delivery_id.as_str()
            ))?,
            delivery_id: inbound.delivery_id,
            question_id: inbound.question_id,
            projected_seq,
            projected_input_id,
            projected_turn_id,
            projected_at_ms: now_ms,
        })?;
        self.commit_atomic_mutation(staged);
        Ok(receipt)
    }

    /// Projects an accepted notification without creating a task or question execution.
    /// 投影已接纳通知，不创建任务或问题执行。
    /// Returns the exact target receipt and preserves all previously emitted history.
    /// 返回精确目标回执，并保留此前全部已发出历史。
    pub fn project_notification_before_api(
        &mut self,
        notification_id: CollaborationNotificationId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationInboundNotificationReceipt>, DomainError> {
        let notification = self
            .collaboration_state()
            .inbound_notifications
            .get(&notification_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_notification",
                identity: notification_id.as_str().to_owned(),
            })?;
        self.validate_target_participant(&notification.target)?;
        let operation_id = CollaborationOperationId::try_new(format!(
            "{}:project",
            notification.delivery_id.as_str()
        ))?;
        if notification.state == CollaborationNotificationState::Projected {
            let event_seq = self
                .collaboration_state()
                .operations
                .get(&operation_id)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.notification.projection_receipt",
                })?
                .event_seq;
            return Ok(IdempotentMutation::Idempotent(
                CollaborationInboundNotificationReceipt {
                    notification,
                    event_seq,
                },
            ));
        }
        if notification.state != CollaborationNotificationState::Accepted {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.notification.projection_state",
            });
        }
        let mut staged = self.begin_atomic_mutation()?;
        let envelope = HostMessageEnvelope {
            event_id: notification.delivery_id.as_str().to_owned(),
            kind: HostMessageKind::AgentNotification,
            source: notification.source.session_id.clone(),
            target: notification.target.session_id.clone(),
            task_id: notification
                .target
                .task_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            question_id: None,
            scope: String::from("session"),
            body: notification.body,
        };
        let (projected_seq, projected_input_id, projected_turn_id) =
            append_envelope(&mut staged, envelope, now_ms)?;
        let receipt = staged.mark_notification_projected(ProjectNotificationInput {
            operation_id,
            notification_id,
            delivery_id: notification.delivery_id,
            projected_seq,
            projected_input_id,
            projected_turn_id,
            projected_at_ms: now_ms,
        })?;
        self.commit_atomic_mutation(staged);
        Ok(receipt)
    }
}
