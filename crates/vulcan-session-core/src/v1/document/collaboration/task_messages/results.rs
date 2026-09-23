//! Freezes real task terminals in the transaction that closes their business turn.
//! 在闭合业务回合的事务中冻结真实任务终态。

use super::*;
use crate::v1::event::{
    ApiCallStatus, ApiPurposeKind, AssistantSemanticStatus, TurnBoundaryPurpose, TurnOutcome,
};

impl SessionDocument {
    /// Freezes every applied task message bound to `turn_id` at `now_ms`.
    /// 在 `now_ms` 冻结所有绑定 `turn_id` 的已应用任务消息。
    /// Called inside the closure's atomic draft; returns after all result proofs are committed.
    /// 在闭合的原子草稿内调用；全部结果凭据提交后返回。
    pub(in crate::v1::document) fn record_task_results_for_turn(
        &mut self,
        turn_id: TurnId,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        // Clone only exact matching effects so committing results cannot change candidate ownership.
        // 仅复制精确匹配效果，避免结果提交改变候选归属。
        let effects = self.collaboration_state().task_message_effects.values()
            .filter(|effect| matches!(&effect.outcome,
                CollaborationTaskMessageEffectKind::Claimed { turn_id: owner, .. }
                | CollaborationTaskMessageEffectKind::Steered { turn_id: owner, .. } if *owner == turn_id))
            .cloned().collect::<Vec<_>>();
        for effect in effects {
            if self
                .collaboration_state()
                .task_results
                .contains_key(&effect.delivery_id)
            {
                continue;
            }
            // Target event identity is unique and keeps the operation bounded even for long delivery IDs.
            // 目标事件身份唯一，即使投递 ID 很长也能保持操作身份有界。
            let operation_id = CollaborationOperationId::try_new(format!(
                "task-result:{}",
                effect.event_seq.value()
            ))?;
            let semantic = CollaborationOperationSemantic::RecordTaskResult {
                delivery_id: effect.delivery_id.clone(),
            };
            if self
                .existing_collaboration_operation(&operation_id, &semantic)?
                .is_some()
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.missing_snapshot",
                });
            }
            let result = CollaborationTaskResult {
                acceptance: self.task_message_acceptance_for_effect(&effect.delivery_id)?,
                terminal: Some(self.build_task_terminal(turn_id)?),
                effect,
                operation_id: operation_id.clone(),
                target_revision: next_revision(self)?,
                event_seq: next_event_seq(self)?,
                recorded_at_ms: now_ms,
            };
            result.validate()?;
            let mut state = self.collaboration_state().clone();
            state
                .task_results
                .insert(result.effect.delivery_id.clone(), result.clone());
            state.operations.insert(
                operation_id.clone(),
                CollaborationOperationRecord {
                    operation_id: operation_id.clone(),
                    semantic,
                    event_seq: result.event_seq,
                    result: CollaborationOperationResult::TaskResult {
                        result: result.clone(),
                    },
                },
            );
            self.commit_collaboration_event(
                state,
                operation_id,
                CollaborationEventKind::TaskResultRecorded,
                CollaborationAfterState::TaskResult { result },
                now_ms,
            )?;
        }
        Ok(())
    }

    /// Freezes one failed application as a terminal delivery receipt without a business response.
    /// 将一次应用前失败冻结为没有业务回复的终态投递回执。
    /// `delivery_id` identifies the target's exact failed effect; retries return its first receipt.
    /// `delivery_id` 指向目标精确失败效果；重试返回首次回执。
    pub fn record_failed_task_result(
        &mut self,
        delivery_id: CollaborationDeliveryId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskResult>, DomainError> {
        validate_timestamp("collaboration.task_result.recorded_at_ms", now_ms)?;
        let effect = self
            .collaboration_state()
            .task_message_effects
            .get(&delivery_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "task_message_effect",
                identity: delivery_id.as_str().to_owned(),
            })?;
        if !matches!(effect.outcome, CollaborationTaskMessageEffectKind::Failed { .. }) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.not_failed",
            });
        }
        if let Some(previous) = self.collaboration_state().task_results.get(&delivery_id) {
            return Ok(IdempotentMutation::Idempotent(previous.clone()));
        }
        let operation_id = CollaborationOperationId::try_new(format!(
            "task-result:{}",
            effect.event_seq.value()
        ))?;
        let semantic = CollaborationOperationSemantic::RecordTaskResult {
            delivery_id: delivery_id.clone(),
        };
        if self.existing_collaboration_operation(&operation_id, &semantic)?.is_some() {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.missing_snapshot",
            });
        }
        let result = CollaborationTaskResult {
            acceptance: self.task_message_acceptance_for_effect(&delivery_id)?,
            effect,
            terminal: None,
            operation_id: operation_id.clone(),
            target_revision: next_revision(self)?,
            event_seq: next_event_seq(self)?,
            recorded_at_ms: now_ms,
        };
        result.validate()?;
        let mut state = self.collaboration_state().clone();
        state.task_results.insert(delivery_id, result.clone());
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: result.event_seq,
                result: CollaborationOperationResult::TaskResult {
                    result: result.clone(),
                },
            },
        );
        self.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::TaskResultRecorded,
            CollaborationAfterState::TaskResult {
                result: result.clone(),
            },
            now_ms,
        )?;
        Ok(IdempotentMutation::Committed(result))
    }

    /// Reads the unique resident terminal and, on success, its final normal business response.
    /// 读取唯一驻留终态；成功时读取其最终正常业务回复。
    /// `turn_id` is authoritative effect ownership, never the currently selected session turn.
    /// `turn_id` 来自权威效果归属，绝非当前界面选中的会话回合。
    fn build_task_terminal(
        &self,
        turn_id: TurnId,
    ) -> Result<CollaborationTaskTerminal, DomainError> {
        let stats = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnStats(stats) if event.turn_id == Some(turn_id) => {
                    Some((event, stats))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if stats.len() != 1 {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.unique_terminal",
            });
        }
        let (stats_event, stats) = stats[0];
        if stats.end_seq != stats_event.seq || stats.start_seq >= stats.end_seq {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.terminal_range",
            });
        }
        let boundary = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::TurnBoundary(boundary)
                    if event.turn_id == Some(turn_id) && event.seq < stats_event.seq =>
                {
                    Some(boundary)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if boundary.len() != 1
            || boundary[0].outcome != stats.outcome
            || boundary[0].purpose != TurnBoundaryPurpose::Business
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.business_boundary",
            });
        }
        let final_assistant = if stats.outcome == TurnOutcome::Completed {
            let (api_event, api) = self
                .events
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::ApiStats(api)
                        if event.turn_id == Some(turn_id) && event.seq < stats_event.seq =>
                    {
                        Some((event, api))
                    }
                    _ => None,
                })
                .max_by_key(|(event, _)| event.seq)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.final_api_missing",
                })?;
            if api.status != ApiCallStatus::Completed
                || matches!(
                    api.purpose,
                    ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
                )
                || self.events.iter().any(|event| {
                    matches!(&event.event,
                    EventBody::ToolCall(tool) if tool.api_call_id == api.api_call_id)
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.final_business_api",
                });
            }
            let mut texts = self.events.iter().filter_map(|event| match &event.event {
                EventBody::AssistantText(text)
                    if event.turn_id == Some(turn_id)
                        && text.api_call_id == api.api_call_id
                        && text.status == AssistantSemanticStatus::Completed =>
                {
                    Some(CollaborationTaskResultText {
                        event_seq: event.seq,
                        text: text.text.clone(),
                    })
                }
                _ => None,
            });
            let source_text = texts
                .next()
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.final_text_missing",
                })?;
            if texts.next().is_some() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.final_text_ambiguous",
                });
            }
            Some(CollaborationTaskFinalAssistant {
                api_call_id: api.api_call_id,
                api_stats_seq: api_event.seq,
                source_text,
            })
        } else {
            None
        };
        Ok(CollaborationTaskTerminal {
            turn_id,
            turn_start_seq: stats.start_seq,
            turn_stats_seq: stats_event.seq,
            outcome: stats.outcome,
            final_assistant,
            completed_at_ms: stats_event.created_at_ms,
        })
    }

    /// Validates frozen results against every source fact retained in the active tail.
    /// 根据活动尾段保留的全部来源事实校验冻结结果。
    /// Sealed facts keep their immutable metadata proof; resident facts must match exactly.
    /// 封存事实保留不可变元数据凭据；驻留事实必须精确匹配。
    pub(in crate::v1::document) fn validate_task_result_facts(&self) -> Result<(), DomainError> {
        // Every applied effect belongs either to the current open turn or to a frozen terminal.
        // 每个已应用效果必须属于当前开放回合，或已经拥有冻结终态。
        // This also detects missing results when their original event segment is no longer resident.
        // 原事件段不再驻留时，此规则仍能发现丢失的结果。
        for effect in self.collaboration_state().task_message_effects.values() {
            let turn_id = match effect.outcome {
                CollaborationTaskMessageEffectKind::Claimed { turn_id, .. }
                | CollaborationTaskMessageEffectKind::Steered { turn_id, .. } => turn_id,
                CollaborationTaskMessageEffectKind::Failed { .. } => continue,
            };
            let is_open = self
                .active_turn()
                .is_some_and(|turn| !turn.closed && turn.turn_id == turn_id);
            let has_result = self
                .collaboration_state()
                .task_results
                .contains_key(&effect.delivery_id);
            if is_open == has_result {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.business_lifetime",
                });
            }
        }
        for result in self.collaboration_state().task_results.values() {
            result.validate()?;
            self.validate_target_participant(&result.acceptance.message.target)?;
            if result.target_revision > self.memory_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.future_revision",
                });
            }
            if result.event_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event| {
                event.seq == result.event_seq && event.turn_id.is_none() && event.visibility == EventVisibility::HostOnly
                    && matches!(&event.event, EventBody::Collaboration(collaboration)
                        if matches!(&collaboration.after_state, CollaborationAfterState::TaskResult { result: stored } if stored == result))
            }) {
                return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.host_fact" });
            }
            let Some(terminal) = &result.terminal else {
                continue;
            };
            if terminal.turn_start_seq.value() >= self.resident_start_seq {
                if self.build_task_terminal(terminal.turn_id)? != *terminal {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.task_result.terminal_fact",
                    });
                }
                continue;
            }
            // Partial tails validate each remaining referenced fact; absence is legal only before the resident frontier.
            // 部分尾段逐项校验仍保留的引用事实；仅驻留前沿之前允许缺失。
            let boundaries = self
                .events
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::TurnBoundary(boundary)
                        if event.turn_id == Some(terminal.turn_id) =>
                    {
                        Some((event, boundary))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if boundaries.len() > 1
                || boundaries.iter().any(|(event, boundary)| {
                    event.seq >= terminal.turn_stats_seq
                        || boundary.outcome != terminal.outcome
                        || boundary.purpose != TurnBoundaryPurpose::Business
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.resident_boundary",
                });
            }
            if terminal.turn_stats_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event| {
                event.seq == terminal.turn_stats_seq && event.turn_id == Some(terminal.turn_id)
                    && event.created_at_ms == terminal.completed_at_ms
                    && matches!(&event.event, EventBody::TurnStats(stats) if stats.start_seq == terminal.turn_start_seq
                        && stats.end_seq == terminal.turn_stats_seq && stats.outcome == terminal.outcome)
            }) {
                return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.resident_terminal" });
            }
            if let Some(assistant) = &terminal.final_assistant {
                // A sealed prefix does not permit a later API, duplicate text or a tool-bearing final.
                // 前缀封存不能放行较晚 API、重复正文或含工具的最终回复。
                if self.events.iter().any(|event| match &event.event {
                    EventBody::ApiStats(_) => {
                        event.turn_id == Some(terminal.turn_id)
                            && event.seq > assistant.api_stats_seq
                            && event.seq < terminal.turn_stats_seq
                    }
                    EventBody::ToolCall(tool) => tool.api_call_id == assistant.api_call_id,
                    EventBody::AssistantText(text) => {
                        text.api_call_id == assistant.api_call_id
                            && event.seq != assistant.source_text.event_seq
                    }
                    _ => false,
                }) {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.task_result.resident_final_conflict",
                    });
                }
                if assistant.api_stats_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event| {
                    event.seq == assistant.api_stats_seq && event.turn_id == Some(terminal.turn_id)
                        && matches!(&event.event, EventBody::ApiStats(api) if api.api_call_id == assistant.api_call_id
                            && api.status == ApiCallStatus::Completed
                            && !matches!(api.purpose, ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                }) {
                    return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.resident_api" });
                }
                {
                    let part = &assistant.source_text;
                    if part.event_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event| {
                        event.seq == part.event_seq && event.turn_id == Some(terminal.turn_id)
                            && matches!(&event.event, EventBody::AssistantText(text)
                                if text.api_call_id == assistant.api_call_id && text.status == AssistantSemanticStatus::Completed
                                    && text.text == part.text)
                    }) {
                        return Err(DomainError::CollaborationStateViolation { field: "collaboration.task_result.resident_text" });
                    }
                }
            }
        }
        Ok(())
    }
}
