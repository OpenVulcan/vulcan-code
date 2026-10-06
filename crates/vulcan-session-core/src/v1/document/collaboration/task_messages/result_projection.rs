//! Atomic complete-result insertion into the original requester turn.
//! 完整结果原子插入原请求者回合。

use super::*;
use crate::v1::ids::ClientActionId;

impl SessionDocument {
    /// Appends the authorized result chunks or one redacted failure at a legal API boundary.
    /// 在合法 API 边界追加已授权结果分块或一份脱敏失败。
    /// Returns the original durable proof on replay; a closed/replaced turn or active API is rejected.
    /// 重放返回原可靠凭据；已关闭或被替换回合、活动 API 均拒绝新插入。
    pub fn project_task_result_before_api(
        &mut self,
        delivery_id: CollaborationDeliveryId,
        disclosure: CollaborationTaskResultDisclosure,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskResultProjection>, DomainError> {
        validate_timestamp("collaboration.task_result.projected_at_ms", now_ms)?;
        let acceptance = self
            .collaboration_state()
            .received_task_results
            .get(&delivery_id)
            .cloned()
            .ok_or(DomainError::CollaborationNotFound {
                kind: "received_task_result",
                identity: delivery_id.as_str().to_owned(),
            })?;
        self.validate_source_participant(&acceptance.result.acceptance.message.source, true)?;
        if acceptance.disclosure == CollaborationTaskResultDisclosure::AuthorizationRevoked
            && disclosure == CollaborationTaskResultDisclosure::Full
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.projection_disclosure",
            });
        }
        if let Some(projection) = self
            .collaboration_state()
            .task_result_projections
            .get(&delivery_id)
        {
            return Ok(IdempotentMutation::Idempotent(projection.clone()));
        }
        let source_turn_id = acceptance.result.acceptance.message.source.turn_id.ok_or(
            DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.source_turn",
            },
        )?;
        // Late results retain their source identity but enter the current receiving turn.
        // 迟到结果保留原来源身份，但进入当前接收回合。
        let turn_id = self
            .active_turn()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.projection_turn",
            })?;
        if turn_id < source_turn_id
            || self.collaboration_run_control() != CollaborationRunControl::Running
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.projection_turn",
            });
        }
        let operation_id = CollaborationOperationId::try_new(format!(
            "task-result-project:{}",
            acceptance.event_seq.value()
        ))?;
        // A prior input-key replay cannot waive the physical request boundary for a new receipt.
        // 既有输入键重放不能豁免新回执所需的物理请求边界。
        if let Some(api) = self.active_api() {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        let semantic = CollaborationOperationSemantic::ProjectTaskResult {
            delivery_id: delivery_id.clone(),
        };
        // A receipt is inseparable from its complete input batch; conflicts must not append a prefix.
        // 回执与完整输入批次不可分割；发生冲突不得追加部分前缀。
        if self
            .existing_collaboration_operation(&operation_id, &semantic)?
            .is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.projection_orphan_operation",
            });
        }
        let mut staged = self.begin_atomic_mutation()?;
        let mut inputs = Vec::new();
        let envelopes = acceptance
            .result
            .task_result_envelopes(acceptance.event_seq, disclosure)?;
        // Revocation hides the original request as well as the final response.
        // 撤权同时隐藏原请求和最终回复。
        if disclosure == CollaborationTaskResultDisclosure::Full {
            let first = envelopes
                .first()
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.empty_envelopes",
                })?;
            super::super::projection::append_late_result_context(
                &mut staged,
                first,
                source_turn_id,
                &acceptance.result.acceptance.message.body,
                now_ms,
            )?;
        }
        for envelope in envelopes {
            let (input_seq, input_id, projected_turn) =
                super::super::projection::append_envelope(&mut staged, envelope, now_ms)?;
            if projected_turn != turn_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_turn",
                });
            }
            inputs.push(CollaborationTaskResultInput {
                input_seq,
                input_id,
            });
        }
        let projection = CollaborationTaskResultProjection {
            delivery_id: delivery_id.clone(),
            acceptance_seq: acceptance.event_seq,
            turn_id,
            disclosure,
            inputs,
            operation_id: operation_id.clone(),
            event_seq: next_event_seq(&staged)?,
            receiver_revision: next_revision(&staged)?,
            projected_at_ms: now_ms,
        };
        let mut state = staged.collaboration_state().clone();
        state
            .task_result_projections
            .insert(delivery_id, projection.clone());
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: projection.event_seq,
                result: CollaborationOperationResult::TaskResultProjected {
                    projection: projection.clone(),
                },
            },
        );
        staged.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskResultProjected,
            CollaborationAfterState::TaskResultProjection {
                projection: projection.clone(),
            },
            now_ms,
        )?;
        staged.validate_task_result_projection_facts()?;
        staged.commit();
        Ok(IdempotentMutation::Committed(projection))
    }

    /// Verifies immutable projection events and every resident chunk against its frozen disclosure.
    /// 按冻结的披露决策核验不可变投影事件及每份驻留分块。
    /// Sealed chunks retain metadata proofs; resident chunks must match identity, turn, order and body.
    /// 封存分块保留元数据凭据；驻留分块必须匹配身份、回合、顺序及正文。
    pub(in crate::v1::document) fn validate_task_result_projection_facts(
        &self,
    ) -> Result<(), DomainError> {
        for projection in self.collaboration_state().task_result_projections.values() {
            let acceptance = self
                .collaboration_state()
                .received_task_results
                .get(&projection.delivery_id)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_acceptance",
                })?;
            self.validate_source_participant(&acceptance.result.acceptance.message.source, true)?;
            if acceptance.disclosure == CollaborationTaskResultDisclosure::AuthorizationRevoked
                && projection.disclosure == CollaborationTaskResultDisclosure::Full
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_disclosure",
                });
            }
            if projection.receiver_revision > self.memory_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_future_revision",
                });
            }
            if projection.event_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event|
                event.seq == projection.event_seq && event.turn_id.is_none() && event.visibility == EventVisibility::HostOnly
                && event.created_at_ms == projection.projected_at_ms
                && matches!(&event.event, EventBody::Collaboration(collaboration)
                    if matches!(&collaboration.after_state, CollaborationAfterState::TaskResultProjection { projection: stored } if stored == projection)))
            {
                return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.projection_resident_fact" });
            }
            let envelopes = acceptance
                .result
                .task_result_envelopes(acceptance.event_seq, projection.disclosure)?;
            if envelopes.len() != projection.inputs.len() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_chunks",
                });
            }
            for (input, envelope) in projection.inputs.iter().zip(envelopes) {
                if input.input_seq.value() < self.resident_start_seq {
                    continue;
                }
                let expected_action =
                    ClientActionId::try_new(format!("agent-message:{}", envelope.event_id))?;
                let expected_text =
                    envelope
                        .render()
                        .map_err(|_| DomainError::CollaborationStateViolation {
                            field: "collaboration.task_result.projection_envelope",
                        })?;
                if !self.events.iter().any(|event| event.seq == input.input_seq && event.turn_id == Some(projection.turn_id)
                    && event.visibility == EventVisibility::Provider && event.created_at_ms == projection.projected_at_ms
                    && matches!(&event.event, EventBody::UserInput(stored) if stored.input_id == input.input_id
                        && stored.input_source == crate::v1::event::UserInputSource::AgentMessage
                        && stored.client_action_id == expected_action && stored.text == expected_text))
                {
                    return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.projection_input" });
                }
            }
        }
        Ok(())
    }
}
