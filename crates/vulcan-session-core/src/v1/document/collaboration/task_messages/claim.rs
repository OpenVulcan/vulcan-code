//! Atomic admission of an accepted queued task into its own business turn.
//! 将已接纳排队任务原子准入自己的业务回合。

use super::*;
use crate::v1::PendingInputId;
use crate::v1::document::{ActiveTurnState, TurnOrigin};

impl SessionDocument {
    /// Claims accepted `delivery_id` under `operation_id` at `now_ms`.
    /// 在 `now_ms` 使用 `operation_id` 认领已接纳的 `delivery_id`。
    /// Returns the original claim receipt on retry; callers must separately prove live launch ownership.
    /// 重试返回原认领回执；调用方必须另行证明当前启动所有权。
    /// A claim opens one business turn and appends its tagged input, without dispatching a physical API.
    /// 认领开启一个业务回合并追加带标签输入，不分发物理 API。
    pub fn claim_task_message_before_api(
        &mut self,
        operation_id: CollaborationOperationId,
        delivery_id: CollaborationDeliveryId,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CollaborationTaskMessageEffect>, DomainError> {
        let acceptance = self.task_message_acceptance_for_effect(&delivery_id)?;
        let semantic = CollaborationOperationSemantic::ClaimTaskMessage {
            delivery_id: delivery_id.clone(),
        };
        if let Some(effect) = self.replay_task_message_effect(&operation_id, &semantic)? {
            return Ok(IdempotentMutation::Idempotent(effect));
        }
        if let Some(effect) = self
            .collaboration_state()
            .task_message_effects
            .get(&delivery_id)
        {
            if matches!(
                &effect.outcome,
                CollaborationTaskMessageEffectKind::Claimed { .. }
            ) {
                return Ok(IdempotentMutation::Idempotent(effect.clone()));
            }
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.claim_terminal_delivery",
            });
        }
        if acceptance.message.kind != CollaborationTaskMessageKind::TaskStart {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.claim_kind",
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
                field: "collaboration.task_message.claim_stopped",
            });
        }
        // User queue priority and all existing execution ownership are preserved.
        // 保留用户队列优先级及全部现有执行所有权。
        if self.active_turn().is_some_and(|turn| !turn.closed)
            || self.active_api().is_some()
            || !self.metadata.input_state.queued_inputs.is_empty()
            || self
                .collaboration_state()
                .question_executions
                .values()
                .any(|execution| execution.state == CollaborationQuestionExecutionState::Running)
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.claim_busy",
            });
        }
        // Reception order is authoritative; callers cannot choose a later pending task first.
        // 接收顺序是权威；调用者不能先选后来的待执行任务。
        if self
            .collaboration_state()
            .inbound_task_messages
            .values()
            .any(|earlier| {
                earlier.message.kind == CollaborationTaskMessageKind::TaskStart
                    && earlier.event_seq < acceptance.event_seq
                    && !self
                        .collaboration_state()
                        .task_message_effects
                        .contains_key(&earlier.message.delivery_id)
            })
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.claim_queue_order",
            });
        }
        self.ensure_transition_capacity(2, 2)?;
        // The draft owns identity allocation, claim, input and optional configuration/control changes.
        // 草稿拥有身份分配、认领、输入及可选配置和控制变更。
        let mut staged = self.begin_atomic_mutation()?;
        let turn_id = TurnId::try_new(staged.id_allocator.next_local_id()?)?;
        let input_id = PendingInputId::try_new(staged.id_allocator.frontier().0)?;
        let input_seq = Seq::try_new(next_event_seq(&staged)?.value().checked_add(1).ok_or(
            DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.claim_sequence",
            },
        )?)?;
        let execution_id =
            CollaborationExecutionId::try_new(format!("{}:task-execution", delivery_id.as_str()))?;
        let effect = staged.commit_task_message_effect(
            operation_id,
            delivery_id.clone(),
            semantic,
            CollaborationTaskMessageEffectKind::Claimed {
                execution_id,
                turn_id,
                input_id,
                input_seq,
            },
            now_ms,
        )?;
        staged.active_turn = Some(ActiveTurnState {
            turn_id,
            origin: TurnOrigin::AgentTask,
            start_seq: effect.event_seq,
            closed: false,
        });
        staged.metadata.open_turn_start_seq = Some(effect.event_seq);
        let actual = super::super::projection::append_envelope(
            &mut staged,
            effects::task_message_envelope(&acceptance.message),
            now_ms,
        )?;
        if actual != (input_seq, input_id, turn_id) {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.claim_input",
            });
        }
        // Apply the target's own pending configuration; never consume a human draft or sender settings.
        // 应用目标自己的待生效配置；绝不消费用户草稿或发送方设置。
        if staged.metadata.config.pending_request.is_some() {
            staged.promote_pending_request(Some(turn_id), now_ms)?;
        }
        if staged.collaboration_run_control() != CollaborationRunControl::Running {
            staged.set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "{}:task-running",
                    delivery_id.as_str()
                ))?,
                state: CollaborationRunControl::Running,
                changed_at_ms: now_ms,
            })?;
        }
        self.commit_atomic_mutation(staged);
        Ok(IdempotentMutation::Committed(effect))
    }
}
