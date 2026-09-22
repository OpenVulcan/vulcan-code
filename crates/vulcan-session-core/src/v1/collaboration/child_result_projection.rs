//! Lossless child-result envelopes and immutable parent input receipts.
//! 无损子结果信封及不可变父输入回执。

use super::*;
use vulcan_ai_contract::host_message::{
    HOST_MESSAGE_MAX_BODY_BYTES, HostMessageEnvelope, HostMessageKind,
};

/// Complete projection proof owned by one exact non-success child result.
/// 由一份精确非成功子结果拥有的完整投影凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationChildTaskResultProjection {
    /// Original child execution.
    /// 原子执行。
    pub execution_id: CollaborationExecutionId,
    /// Parent's immutable result event.
    /// 父侧不可变结果事件。
    pub result_seq: Seq,
    /// Original parent business turn receiving the input.
    /// 接收输入的原父业务回合。
    pub parent_turn_id: TurnId,
    /// Complete ordered User input batch.
    /// 完整有序 User 输入批次。
    pub inputs: Vec<CollaborationTaskResultInput>,
    /// Exact projection operation.
    /// 精确投影操作。
    pub operation_id: CollaborationOperationId,
    /// Host event committed after every input chunk.
    /// 在全部输入分块之后提交的宿主事件。
    pub event_seq: Seq,
    /// Atomic revision that publishes the input batch.
    /// 发布输入批次的原子修订。
    pub parent_revision: u64,
    /// Original projection timestamp retained by retries.
    /// 重试保留的原投影时间戳。
    pub projected_at_ms: i64,
}

/// Fixed header describing how to reconstruct the following original JSON bytes.
/// 描述如何重建后续原始 JSON 字节的固定头部。
#[derive(Serialize)]
struct ChildResultChunkHeader {
    /// Envelope payload format version.
    /// 信封载荷格式版本。
    version: u32,
    /// Explicit body encoding independent of routing tags.
    /// 独立于路由标签的明确正文编码。
    encoding: &'static str,
    /// Zero-based part index.
    /// 从零开始的分块索引。
    chunk_index: usize,
    /// Number of parts required for reconstruction.
    /// 重建所需分块总数。
    chunk_count: usize,
    /// Inclusive original byte start.
    /// 包含边界的原字节起点。
    byte_start: usize,
    /// Exclusive original byte end.
    /// 不包含边界的原字节终点。
    byte_end: usize,
    /// Total original UTF-8 byte length.
    /// 原始 UTF-8 总字节长度。
    total_text_bytes: usize,
}

impl ChildResultChunkHeader {
    /// Encodes the typed routing header or returns an explicit serialization error.
    /// 编码强类型路由头部，或返回明确序列化错误。
    fn encode(&self) -> Result<String, DomainError> {
        serde_json::to_string(self).map_err(|_| DomainError::CollaborationStateViolation {
            field: "collaboration.child_result.chunk_header",
        })
    }
}

impl CollaborationChildTaskNonSuccessResult {
    /// Builds complete deterministic envelopes for `task` addressed to its owning parent document.
    /// 为 `task` 构造发送到所属父文档的完整确定性信封。
    /// `parent_session_id` is supplied by the canonical parent; this pure projection grants no authority.
    /// `parent_session_id` 由规范父文档提供；此纯投影不授予权限。
    /// Returns ordered bounded chunks without dropping or truncating any original diagnostic.
    /// 返回有序有界分块，不丢弃或截断任何原诊断。
    pub fn model_envelopes(
        &self,
        task: &CollaborationChildTaskRecord,
        parent_session_id: &str,
    ) -> Result<Vec<HostMessageEnvelope>, DomainError> {
        self.validate(task)?;
        // Preserve the entire typed proof, including original diagnostic identities, as JSON.
        // 将完整强类型凭据保存为 JSON，包括原诊断身份。
        let text = serde_json::to_string(&self.proof).map_err(|_| {
            DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.body",
            }
        })?;
        // Every coordinate is bounded by the body length; reserve its serialized worst case.
        // 每个坐标均以正文长度为上界；预留其序列化最大空间。
        let bound = text.len().max(1);
        // The header identifies JSON fragments so receivers can reconstruct before parsing.
        // 头部明确标识 JSON 片段，使接收方能先重建再解析。
        let mut header = ChildResultChunkHeader {
            version: 1,
            encoding: "child_non_success_json",
            chunk_index: bound,
            chunk_count: bound,
            byte_start: bound,
            byte_end: bound,
            total_text_bytes: text.len(),
        };
        // UTF-8 permits at most four bytes per scalar, guaranteeing progress at every boundary.
        // UTF-8 每个标量最多四字节，保证每个边界均可推进。
        let capacity = HOST_MESSAGE_MAX_BODY_BYTES
            .checked_sub(header.encode()?.len() + 1)
            .filter(|capacity| *capacity >= 4)
            .ok_or(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.chunk_capacity",
            })?;
        // Ranges avoid copying diagnostics before the final envelopes are allocated.
        // 使用范围避免在分配最终信封前复制诊断。
        let mut ranges = Vec::new();
        // Start indexes the original JSON byte sequence.
        // 起点索引原 JSON 字节序列。
        let mut start = 0usize;
        loop {
            // Move the end back only to a complete UTF-8 character boundary.
            // 仅将终点回退到完整 UTF-8 字符边界。
            let mut end = start.saturating_add(capacity).min(text.len());
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
        // Stable local report coordinates keep every retry's message identities bounded.
        // 稳定本地报告坐标使每次重试的消息身份保持有界。
        let mut envelopes = Vec::with_capacity(ranges.len());
        for (index, range) in ranges.into_iter().enumerate() {
            header.chunk_index = index;
            header.byte_start = range.start;
            header.byte_end = range.end;
            // Kind and routing remain identical for every fragment of this one result.
            // 同一结果的所有片段保持相同种类与路由。
            let envelope = HostMessageEnvelope {
                event_id: format!("child-result:{}:{index}", self.event_seq.value()),
                kind: HostMessageKind::TaskResult,
                source: self.proof.child_session_id.clone(),
                target: parent_session_id.to_owned(),
                task_id: Some(task.task_id.as_str().to_owned()),
                question_id: None,
                scope: String::from("task"),
                body: format!("{}\n{}", header.encode()?, &text[range]),
            };
            envelope
                .validate()
                .map_err(|_| DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.envelope",
                })?;
            envelopes.push(envelope);
        }
        Ok(envelopes)
    }
}

impl CollaborationChildTaskResultProjection {
    /// Validates coordinates and full input count against the exact original report.
    /// 按精确原报告校验坐标和完整输入数量。
    /// `task` owns this receipt; returns an error for partial, unordered or unrelated projection.
    /// `task` 拥有此回执；部分、无序或无关投影返回错误。
    pub fn validate(&self, task: &CollaborationChildTaskRecord) -> Result<(), DomainError> {
        validate_timestamp(
            "collaboration.child_result.projected_at_ms",
            self.projected_at_ms,
        )?;
        // Projection cannot exist without its original immutable result.
        // 投影不能在缺少原不可变结果时存在。
        let result =
            task.non_success_result
                .as_ref()
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.projection_report",
                })?;
        if self.execution_id != task.execution_id
            || self.result_seq != result.event_seq
            || self.parent_turn_id != task.parent_turn_id
            || self.parent_revision <= result.parent_revision
            || self.parent_revision > JS_SAFE_INTEGER_MAX
            || self.inputs.is_empty()
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "collaboration.child_result.projection_binding",
            });
        }
        // Each input identity is unique and lies strictly between acceptance and its projection event.
        // 每个输入身份唯一，并严格位于接纳与投影事件之间。
        let mut previous = self.result_seq;
        // Duplicate input IDs would let one chunk impersonate another.
        // 重复输入 ID 会使一份分块冒充另一份。
        let mut identities = BTreeSet::new();
        for input in &self.inputs {
            if input.input_seq <= previous
                || input.input_seq >= self.event_seq
                || !identities.insert(input.input_id)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.projection_order",
                });
            }
            previous = input.input_seq;
        }
        Ok(())
    }
}

impl CollaborationState {
    /// Validates both directions between projection receipts and their immutable operations.
    /// 校验投影回执与不可变操作之间的双向关联。
    /// Returns an error for orphaned, substituted or conflicting receipt identities.
    /// 孤立、替换或冲突的回执身份返回错误。
    pub(super) fn validate_child_result_projection_operations(&self) -> Result<(), DomainError> {
        for task in self.child_tasks.values() {
            let Some(projection) = &task.non_success_projection else {
                continue;
            };
            if !self
                .operations
                .get(&projection.operation_id)
                .is_some_and(|operation| {
                    operation.event_seq == projection.event_seq
                        && operation.semantic
                            == (CollaborationOperationSemantic::ProjectChildTaskNonSuccess {
                                execution_id: task.execution_id.clone(),
                                result_seq: projection.result_seq,
                            })
                        && operation.result
                            == (CollaborationOperationResult::ChildTask {
                                execution_id: task.execution_id.clone(),
                                task_id: task.task_id.clone(),
                                state: CollaborationChildTaskState::Terminal,
                            })
                })
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "collaboration.child_result.projection_operation",
                });
            }
        }
        for operation in self.operations.values() {
            if let CollaborationOperationSemantic::ProjectChildTaskNonSuccess {
                execution_id,
                result_seq,
            } = &operation.semantic
            {
                if !self
                    .child_tasks
                    .get(execution_id)
                    .and_then(|task| task.non_success_projection.as_ref())
                    .is_some_and(|projection| {
                        projection.operation_id == operation.operation_id
                            && projection.event_seq == operation.event_seq
                            && projection.result_seq == *result_seq
                    })
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "collaboration.child_result.projection_operation_owner",
                    });
                }
            }
        }
        Ok(())
    }
}
