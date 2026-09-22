//! Durable task-message identities, separate from notifications and question transactions.
//! 可靠任务消息身份，与通知及问答事务分离。

use super::*;

/// Business effect requested by one task message; never inferred from its text.
/// 单条任务消息请求的业务效果，绝不从正文推断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationTaskMessageKind {
    /// Queue a distinct business task without binding the currently active target turn.
    /// 排队独立业务任务，不绑定目标当前活动回合。
    TaskStart,
    /// Guide the exact target turn without cancelling it or creating another task.
    /// 引导精确目标回合，不取消它，也不创建另一任务。
    TaskSteer,
}

/// Immutable source-owned intent retained verbatim across delivery and operation retries.
/// 来源拥有的不可变意图，在投递及操作重试中原样保留。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskMessage {
    /// Stable delivery identity shared by both participants.
    /// 双方共用的稳定投递身份。
    pub delivery_id: CollaborationDeliveryId,
    /// Exact source operation that recorded this intent.
    /// 记录此意图的精确来源操作。
    pub operation_id: CollaborationOperationId,
    /// Host-proven sender and original business turn.
    /// 宿主证明的发送者及原业务回合。
    pub source: CollaborationParticipant,
    /// Host-proven receiver; only steering carries an existing target turn.
    /// 宿主证明的接收者；只有方向引导携带已有目标回合。
    pub target: CollaborationParticipant,
    /// Explicit queue or steering effect.
    /// 明确的排队或方向引导效果。
    pub kind: CollaborationTaskMessageKind,
    /// Exact message body, never interpreted as a routing envelope.
    /// 精确正文，绝不解释为路由信封。
    pub body: String,
}

impl CollaborationTaskMessage {
    /// Validates immutable identity, body and kind-specific turn binding; returns no mutation.
    /// 校验不可变身份、正文及类型对应回合绑定，不产生修改。
    pub fn validate(&self) -> Result<(), DomainError> {
        self.source
            .validate("collaboration.task_message.source", true)?;
        self.target
            .validate("collaboration.task_message.target", false)?;
        validate_body("collaboration.task_message.body", &self.body)?;
        if self.source.session_id == self.target.session_id
            || matches!(self.kind, CollaborationTaskMessageKind::TaskStart)
                != self.target.turn_id.is_none()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.target_binding",
            });
        }
        Ok(())
    }
}

/// Original target acceptance receipt; acceptance does not imply execution or completion.
/// 原始目标接纳回执；接纳不代表执行或完成。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskMessageAcceptance {
    /// Immutable accepted source intent.
    /// 接纳的不可变来源意图。
    pub message: CollaborationTaskMessage,
    /// Exact target-side acceptance operation.
    /// 精确目标侧接纳操作。
    pub operation_id: CollaborationOperationId,
    /// Target revision publishing this acceptance.
    /// 发布此次接纳的目标修订。
    pub target_revision: u64,
    /// Target event publishing this acceptance.
    /// 发布此次接纳的目标事件。
    pub event_seq: Seq,
    /// Host acceptance time in Unix milliseconds.
    /// 宿主接纳时间，Unix 毫秒。
    pub accepted_at_ms: i64,
}

impl CollaborationTaskMessageAcceptance {
    /// Validates the exact intent and publication coordinates of this receipt.
    /// 校验此回执的精确意图及发布坐标。
    pub fn validate(&self) -> Result<(), DomainError> {
        self.message.validate()?;
        validate_timestamp(
            "collaboration.task_message.accepted_at_ms",
            self.accepted_at_ms,
        )?;
        if self.target_revision == 0 {
            return Err(DomainError::InvalidStatistics {
                field: "collaboration.task_message.target_revision",
            });
        }
        Ok(())
    }
}

/// Source outbox record with an optional exact target acknowledgement.
/// 来源出站记录，可携带精确目标确认。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskMessageRecord {
    /// Original immutable intent.
    /// 原始不可变意图。
    pub message: CollaborationTaskMessage,
    /// Source creation time, excluded from the idempotent intent comparison.
    /// 来源创建时间，不参与幂等意图比较。
    pub created_at_ms: i64,
    /// Target acceptance once durably acknowledged by the source.
    /// 来源可靠确认后的目标接纳。
    pub acceptance: Option<CollaborationTaskMessageAcceptance>,
}

impl CollaborationTaskMessageRecord {
    /// Validates the source record and requires any acknowledgement to retain its exact intent.
    /// 校验来源记录，并要求所有确认保留其精确意图。
    pub fn validate(&self) -> Result<(), DomainError> {
        self.message.validate()?;
        validate_timestamp(
            "collaboration.task_message.created_at_ms",
            self.created_at_ms,
        )?;
        if let Some(acceptance) = &self.acceptance {
            acceptance.validate()?;
            if acceptance.message != self.message {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.acceptance_binding",
                });
            }
        }
        Ok(())
    }
}

/// Durable source-side receipt for creation or acknowledgement.
/// 创建或确认的可靠来源侧回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskMessageReceipt {
    /// Original result snapshot of this operation.
    /// 此操作的原始结果快照。
    pub record: CollaborationTaskMessageRecord,
    /// Source event publishing this operation.
    /// 发布此操作的来源事件。
    pub event_seq: Seq,
}

/// Terminal delivery effect on the receiver; this never represents business task completion.
/// 接收方投递终态效果；绝不表示业务任务完成。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CollaborationTaskMessageEffectKind {
    /// The queued intent owns a new business turn, before any physical API is dispatched.
    /// 排队意图已拥有新业务回合，尚未分发物理 API。
    Claimed {
        /// Stable business execution identity derived from this delivery.
        /// 从此投递派生的稳定业务执行身份。
        execution_id: CollaborationExecutionId,
        /// New independent business turn.
        /// 新的独立业务回合。
        turn_id: TurnId,
        /// Exact tagged User input identity.
        /// 精确带标签 User 输入身份。
        input_id: PendingInputId,
        /// First provider-visible input, immediately after its claim event.
        /// 首个模型可见输入，紧随认领事件。
        input_seq: Seq,
    },
    /// The guidance was appended to its original task at a legal request boundary.
    /// 在合法请求边界将方向追加到原任务。
    Steered {
        /// Exact original target turn.
        /// 精确原目标回合。
        turn_id: TurnId,
        /// Provider-visible input identity allocated on the target.
        /// 目标分配的模型可见输入身份。
        input_id: PendingInputId,
        /// Sequence of the tagged User input.
        /// 带标签 User 输入的序号。
        input_seq: Seq,
    },
    /// Delivery failed before application; the host preserves the exact classification.
    /// 投递在应用前失败；宿主保留精确分类。
    Failed {
        /// Stable host-provided failure reason.
        /// 宿主提供的稳定失败原因。
        reason: CollaborationDeliveryFailureReason,
    },
}

/// Target-owned immutable application/failure receipt for an accepted task message.
/// 已接纳任务消息的目标所有不可变应用或失败回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTaskMessageEffect {
    /// Exact accepted delivery.
    /// 精确已接纳投递。
    pub delivery_id: CollaborationDeliveryId,
    /// Target operation committing this effect.
    /// 提交此效果的目标操作。
    pub operation_id: CollaborationOperationId,
    /// Delivery outcome distinct from the business task outcome.
    /// 与业务任务结果区分的投递结果。
    pub outcome: CollaborationTaskMessageEffectKind,
    /// Target revision publishing both input and receipt, or only the failure receipt.
    /// 同时发布输入与回执的目标修订，或仅发布失败回执的修订。
    pub target_revision: u64,
    /// Host event recording this effect.
    /// 记录此效果的宿主事件。
    pub event_seq: Seq,
    /// Host effect time in Unix milliseconds.
    /// 宿主效果时间，Unix 毫秒。
    pub occurred_at_ms: i64,
}

impl CollaborationTaskMessageEffect {
    /// Validates this receipt against `acceptance`; does not infer any missing task identity.
    /// 根据 `acceptance` 校验回执；不推断任何缺失任务身份。
    pub fn validate(
        &self,
        acceptance: &CollaborationTaskMessageAcceptance,
    ) -> Result<(), DomainError> {
        acceptance.validate()?;
        validate_timestamp(
            "collaboration.task_message.effect_time",
            self.occurred_at_ms,
        )?;
        if self.delivery_id != acceptance.message.delivery_id
            || self.target_revision <= acceptance.target_revision
            || self.event_seq <= acceptance.event_seq
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.task_message.effect_acceptance",
            });
        }
        if let CollaborationTaskMessageEffectKind::Steered {
            turn_id, input_seq, ..
        } = &self.outcome
        {
            if acceptance.message.kind != CollaborationTaskMessageKind::TaskSteer
                || acceptance.message.target.turn_id != Some(*turn_id)
                || *input_seq <= acceptance.event_seq
                || *input_seq >= self.event_seq
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.steering_effect",
                });
            }
        }
        if let CollaborationTaskMessageEffectKind::Claimed {
            execution_id,
            turn_id,
            input_id,
            input_seq,
        } = &self.outcome
        {
            if acceptance.message.kind != CollaborationTaskMessageKind::TaskStart
                || execution_id.as_str() != format!("{}:task-execution", self.delivery_id.as_str())
                || turn_id.value().checked_add(1) != Some(input_id.value())
                || self.event_seq.value().checked_add(1) != Some(input_seq.value())
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_message.claim_effect",
                });
            }
        }
        Ok(())
    }
}
