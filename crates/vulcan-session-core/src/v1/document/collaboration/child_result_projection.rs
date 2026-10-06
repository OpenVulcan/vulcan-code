//! Atomic parent-context insertion and recovery validation for non-success child reports.
//! 非成功子报告的父上下文原子插入及恢复校验。

use super::*;
use crate::v1::document::TurnOrigin;
use crate::v1::ids::ClientActionId;

impl SessionDocument {
    /// Projects every chunk of `execution_id` at the original parent's legal request boundary.
    /// 在原父会话合法请求边界投影 `execution_id` 的全部分块。
    /// `now_ms` timestamps the atomic batch; retries return its original immutable receipt.
    /// `now_ms` 为原子批次时间戳；重试返回原不可变回执。
    /// Returns an error without partial insertion on ownership, durability or operation conflicts.
    /// 归属、持久化或操作冲突返回错误，不发生部分插入。
    pub fn project_child_task_non_success_before_api(
        &mut self,
        execution_id: CollaborationExecutionId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationChildTaskResultProjection>, DomainError> {
        validate_timestamp("collaboration.child_result.projected_at_ms", now_ms)?;
        // Resolve the exact original execution rather than the physical child's newest run.
        // 解析精确原执行，不使用物理子会话最新运行。
        let task = self
            .collaboration_state()
            .child_tasks
            .get(&execution_id)
            .cloned()
            .ok_or_else(|| DomainError::CollaborationNotFound {
                kind: "child_task",
                identity: execution_id.as_str().to_owned(),
            })?;
        // The report's publication revision is part of the model-disclosure authority.
        // 报告发布修订属于模型披露权威的一部分。
        let result =
            task.non_success_result
                .as_ref()
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.projection_report",
                })?;
        if let Some(projection) = &task.non_success_projection {
            return Ok(IdempotentMutation::Idempotent(projection.clone()));
        }
        if self.durable_revision < result.parent_revision
            || task.parent_segment_generation != self.segment_generation()
            || self.collaboration_run_control() != CollaborationRunControl::Running
            || self.active_turn().is_none_or(|turn| {
                turn.closed
                    || turn.turn_id != task.parent_turn_id
                    || turn.origin == TurnOrigin::QuestionExecution
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.projection_owner",
            });
        }
        if let Some(api) = self.active_api() {
            return Err(DomainError::TurnCompletionWhileApiActive {
                turn_id: task.parent_turn_id.value(),
                api_call_id: api.api_call_id.value(),
            });
        }
        // Derive a bounded local operation from the accepted result sequence.
        // 从已接纳结果序号派生有界本地操作。
        let operation_id = CollaborationOperationId::try_new(format!(
            "child-result-project:{}",
            result.event_seq.value()
        ))?;
        // The operation selects an immutable report and execution, not a body supplied by the caller.
        // 操作选定不可变报告与执行，而非调用方提供的正文。
        let semantic = CollaborationOperationSemantic::ProjectChildTaskNonSuccess {
            execution_id: execution_id.clone(),
            result_seq: result.event_seq,
        };
        if self
            .existing_collaboration_operation(&operation_id, &semantic)?
            .is_some()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.orphan_projection",
            });
        }
        // Build complete envelopes before staging so oversized or invalid input cannot leave a prefix.
        // 暂存前构造完整信封，避免超限或无效输入留下前缀。
        let envelopes = result.model_envelopes(&task, &self.metadata.session_id)?;
        // Every input and its receipt share the same reserved publication revision.
        // 全部输入及其回执共用同一预留发布修订。
        let mut staged = self.begin_atomic_mutation()?;
        // Preserve exact input identities for replay and later business consumption checks.
        // 保留精确输入身份，用于重放及后续业务消费检查。
        let mut inputs = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            // The shared helper appends tagged User messages and marks them as AgentMessage.
            // 共享辅助器追加带标签 User 消息，并标记为 AgentMessage。
            let (input_seq, input_id, turn_id) =
                projection::append_envelope(&mut staged, envelope, now_ms)?;
            if turn_id != task.parent_turn_id {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.projection_turn",
                });
            }
            inputs.push(CollaborationTaskResultInput {
                input_seq,
                input_id,
            });
        }
        // The receipt follows the complete batch and retains the first projection's timestamp.
        // 回执位于完整批次之后，并保留首次投影时间戳。
        let projected = CollaborationChildTaskResultProjection {
            execution_id: execution_id.clone(),
            result_seq: result.event_seq,
            parent_turn_id: task.parent_turn_id,
            inputs,
            operation_id: operation_id.clone(),
            event_seq: next_event_seq(&staged)?,
            parent_revision: next_revision(&staged)?,
            projected_at_ms: now_ms,
        };
        // The mutable state is isolated until its inputs and proof pass validation together.
        // 可变状态保持隔离，直到输入及凭据一起通过校验。
        let mut state = staged.collaboration_state().clone();
        // Preserve the original result receipt while adding the separate projection fact.
        // 添加独立投影事实，同时保留原结果回执。
        let mut after = task;
        after.non_success_projection = Some(projected.clone());
        state
            .child_tasks
            .insert(execution_id.clone(), after.clone());
        state.operations.insert(
            operation_id.clone(),
            CollaborationOperationRecord {
                operation_id: operation_id.clone(),
                semantic,
                event_seq: projected.event_seq,
                result: CollaborationOperationResult::ChildTask {
                    execution_id,
                    task_id: after.task_id.clone(),
                    state: CollaborationChildTaskState::Terminal,
                },
            },
        );
        staged.commit_collaboration_event(
            state,
            operation_id,
            CollaborationEventKind::ChildTaskNonSuccessProjected,
            CollaborationAfterState::ChildTask { task: after },
            now_ms,
        )?;
        staged.validate_child_result_projection_facts()?;
        staged.commit();
        Ok(IdempotentMutation::Committed(projected))
    }

    /// Validates immutable child reports and every resident projected input against canonical state.
    /// 按规范状态校验不可变子报告及每个驻留投影输入。
    /// Returns an error for missing, tampered or future facts; sealed inputs retain their metadata proof.
    /// 缺失、篡改或未来事实返回错误；封存输入保留元数据凭据。
    pub(in crate::v1::document) fn validate_child_result_projection_facts(
        &self,
    ) -> Result<(), DomainError> {
        for task in self.collaboration_state().child_tasks.values() {
            let Some(result) = &task.non_success_result else {
                continue;
            };
            if result.parent_revision > self.memory_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.future_report",
                });
            }
            let Some(projected) = &task.non_success_projection else {
                continue;
            };
            if projected.parent_revision > self.memory_revision {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.future_projection",
                });
            }
            if projected.event_seq.value() >= self.resident_start_seq && !self.events.iter().any(|event|
                event.seq == projected.event_seq && event.turn_id.is_none() && event.visibility == EventVisibility::HostOnly
                && event.created_at_ms == projected.projected_at_ms
                && matches!(&event.event, EventBody::Collaboration(collaboration)
                    if collaboration.event_kind == CollaborationEventKind::ChildTaskNonSuccessProjected
                    && matches!(&collaboration.after_state, CollaborationAfterState::ChildTask { task: after }
                        if after.non_success_projection.as_ref() == Some(projected)))) {
                return Err(DomainError::CollaborationStateViolation { field: "collaboration.child_result.projection_fact" });
            }
            // Rebuild all original bytes; count equality also detects a silently omitted final chunk.
            // 重建全部原始字节；数量一致性也能发现静默遗漏的最后一块。
            let envelopes = result.model_envelopes(task, &self.metadata.session_id)?;
            if envelopes.len() != projected.inputs.len() {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.projection_chunks",
                });
            }
            for (input, envelope) in projected.inputs.iter().zip(envelopes) {
                if input.input_seq.value() < self.resident_start_seq {
                    continue;
                }
                // Input action and body are reconstructed from authority rather than trusted from stored text.
                // 从权威重建输入动作与正文，不直接信任存储文本。
                let action =
                    ClientActionId::try_new(format!("agent-message:{}", envelope.event_id))?;
                // Shared escaping is identical on first insertion and recovery validation.
                // 首次插入与恢复校验使用完全相同的共享转义。
                let text =
                    envelope
                        .render()
                        .map_err(|_| DomainError::CollaborationStateViolation {
                            field: "collaboration.child_result.projection_envelope",
                        })?;
                if !self.events.iter().any(|event| event.seq == input.input_seq && event.turn_id == Some(projected.parent_turn_id)
                    && event.visibility == EventVisibility::Provider && event.created_at_ms == projected.projected_at_ms
                    && matches!(&event.event, EventBody::UserInput(stored) if stored.input_id == input.input_id
                        && stored.input_source == crate::v1::event::UserInputSource::AgentMessage
                        && stored.client_action_id == action && stored.text == text)) {
                    return Err(DomainError::CollaborationStateViolation { field: "collaboration.child_result.projection_input" });
                }
            }
        }
        Ok(())
    }
}
