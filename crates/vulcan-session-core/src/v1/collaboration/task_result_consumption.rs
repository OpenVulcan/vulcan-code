//! Durable input proofs for requester-side task-result projection.
//! 请求者侧任务结果投影的可靠输入凭据。

use super::*;

/// Exact ordered User input created for one result chunk.
/// 一份结果分块创建的精确有序 User 输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskResultInput {
    /// Receiver-local provider input event.
    /// 接收方本地 Provider 输入事件。
    pub input_seq: Seq,
    /// Stable identity of that User input.
    /// 该 User 输入的稳定身份。
    pub input_id: PendingInputId,
}

/// Content allowed in one immutable requester-side task-result projection.
/// 一次不可变请求者侧任务结果投影允许包含的内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationTaskResultDisclosure {
    /// Include the complete accepted result while the original grant is valid.
    /// 原授权有效时包含完整已接纳结果。
    Full,
    /// Report only that the original authorization was revoked before model disclosure.
    /// 只报告原授权在模型披露前已撤销。
    AuthorizationRevoked,
}

/// Immutable proof that the chosen result disclosure entered the original source turn atomically.
/// 选定的结果披露内容原子进入原来源回合的不可变凭据。
/// This proves context insertion, not successful model execution or acknowledgement.
/// 此记录证明上下文插入，不代表模型执行成功或回复确认。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskResultProjection {
    /// Original task delivery, resolved through the local accepted result.
    /// 原任务投递，通过本地已接纳结果解析。
    pub delivery_id: CollaborationDeliveryId,
    /// Exact local result acceptance event.
    /// 精确本地结果接纳事件。
    pub acceptance_seq: Seq,
    /// Original requesting business turn; never the executor's turn.
    /// 原请求业务回合，绝不是执行者回合。
    pub turn_id: TurnId,
    /// Frozen content decision made under the requester's canonical lifecycle gate.
    /// 在请求者规范生命周期锁内冻结的内容决策。
    pub disclosure: CollaborationTaskResultDisclosure,
    /// Complete ordered input proofs with no omitted chunks.
    /// 完整有序输入凭据，不遗漏任何分块。
    pub inputs: Vec<CollaborationTaskResultInput>,
    /// Stable operation that published the projection.
    /// 发布此次投影的稳定操作。
    pub operation_id: CollaborationOperationId,
    /// Local host event committed after all input chunks.
    /// 在全部输入分块之后提交的本地宿主事件。
    pub event_seq: Seq,
    /// Atomic revision publishing all inputs and their proof.
    /// 发布全部输入及其凭据的原子修订。
    pub receiver_revision: u64,
    /// Original projection time, unchanged by replay.
    /// 原投影时间，重放不改变。
    pub projected_at_ms: i64,
}

impl CollaborationTaskResultProjection {
    /// Validates ordered nonempty input coordinates and publication metadata.
    /// 校验有序非空输入坐标和发布元数据。
    /// Returns an explicit error for duplicate identities or inputs outside the receipt interval.
    /// 身份重复或输入落在回执区间之外时返回明确错误。
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_timestamp(
            "collaboration.task_result.projected_at_ms",
            self.projected_at_ms,
        )?;
        let mut previous = self.acceptance_seq;
        let mut identities = BTreeSet::new();
        if self.inputs.is_empty() || self.receiver_revision == 0 {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.projection_shape",
            });
        }
        for input in &self.inputs {
            if input.input_seq <= previous
                || input.input_seq >= self.event_seq
                || !identities.insert(input.input_id)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_order",
                });
            }
            previous = input.input_seq;
        }
        Ok(())
    }
}

impl CollaborationState {
    /// Verifies projection ownership, complete chunk count and bidirectional operation bindings.
    /// 核验投影归属、完整分块数量和双向操作绑定。
    /// Accepted result metadata remains authoritative even when source events are sealed.
    /// 即使来源事件封存，已接纳结果元数据仍为权威。
    pub(super) fn validate_task_result_projections(&self) -> Result<(), DomainError> {
        for (delivery_id, projection) in &self.task_result_projections {
            projection.validate()?;
            let acceptance = self.received_task_results.get(delivery_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_acceptance",
                },
            )?;
            if projection.delivery_id != *delivery_id
                || projection.acceptance_seq != acceptance.event_seq
                || acceptance
                    .result
                    .acceptance
                    .message
                    .source
                    .turn_id
                    .is_none_or(|source_turn| projection.turn_id < source_turn)
                || projection.receiver_revision <= acceptance.receiver_revision
                || (acceptance.disclosure
                    == CollaborationTaskResultDisclosure::AuthorizationRevoked
                    && projection.disclosure == CollaborationTaskResultDisclosure::Full)
                || (projection.disclosure
                    == CollaborationTaskResultDisclosure::AuthorizationRevoked
                    && (acceptance.result.terminal.is_none()
                        || acceptance.result.acceptance.message.source.project_id
                            == acceptance.result.acceptance.message.target.project_id))
                || projection.inputs.len()
                    != acceptance
                        .result
                        .task_result_envelopes(acceptance.event_seq, projection.disclosure)?
                        .len()
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_binding",
                });
            }
            let operation = self.operations.get(&projection.operation_id).ok_or(
                DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_operation",
                },
            )?;
            if operation.event_seq != projection.event_seq
                || operation.semantic
                    != (CollaborationOperationSemantic::ProjectTaskResult {
                        delivery_id: delivery_id.clone(),
                    })
                || operation.result
                    != (CollaborationOperationResult::TaskResultProjected {
                        projection: projection.clone(),
                    })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.projection_operation_binding",
                });
            }
        }
        for operation in self.operations.values() {
            match (&operation.semantic, &operation.result) {
                (
                    CollaborationOperationSemantic::ProjectTaskResult { delivery_id },
                    CollaborationOperationResult::TaskResultProjected { projection },
                ) if self.task_result_projections.get(delivery_id) == Some(projection)
                    && operation.operation_id == projection.operation_id => {}
                (CollaborationOperationSemantic::ProjectTaskResult { .. }, _)
                | (_, CollaborationOperationResult::TaskResultProjected { .. }) => {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.task_result.projection_reverse_binding",
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}
