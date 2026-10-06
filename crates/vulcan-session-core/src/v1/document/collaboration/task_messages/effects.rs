//! Atomic application of steering and explicit failure of accepted task messages.
//! 已接纳任务消息方向应用及明确失败的原子处理。

use super::*;
use crate::v1::ClientActionId;
use vulcan_ai_contract::host_message::{HostMessageEnvelope, HostMessageKind};

/// Builds the deterministic task envelope for the exact persisted `message`.
/// 为精确持久化 `message` 构建确定的任务信封。
/// Returns representation only; source, target and turn have already been frozen by admission.
/// 仅返回表示形式；来源、目标和回合已经由准入冻结。
pub(super) fn task_message_envelope(message: &CollaborationTaskMessage) -> HostMessageEnvelope {
    HostMessageEnvelope {
        event_id: message.delivery_id.as_str().to_owned(),
        kind: match message.kind {
            CollaborationTaskMessageKind::TaskStart => HostMessageKind::TaskStart,
            CollaborationTaskMessageKind::TaskSteer => HostMessageKind::TaskSteer,
        },
        source: message.source.session_id.clone(),
        target: message.target.session_id.clone(),
        task_id: message
            .target
            .task_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        question_id: None,
        scope: String::from("turn"),
        body: message.body.clone(),
    }
}

impl SessionDocument {
    /// Fails accepted steering that never reached a provider request before its bound turn closes.
    /// 原绑定回合闭合前从未进入供应商请求的已接纳方向在此失败。
    /// `turn_id` is the closing business turn and `reason` describes its terminal boundary;
    /// returns after every matching failure effect is staged with that same closure.
    /// `turn_id` 是即将闭合的业务回合，`reason` 描述其终态边界；所有匹配的失败效果与闭合一起暂存。
    pub(in crate::v1::document) fn fail_unapplied_task_steering_for_turn(
        &mut self,
        turn_id: TurnId,
        reason: CollaborationDeliveryFailureReason,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        let pending = self
            .collaboration_state()
            .inbound_task_messages
            .iter()
            .filter(|(delivery_id, acceptance)| {
                acceptance.message.kind == CollaborationTaskMessageKind::TaskSteer
                    && acceptance.message.target.turn_id == Some(turn_id)
                    && !self
                        .collaboration_state()
                        .task_message_effects
                        .contains_key(*delivery_id)
            })
            .map(|(delivery_id, _)| delivery_id.clone())
            .collect::<Vec<_>>();
        for delivery_id in pending {
            let operation_id = CollaborationOperationId::try_new(format!(
                "{}:task-message-effect",
                delivery_id.as_str()
            ))?;
            self.fail_task_message(operation_id, delivery_id, reason, now_ms)?;
        }
        Ok(())
    }

    /// Appends accepted `delivery_id` as tagged User guidance under `operation_id` at `now_ms`.
    /// 在 `now_ms` 使用 `operation_id` 将已接纳 `delivery_id` 追加为带标签 User 方向。
    /// Returns the exact effect receipt; input and receipt publish in one mutation revision.
    /// 返回精确效果回执；输入与回执在一次修改修订中发布。
    pub fn apply_task_steering_before_api(
        &mut self,
        operation_id: CollaborationOperationId,
        delivery_id: CollaborationDeliveryId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskMessageEffect>, DomainError> {
        let acceptance = self.task_message_acceptance_for_effect(&delivery_id)?;
        let semantic = CollaborationOperationSemantic::ApplyTaskSteering {
            delivery_id: delivery_id.clone(),
        };
        if let Some(receipt) = self.replay_task_message_effect(&operation_id, &semantic)? {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        if let Some(effect) = self
            .collaboration_state()
            .task_message_effects
            .get(&delivery_id)
        {
            if matches!(
                effect.outcome,
                CollaborationTaskMessageEffectKind::Steered { .. }
            ) {
                return Ok(IdempotentMutation::Idempotent(effect.clone()));
            }
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.already_failed",
            });
        }
        if acceptance.message.kind != CollaborationTaskMessageKind::TaskSteer
            || self.active_turn().is_none_or(|turn| {
                turn.closed
                    || Some(turn.turn_id) != acceptance.message.target.turn_id
                    || turn.origin == crate::v1::document::TurnOrigin::QuestionExecution
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.steering_turn",
            });
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
                field: "collaboration.task_message.steering_stopped",
            });
        }
        // The draft owns both edits: a failed receipt must never leave an untracked User message.
        // 草稿持有两个修改：回执失败绝不能留下未追踪 User 消息。
        let mut staged = self.begin_atomic_mutation()?;
        let (input_seq, input_id, turn_id) = super::super::projection::append_envelope(
            &mut staged,
            task_message_envelope(&acceptance.message),
            now_ms,
        )?;
        let effect = staged.commit_task_message_effect(
            operation_id,
            delivery_id,
            semantic,
            CollaborationTaskMessageEffectKind::Steered {
                turn_id,
                input_id,
                input_seq,
            },
            now_ms,
        )?;
        staged.commit();
        Ok(IdempotentMutation::Committed(effect))
    }

    /// Fails accepted `delivery_id` with `reason` under `operation_id`, including after target stop.
    /// 使用 `operation_id` 与 `reason` 使已接纳 `delivery_id` 失败，包括目标停止之后。
    /// Returns the winning failure receipt; applied steering cannot be rewritten as failure.
    /// 返回胜出失败回执；已应用方向不能改写为失败。
    pub fn fail_task_message(
        &mut self,
        operation_id: CollaborationOperationId,
        delivery_id: CollaborationDeliveryId,
        reason: CollaborationDeliveryFailureReason,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskMessageEffect>, DomainError> {
        self.task_message_acceptance_for_effect(&delivery_id)?;
        let semantic = CollaborationOperationSemantic::FailTaskMessage {
            delivery_id: delivery_id.clone(),
            reason,
        };
        if let Some(receipt) = self.replay_task_message_effect(&operation_id, &semantic)? {
            return Ok(IdempotentMutation::Idempotent(receipt));
        }
        if let Some(effect) = self
            .collaboration_state()
            .task_message_effects
            .get(&delivery_id)
        {
            if effect.outcome == (CollaborationTaskMessageEffectKind::Failed { reason }) {
                return Ok(IdempotentMutation::Idempotent(effect.clone()));
            }
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.effect_conflict",
            });
        }
        self.commit_task_message_effect(
            operation_id,
            delivery_id,
            semantic,
            CollaborationTaskMessageEffectKind::Failed { reason },
            now_ms,
        )
        .map(IdempotentMutation::Committed)
    }

    /// Resolves and validates the local acceptance for `delivery_id` without changing it.
    /// 解析并校验 `delivery_id` 的本地接纳，不改变记录。
    pub(super) fn task_message_acceptance_for_effect(
        &self,
        delivery_id: &CollaborationDeliveryId,
    ) -> Result<CollaborationTaskMessageAcceptance, DomainError> {
        let acceptance = self
            .collaboration_state()
            .inbound_task_messages
            .get(delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "inbound_task_message",
                identity: delivery_id.as_str().to_owned(),
            })?;
        self.validate_target_participant(&acceptance.message.target)?;
        Ok(acceptance)
    }

    /// Returns an original effect only when `operation_id` and `semantic` exactly match.
    /// 仅在 `operation_id` 与 `semantic` 精确匹配时返回原效果。
    pub(super) fn replay_task_message_effect(
        &self,
        operation_id: &CollaborationOperationId,
        semantic: &CollaborationOperationSemantic,
    ) -> Result<Option<CollaborationTaskMessageEffect>, DomainError> {
        let Some(previous) = self.existing_collaboration_operation(operation_id, semantic)? else {
            return Ok(None);
        };
        let CollaborationOperationResult::TaskMessageEffect { effect } = previous.result else {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.effect_result",
            });
        };
        Ok(Some(effect))
    }

    /// Commits validated `outcome` and its operation result at `now_ms`, returning exact coordinates.
    /// 在 `now_ms` 提交已校验 `outcome` 及其操作结果，返回精确坐标。
    pub(super) fn commit_task_message_effect(
        &mut self,
        operation_id: CollaborationOperationId,
        delivery_id: CollaborationDeliveryId,
        semantic: CollaborationOperationSemantic,
        outcome: CollaborationTaskMessageEffectKind,
        now_ms: i64,
    ) -> Result<CollaborationTaskMessageEffect, DomainError> {
        let acceptance = self.task_message_acceptance_for_effect(&delivery_id)?;
        let effect = CollaborationTaskMessageEffect {
            delivery_id: delivery_id.clone(),
            operation_id: operation_id.clone(),
            outcome,
            target_revision: next_revision(self)?,
            event_seq: next_event_seq(self)?,
            occurred_at_ms: now_ms,
        };
        effect.validate(&acceptance)?;
        let mut state = self.collaboration_state().clone();
        state
            .task_message_effects
            .insert(delivery_id, effect.clone());
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: effect.event_seq,
                result: CollaborationOperationResult::TaskMessageEffect {
                    effect: effect.clone(),
                },
            },
        );
        self.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskMessageEffect,
            CollaborationAfterState::TaskMessageEffect {
                effect: effect.clone(),
            },
            now_ms,
        )?;
        Ok(effect)
    }

    /// Validates resident task input identity, claim anchor and body against durable effects on reload.
    /// 重载时根据可靠效果校验驻留任务输入身份、认领锚点与正文。
    /// Sealed inputs retain their typed metadata proof and are checked when their segment is loaded.
    /// 封存输入保留类型化元数据凭据，在加载对应分段时校验。
    pub(in crate::v1::document) fn validate_task_message_effect_projections(
        &self,
    ) -> Result<(), DomainError> {
        for effect in self.collaboration_state().task_message_effects.values() {
            let (turn_id, input_id, input_seq) = match &effect.outcome {
                CollaborationTaskMessageEffectKind::Steered {
                    turn_id,
                    input_id,
                    input_seq,
                }
                | CollaborationTaskMessageEffectKind::Claimed {
                    turn_id,
                    input_id,
                    input_seq,
                    ..
                } => (turn_id, input_id, input_seq),
                CollaborationTaskMessageEffectKind::Failed { .. } => continue,
            };
            if matches!(
                &effect.outcome,
                CollaborationTaskMessageEffectKind::Claimed { .. }
            ) && self
                .events
                .first()
                .is_some_and(|event| event.seq <= effect.event_seq)
            {
                let anchor = self
                    .events
                    .iter()
                    .find(|event| event.seq == effect.event_seq);
                if anchor.is_none_or(|event| event.turn_id != Some(*turn_id)
                    || event.visibility != EventVisibility::HostOnly
                    || !matches!(&event.event, EventBody::Collaboration(collaboration)
                        if matches!(&collaboration.after_state, CollaborationAfterState::TaskMessageEffect { effect: recorded } if recorded == effect)))
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.task_message.claim_anchor",
                    });
                }
            }
            // A resident range must contain its claimed input; a sealed range is not scanned here.
            // 驻留范围必须包含声称的输入；这里不扫描封存范围。
            if self
                .events
                .first()
                .is_none_or(|event| event.seq > *input_seq)
            {
                continue;
            }
            let acceptance = self.task_message_acceptance_for_effect(&effect.delivery_id)?;
            let expected_action =
                ClientActionId::try_new(format!("agent-message:{}", effect.delivery_id.as_str()))?;
            let expected_text = task_message_envelope(&acceptance.message)
                .render()
                .map_err(|_| DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.steering_envelope",
                })?;
            let matched = self.events.iter().any(|event| event.seq == *input_seq && event.turn_id == Some(*turn_id)
                && event.visibility == EventVisibility::Provider
                && matches!(&event.event, EventBody::UserInput(input) if input.input_id == *input_id
                    && input.input_source == crate::v1::event::UserInputSource::AgentMessage
                    && input.client_action_id == expected_action && input.text == expected_text));
            if !matched {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.steering_input",
                });
            }
        }
        Ok(())
    }
}
