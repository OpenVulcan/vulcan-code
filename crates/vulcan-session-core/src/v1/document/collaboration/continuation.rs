//! Derive pending business continuation from exact local acceptance and input facts.
//! 根据精确本地接纳与输入事实派生待处理业务续跑。

use super::*;
use crate::v1::document::TurnOrigin;
use crate::v1::event::ApiPurposeKind;

impl SessionDocument {
    /// Returns the oldest unconsumed steering/result for the open running business turn.
    /// 返回开放运行业务回合最早尚未消费的方向或结果。
    /// Notifications and questions never become completion obligations through this query.
    /// 通知与问题绝不通过此查询成为完成义务。
    pub fn pending_collaboration_continuation(&self) -> Option<CollaborationContinuation> {
        let turn = self
            .active_turn()
            .filter(|turn| !turn.closed && turn.origin != TurnOrigin::QuestionExecution)?;
        if self.collaboration_run_control() != CollaborationRunControl::Running {
            return None;
        }
        // Only an actual business API dispatch covers model input; compaction and auxiliary Q&A do not.
        // 只有实际业务 API 分发覆盖模型输入；压缩与辅助答疑不能覆盖。
        let last_business_start = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStarted(started)
                    if event.turn_id == Some(turn.turn_id)
                        && !matches!(
                            started.purpose,
                            ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
                        ) =>
                {
                    Some(event.seq)
                }
                _ => None,
            })
            .max();
        let state = self.collaboration_state();
        let mut candidates = Vec::new();
        // Unprojected child results retain continuation authority after their child becomes terminal.
        // 子任务终态之后，尚未投影的子结果继续持有续跑权威。
        for task in state.child_tasks.values() {
            if task.parent_turn_id != turn.turn_id {
                continue;
            }
            if let Some(result) = &task.non_success_result {
                if task
                    .non_success_projection
                    .as_ref()
                    .is_some_and(|projection| {
                        projection.inputs.iter().all(|input| {
                            last_business_start.is_some_and(|seq| seq > input.input_seq)
                        })
                    })
                {
                    continue;
                }
                candidates.push(CollaborationContinuation {
                    turn_id: turn.turn_id,
                    acceptance_seq: result.event_seq,
                    source: CollaborationContinuationSource::ChildTaskResult {
                        execution_id: task.execution_id.clone(),
                    },
                });
            }
        }
        for (delivery_id, acceptance) in &state.inbound_task_messages {
            if acceptance.message.kind != CollaborationTaskMessageKind::TaskSteer
                || acceptance.message.target.turn_id != Some(turn.turn_id)
            {
                continue;
            }
            let pending = match state
                .task_message_effects
                .get(delivery_id)
                .map(|effect| &effect.outcome)
            {
                None => true,
                Some(CollaborationTaskMessageEffectKind::Steered { input_seq, .. }) => {
                    last_business_start.is_none_or(|seq| seq < *input_seq)
                }
                Some(
                    CollaborationTaskMessageEffectKind::Failed { .. }
                    | CollaborationTaskMessageEffectKind::Claimed { .. },
                ) => false,
            };
            if pending {
                candidates.push(CollaborationContinuation {
                    turn_id: turn.turn_id,
                    acceptance_seq: acceptance.event_seq,
                    source: CollaborationContinuationSource::Steering {
                        delivery_id: delivery_id.clone(),
                    },
                });
            }
        }
        // A result already consumed in an earlier business turn must not become pending again.
        // 已在此前业务回合消费的结果不得再次变成待处理。
        let last_result_request = self
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::ApiStarted(started)
                    if !matches!(
                        started.purpose,
                        ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
                    ) =>
                {
                    Some(event.seq)
                }
                _ => None,
            })
            .max();
        for (delivery_id, acceptance) in &state.received_task_results {
            if acceptance
                .result
                .acceptance
                .message
                .source
                .turn_id
                .is_none_or(|source_turn| source_turn > turn.turn_id)
            {
                continue;
            }
            let pending = match state.task_result_projections.get(delivery_id) {
                None => true,
                Some(projection) => projection
                    .inputs
                    .iter()
                    .any(|input| last_result_request.is_none_or(|seq| seq < input.input_seq)),
            };
            if pending {
                candidates.push(CollaborationContinuation {
                    turn_id: turn.turn_id,
                    acceptance_seq: acceptance.event_seq,
                    source: CollaborationContinuationSource::TaskResult {
                        delivery_id: delivery_id.clone(),
                    },
                });
            }
        }
        candidates
            .into_iter()
            .min_by_key(|candidate| candidate.acceptance_seq)
    }
}
