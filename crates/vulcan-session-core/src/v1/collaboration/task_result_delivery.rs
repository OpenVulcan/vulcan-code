//! Reliable receiver receipts and executor acknowledgements for immutable task results.
//! 不可变任务结果的可靠接收回执与执行者确认。

use super::*;

/// Requester-owned receipt for a complete result, independent of its current business turn.
/// 请求者持有的完整结果回执，独立于其当前业务回合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskResultAcceptance {
    /// Exact executor-owned immutable result and original participants.
    /// 精确执行者所有不可变结果及原始参与者。
    pub result: CollaborationTaskResult,
    /// Stable receiver operation and local event identity.
    /// 稳定接收操作及本地事件身份。
    pub operation_id: CollaborationOperationId,
    /// Receiver-local host event used for deterministic later projection keys.
    /// 接收方本地宿主事件，用于后续确定性投影键。
    pub event_seq: Seq,
    /// Receiver revision and timestamp publishing this acceptance.
    /// 发布此接纳的接收方修订及时间戳。
    pub receiver_revision: u64,
    /// Original acceptance timestamp, unchanged by retry.
    /// 原接纳时间戳，重试不改变。
    pub accepted_at_ms: i64,
}

impl CollaborationTaskResultAcceptance {
    /// Validates the immutable result and receiver publication coordinates; returns a domain error on conflict.
    /// 校验不可变结果及接收方发布坐标；冲突时返回领域错误。
    pub fn validate(&self) -> Result<(), DomainError> {
        self.result.validate()?;
        validate_timestamp(
            "collaboration.task_result.accepted_at_ms",
            self.accepted_at_ms,
        )?;
        if self.receiver_revision == 0 {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.receiver_revision",
            });
        }
        Ok(())
    }
}

/// Executor-owned durable acknowledgement; it does not claim the receiving model consumed the result.
/// 执行者所有可靠确认；不表示接收方模型已经消费结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskResultAcknowledgement {
    /// Original receiver receipt, retained exactly across retries.
    /// 原始接收回执，重试时精确保留。
    pub acceptance: CollaborationTaskResultAcceptance,
    /// Executor acknowledgement operation and local event identity.
    /// 执行者确认操作及本地事件身份。
    pub operation_id: CollaborationOperationId,
    /// Executor-local event publishing the acknowledgement.
    /// 发布确认的执行者本地事件。
    pub event_seq: Seq,
    /// Executor publication revision and acknowledgement time.
    /// 执行者发布修订及确认时间。
    pub target_revision: u64,
    /// Original internal acknowledgement timestamp.
    /// 原始内部确认时间戳。
    pub acknowledged_at_ms: i64,
}

impl CollaborationTaskResultAcknowledgement {
    /// Validates exact receiver proof and local ordering; returns a domain error for invalid coordinates.
    /// 校验精确接收凭据及本地顺序；无效坐标返回领域错误。
    pub fn validate(&self) -> Result<(), DomainError> {
        self.acceptance.validate()?;
        validate_timestamp(
            "collaboration.task_result.acknowledged_at_ms",
            self.acknowledged_at_ms,
        )?;
        if self.target_revision <= self.acceptance.result.target_revision
            || self.event_seq <= self.acceptance.result.event_seq
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.ack_coordinates",
            });
        }
        Ok(())
    }
}

impl CollaborationState {
    /// Checks result transfer maps and their exact operation bindings without requiring resident history.
    /// 不依赖驻留历史，检查结果投递表及其精确操作绑定。
    /// Returns a domain error when any source intent, target result or immutable receipt differs.
    /// 任一来源意图、目标结果或不可变回执不一致时返回领域错误。
    pub(super) fn validate_task_result_transfers(&self) -> Result<(), DomainError> {
        for (delivery_id, acceptance) in &self.received_task_results {
            acceptance.validate()?;
            let result = &acceptance.result;
            let outbound = self.outbound_task_messages.get(delivery_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.outbound_missing",
                },
            )?;
            if result.effect.delivery_id != *delivery_id
                || outbound.message != result.acceptance.message
                || outbound.acceptance.as_ref() != Some(&result.acceptance)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.outbound_binding",
                });
            }
            let operation = self.operations.get(&acceptance.operation_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.accept_operation_missing",
                },
            )?;
            if operation.event_seq != acceptance.event_seq
                || operation.semantic
                    != (CollaborationOperationSemantic::AcceptTaskResult {
                        result: result.clone(),
                    })
                || operation.result
                    != (CollaborationOperationResult::TaskResultAccepted {
                        acceptance: acceptance.clone(),
                    })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.accept_operation_binding",
                });
            }
        }
        for (delivery_id, acknowledgement) in &self.task_result_acknowledgements {
            acknowledgement.validate()?;
            if acknowledgement.acceptance.result.effect.delivery_id != *delivery_id
                || self.task_results.get(delivery_id) != Some(&acknowledgement.acceptance.result)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.ack_result_binding",
                });
            }
            let operation = self.operations.get(&acknowledgement.operation_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.ack_operation_missing",
                },
            )?;
            if operation.event_seq != acknowledgement.event_seq
                || operation.semantic
                    != (CollaborationOperationSemantic::AcknowledgeTaskResult {
                        acceptance: acknowledgement.acceptance.clone(),
                    })
                || operation.result
                    != (CollaborationOperationResult::TaskResultAcknowledged {
                        acknowledgement: acknowledgement.clone(),
                    })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.ack_operation_binding",
                });
            }
        }
        for operation in self.operations.values() {
            // New semantics and result variants form closed pairs, including otherwise orphaned operations.
            // 新语义与结果分支组成封闭配对，包括其他情况下可能孤立的操作。
            let paired = match &operation.semantic {
                CollaborationOperationSemantic::AcceptTaskResult { .. } => matches!(
                    operation.result,
                    CollaborationOperationResult::TaskResultAccepted { .. }
                ),
                CollaborationOperationSemantic::AcknowledgeTaskResult { .. } => matches!(
                    operation.result,
                    CollaborationOperationResult::TaskResultAcknowledged { .. }
                ),
                _ => !matches!(
                    operation.result,
                    CollaborationOperationResult::TaskResultAccepted { .. }
                        | CollaborationOperationResult::TaskResultAcknowledged { .. }
                ),
            };
            if !paired {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.transfer_operation_pair",
                });
            }
            let matches = match &operation.result {
                CollaborationOperationResult::TaskResultAccepted { acceptance } => {
                    self.received_task_results
                        .get(&acceptance.result.effect.delivery_id)
                        == Some(acceptance)
                        && operation.operation_id == acceptance.operation_id
                }
                CollaborationOperationResult::TaskResultAcknowledged { acknowledgement } => {
                    self.task_result_acknowledgements
                        .get(&acknowledgement.acceptance.result.effect.delivery_id)
                        == Some(acknowledgement)
                        && operation.operation_id == acknowledgement.operation_id
                }
                _ => true,
            };
            if !matches {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.transfer_reverse_binding",
                });
            }
        }
        Ok(())
    }
}
