//! Exact answer delivery evidence from a completed native wait tool.
//! 来自已完成原生等待工具的精确答案交付凭据。

use super::*;
use crate::v1::event::ToolCallEvent;
use crate::v1::ids::ToolCallId;
use serde_json::Value;

/// Reads only the canonical runtime tool-output wrapper, never assistant text or envelope tags.
/// 只读取规范运行时工具输出包装，不读取助手文本或信封标签。
/// Returns the native tool's structured JSON result when the completed invocation succeeded.
/// 已完成调用成功时返回原生工具的结构化 JSON 结果。
fn successful_wait_snapshot(tool: &ToolCallEvent) -> Option<Value> {
    if tool.tool_name != "wait_question" || tool.status != ToolCallStatus::Completed {
        return None;
    }
    // These exact fields are produced by persistent_tool_output_from_result at the runtime boundary.
    // 这些精确字段由运行时边界的 persistent_tool_output_from_result 产生。
    let output = serde_json::to_value(tool.output.as_ref()?).ok()?;
    if output.get("is_error")?.as_bool()? || output.get("tool_name")?.as_str()? != "wait_question" {
        return None;
    }
    serde_json::from_str(output.get("model_output")?.as_str()?).ok()
}

/// Checks a completed wait against its exact arguments and the immutable source-owned answer.
/// 将已完成等待与精确参数及来源拥有的不可变答案核对。
/// Delivery progress can advance independently; settlement identity, participants and body cannot.
/// 交付进度可以独立推进；结算身份、参与者及正文不能改变。
pub(super) fn proves_wait_answer(
    tool: &ToolCallEvent,
    question: &CollaborationQuestionRecord,
) -> bool {
    // Snapshot contains the actual model-visible native result.
    // snapshot 包含模型实际可见的原生结果。
    let Some(snapshot) = successful_wait_snapshot(tool) else {
        return false;
    };
    // Arguments bind the completed tool to the originally requested question.
    // arguments 将已完成工具绑定到原始请求的问题。
    let Ok(arguments) = serde_json::to_value(&tool.input) else {
        return false;
    };
    if arguments.get("question_id").and_then(Value::as_str) != Some(question.question_id.as_str())
        || snapshot.get("question_id").and_then(Value::as_str)
            != Some(question.question_id.as_str())
        || snapshot.get("question_state").and_then(Value::as_str) != Some("answered")
        || snapshot.get("status").and_then(Value::as_str) != Some("ok")
    {
        return false;
    }
    // Answer is the immutable settlement held by the source authority.
    // answer 是来源权威持有的不可变结算。
    let Some(answer) = question.answer.as_ref() else {
        return false;
    };
    // Answer value follows the native question DTO, not a free-form text convention.
    // answer_value 遵循原生问题 DTO，不依赖自由文本约定。
    let Some(answer_value) = snapshot.get("answer") else {
        return false;
    };
    // Returned answer must satisfy the same strict type used by persistence.
    // returned_answer 必须满足持久层使用的相同严格类型。
    let Ok(returned_answer) =
        serde_json::from_value::<CollaborationAnswerRecord>(answer_value.clone())
    else {
        return false;
    };
    // Source contains the complete immutable asker identity.
    // source 包含完整的不可变提问者身份。
    let Ok(source) = serde_json::to_value(&question.source) else {
        return false;
    };
    // Target contains the complete immutable recipient identity.
    // target 包含完整的不可变接收者身份。
    let Ok(target) = serde_json::to_value(&question.target) else {
        return false;
    };
    snapshot.get("source") == Some(&source)
        && snapshot.get("target") == Some(&target)
        && returned_answer.operation_id == answer.operation_id
        && returned_answer.responder == answer.responder
        && returned_answer.body == answer.body
        && returned_answer.settled_at_ms == answer.settled_at_ms
}

impl SessionDocument {
    /// Records source-side answer consumption from one actual completed wait invocation.
    /// 根据一次真实完成的等待调用记录来源侧答案消费。
    /// Pending, incoming, failed and target-side snapshots do not consume a source answer.
    /// 未结、入站、失败及目标侧快照均不消费来源答案。
    pub(in crate::v1::document) fn deliver_completed_question_wait(
        &mut self,
        tool_call_id: ToolCallId,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        // Event is located by its canonical local tool ID, never the tool output's claimed ID.
        // event 通过规范本地工具 ID 定位，绝不依据工具输出自述的 ID。
        let event = self
            .events
            .iter()
            .find(|event| {
                matches!(&event.event,
                    EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id
                )
            })
            .ok_or_else(|| DomainError::reference("tool_call_id", tool_call_id.value()))?;
        // Tool retains the exact persisted invocation selected above.
        // tool 保留上面选中的精确持久调用。
        let EventBody::ToolCall(tool) = &event.event else {
            unreachable!("matched tool event")
        };
        // Snapshot is absent for failed or non-wait tools, which cannot consume an answer.
        // snapshot 在失败或非等待工具中不存在，它们不能消费答案。
        let Some(snapshot) = successful_wait_snapshot(tool) else {
            return Ok(());
        };
        // Question identity is supplied by the native runtime result and still checked against input.
        // 问题身份由原生运行时结果提供，仍须与输入核对。
        let Some(question_id_text) = snapshot.get("question_id").and_then(Value::as_str) else {
            return Ok(());
        };
        // Typed identity rejects malformed result keys before touching source authority.
        // 强类型身份在接触来源权威之前拒绝格式错误的结果键。
        let question_id = CollaborationQuestionId::try_new(question_id_text)?;
        // Foreign target-side query results have no source question in this document.
        // 外部问题的目标侧查询结果在当前文档中没有来源问题。
        let Some(question) = self.question_snapshot(&question_id) else {
            return Ok(());
        };
        if !proves_wait_answer(tool, question)
            || question.answer.as_ref().is_some_and(|answer| {
                answer.delivery.state == CollaborationAnswerDeliveryState::Delivered
            })
        {
            return Ok(());
        }
        // Result sequence is the actual same-event terminal ToolCall position.
        // tool_result_seq 是实际同事件终态 ToolCall 的位置。
        let tool_result_seq = event.seq;
        // Stable operation identity makes durable completion replay safe.
        // 稳定操作身份确保可靠完成可以安全回放。
        let operation_id = CollaborationOperationId::try_new(format!(
            "{}:wait-answer:{}",
            question.delivery_id.as_str(),
            tool_call_id.value()
        ))?;
        self.deliver_answer(DeliverAnswerInput {
            operation_id,
            question_id,
            delivery_kind: CollaborationAnswerDeliveryKind::WaitTool,
            projected_seq: None,
            projected_input_id: None,
            tool_call_id: Some(tool_call_id),
            tool_result_seq: Some(tool_result_seq),
            delivered_at_ms: now_ms,
        })?;
        Ok(())
    }
}
