//! Deterministic, lossless model envelopes for immutable task results.
//! 不可变任务结果的确定性、无损模型信封。

use super::*;
use crate::v1::event::TurnOutcome;
use vulcan_ai_contract::host_message::{
    HOST_MESSAGE_MAX_BODY_BYTES, HostMessageEnvelope, HostMessageKind,
};

/// First-line routing and byte coordinates for one result chunk; text follows the newline literally.
/// 结果块首行路由与字节坐标；换行之后为原样正文。
#[derive(Serialize)]
struct TaskResultChunkHeader<'a> {
    /// Fixed chunk representation version, independent of the original final response.
    /// 固定分块表示版本，与原始最终回复独立。
    version: u32,
    /// Original task message, retained without reusing its unbounded length in input action IDs.
    /// 原始任务消息，不将其长身份复用于输入动作 ID。
    delivery_id: &'a CollaborationDeliveryId,
    /// Exact finished target business turn and outcome.
    /// 精确结束的目标业务回合与结果。
    turn_id: TurnId,
    outcome: TurnOutcome,
    /// Original completed Assistant event, absent for stopped or abandoned work.
    /// 原已完成助手事件，停止或放弃时不存在。
    source_text_seq: Option<Seq>,
    /// Zero-based chunk index and total chunk count.
    /// 从零开始的块序号与总块数。
    chunk_index: usize,
    chunk_count: usize,
    /// Half-open UTF-8 byte range and total original body length.
    /// 半开 UTF-8 字节范围及原正文总长度。
    byte_start: usize,
    byte_end: usize,
    total_text_bytes: usize,
}

/// Fixed model payload for a failed task delivery or an unavailable result body.
/// 任务投递失败或结果正文不可披露时使用的固定模型载荷。
#[derive(Serialize)]
struct TaskStatusBody<'a> {
    /// Stable payload representation version.
    /// 稳定载荷表示版本。
    version: u32,
    /// Original immutable delivery identity.
    /// 原始不可变投递身份。
    delivery_id: &'a CollaborationDeliveryId,
    /// Host-classified reason without a fabricated business outcome.
    /// 宿主分类的原因，不伪造业务结果。
    reason: CollaborationDeliveryFailureReason,
}

impl TaskResultChunkHeader<'_> {
    /// Serializes the exact header with JSON escaping; returns a stable domain error on failure.
    /// 使用 JSON 转义序列化精确头部；失败时返回稳定领域错误。
    fn encode(&self) -> Result<String, DomainError> {
        serde_json::to_string(self).map_err(|_| DomainError::CollaborationStateViolation {
            field: "collaboration.task_result.chunk_header",
        })
    }
}

impl CollaborationTaskResult {
    /// Builds the authorized model envelopes from this result and its receiver-local acceptance event.
    /// 从本结果及接收方本地接纳事件构造已授权的模型信封。
    /// `receipt_seq` must be supplied by a verified durable receipt; this pure method grants no authority.
    /// `receipt_seq` 必须来自已核验的可靠回执；此纯方法不授予权限。
    /// Returns ordered bounded chunks, or one redacted failure when `disclosure` forbids the body.
    /// 返回有序且有界的分块；`disclosure` 禁止正文时只返回一份脱敏失败。
    pub fn task_result_envelopes(
        &self,
        receipt_seq: Seq,
        disclosure: CollaborationTaskResultDisclosure,
    ) -> Result<Vec<HostMessageEnvelope>, DomainError> {
        self.validate()?;
        if disclosure == CollaborationTaskResultDisclosure::AuthorizationRevoked {
            if self.terminal.is_none()
                || self.acceptance.message.source.project_id
                    == self.acceptance.message.target.project_id
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.revoked_disclosure_binding",
                });
            }
            return Ok(vec![self.task_status_envelope(
                receipt_seq,
                HostMessageKind::TaskResultUnavailable,
                CollaborationDeliveryFailureReason::AuthorizationRevoked,
            )?]);
        }
        if let CollaborationTaskMessageEffectKind::Failed { reason } = &self.effect.outcome {
            return Ok(vec![self.task_status_envelope(
                receipt_seq,
                HostMessageKind::TaskDeliveryFailed,
                *reason,
            )?]);
        }
        let terminal = self
            .terminal
            .as_ref()
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.missing_business_terminal",
            })?;
        // A missing final response is an explicit non-success outcome, never fabricated assistant text.
        // 缺少最终回复表示明确非成功结果，绝不伪造助手正文。
        let (text, source_text_seq) = match &terminal.final_assistant {
            Some(assistant) => (
                assistant.source_text.text.as_str(),
                Some(assistant.source_text.event_seq),
            ),
            None => ("", None),
        };
        // Every decimal coordinate is at most the byte length (or one for the empty chunk).
        // 每个十进制坐标至多为正文长度，空块则用一作为上界。
        // Reserve the actual serialized upper bound, including escapes in the original delivery ID.
        // 为实际序列化上界预留空间，包括原投递 ID 中的转义。
        let coordinate_bound = text.len().max(1);
        let mut header = TaskResultChunkHeader {
            version: 1,
            delivery_id: &self.effect.delivery_id,
            turn_id: terminal.turn_id,
            outcome: terminal.outcome,
            source_text_seq,
            chunk_index: coordinate_bound,
            chunk_count: coordinate_bound,
            byte_start: coordinate_bound,
            byte_end: coordinate_bound,
            total_text_bytes: text.len(),
        };
        let text_capacity = HOST_MESSAGE_MAX_BODY_BYTES
            .checked_sub(header.encode()?.len() + 1)
            .filter(|capacity| *capacity >= 4)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.chunk_capacity",
            })?;
        // Store only byte ranges while computing the count; each body is copied once into its envelope.
        // 计算块数时仅保存字节范围；每份正文仅在信封中复制一次。
        let mut ranges = Vec::new();
        let mut start: usize = 0;
        loop {
            let mut end = start.saturating_add(text_capacity).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            ranges.push(start..end);
            if end == text.len() {
                break;
            }
            start = end;
        }
        header.chunk_count = ranges.len();
        let mut envelopes = Vec::with_capacity(ranges.len());
        for (index, range) in ranges.into_iter().enumerate() {
            header.chunk_index = index;
            header.byte_start = range.start;
            header.byte_end = range.end;
            let envelope = HostMessageEnvelope {
                // Receiver-local event identity keeps retries stable without concatenating arbitrary identifiers.
                // 接收方本地事件身份保持重试稳定，不拼接任意长标识。
                event_id: format!("task-result:{}:{index}", receipt_seq.value()),
                kind: HostMessageKind::TaskResult,
                source: self.acceptance.message.target.session_id.clone(),
                target: self.acceptance.message.source.session_id.clone(),
                task_id: self
                    .acceptance
                    .message
                    .target
                    .task_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned()),
                question_id: None,
                scope: String::from("task"),
                body: format!("{}\n{}", header.encode()?, &text[range]),
            };
            envelope
                .validate()
                .map_err(|_| DomainError::CollaborationStateViolation {
                    field: "collaboration.task_result.chunk_envelope",
                })?;
            envelopes.push(envelope);
        }
        Ok(envelopes)
    }

    /// Creates one bounded status envelope with the receiver's stable event identity.
    /// 使用接收方稳定事件身份创建一份有界状态信封。
    /// `kind` distinguishes application failure from revoked result disclosure; `reason` is stable.
    /// `kind` 区分应用失败与撤权后结果不可披露；`reason` 是稳定分类。
    fn task_status_envelope(
        &self,
        receipt_seq: Seq,
        kind: HostMessageKind,
        reason: CollaborationDeliveryFailureReason,
    ) -> Result<HostMessageEnvelope, DomainError> {
        let body = serde_json::to_string(&TaskStatusBody {
            version: 1,
            delivery_id: &self.effect.delivery_id,
            reason,
        })
        .map_err(|_| DomainError::CollaborationStateViolation {
            field: "collaboration.task_result.failure_body",
        })?;
        let envelope = HostMessageEnvelope {
            event_id: format!("task-result:{}:0", receipt_seq.value()),
            kind,
            source: self.acceptance.message.target.session_id.clone(),
            target: self.acceptance.message.source.session_id.clone(),
            task_id: self
                .acceptance
                .message
                .target
                .task_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            question_id: None,
            scope: String::from("task"),
            body,
        };
        envelope
            .validate()
            .map_err(|_| DomainError::CollaborationStateViolation {
                field: "collaboration.task_result.failure_envelope",
            })?;
        Ok(envelope)
    }
}
