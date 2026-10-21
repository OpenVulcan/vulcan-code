//! Conversation-only projections of collaboration receipts; runtime JSON remains authoritative.
//! 仅面向会话的协作回执投影；运行时 JSON 保持权威。

use super::{WAIT_AGENTS_RESULT_TOOL_ID, WAIT_RESULT_ACK_KIND};
use serde::Deserialize;
use serde_json::{Value, json};
use vulcan_tools_manager::result_markdown::inline_code;
use vulcan_tools_manager::{VulcanToolHostResult, VulcanToolInvocationResult};

mod receipts;
mod session_read;

/// Projects a public conversation result without modifying the internal command API.
/// 投影公开会话结果，不修改内部命令接口。
/// Takes the tool identity and successful runtime response; returns readable output or a contract error.
/// 接收工具身份和成功的运行时响应，返回可读输出或契约错误。
pub(super) fn project(tool: &str, output: Value) -> Result<VulcanToolInvocationResult, String> {
    if tool == super::READ_SESSION_TOOL_ID {
        return session_read::project(output);
    }
    if tool != WAIT_AGENTS_RESULT_TOOL_ID {
        if let Some(body) = receipts::render(tool, &output)? {
            let mut payload = json!({"format":"markdown"});
            // The returned question, delivery and notification identities join the existing live
            // collaboration state, so the tool card carries their delivery status.
            // 返回的问题、投递与通知身份用于关联既有实时协作状态，使工具卡承载其投递状态。
            for key in ["question_id", "delivery_id", "notification_id"] {
                if let Some(value) = output.get(key).filter(|value| value.is_string()) {
                    payload[key] = value.clone();
                }
            }
            return Ok(
                VulcanToolInvocationResult::from_model_output(Value::String(body))
                    .with_host_result(VulcanToolHostResult {
                        kind: "collaboration_receipt_v1".into(),
                        payload,
                    }),
            );
        }
        return Ok(VulcanToolInvocationResult::from_model_output(output));
    }
    let snapshot: WaitSnapshot = serde_json::from_value(output)
        .map_err(|error| format!("Agent wait result projection failed: {error}."))?;
    let body = wait_markdown(&snapshot);
    let result = VulcanToolInvocationResult::from_model_output(Value::String(body));
    // Only these three source fields acknowledge a report; session and message bodies are unnecessary.
    // 只有这三个来源字段用于确认报告；不需要会话和消息正文。
    let reports: Vec<Value> = snapshot
        .reports
        .iter()
        .map(|report| {
            json!({
                "work_id": report.work_id, "outcome": report.outcome, "reason": report.reason,
            })
        })
        .collect();
    Ok(result.with_host_result(VulcanToolHostResult {
        kind: WAIT_RESULT_ACK_KIND.into(),
        payload: json!({"format":"markdown", "reports": reports, "status":snapshot.status,
            "interrupted":snapshot.interrupted, "context_messages_pending":snapshot.context_messages_pending}),
    }))
}

/// Mirrors the readiness response produced by the service's unified wait owner.
/// 对应服务统一等待所有者产出的就绪响应。
#[derive(Deserialize)]
struct WaitSnapshot {
    /// Observed exit status, distinct from completion of all work.
    /// 观测到的退出状态，区别于全部工作完成。
    status: String,
    /// Whether the caller was interrupted by user input.
    /// 调用方是否被用户输入打断。
    interrupted: bool,
    /// Whether canonical context delivery will supply message bodies.
    /// 规范上下文投递是否将提供消息正文。
    context_messages_pending: bool,
    /// Ready inbox identities, never copies of their bodies.
    /// 就绪收件身份，绝不复制其正文。
    messages: Vec<ReadyMessage>,
    /// Newly observed terminal or delivery failures.
    /// 新观测到的终态或投递失败。
    reports: Vec<WorkReport>,
    /// Number of physically running targets.
    /// 实际运行的目标数量。
    pending_work_count: u64,
    /// Number of targets whose state is not verified.
    /// 状态未得到验证的目标数量。
    unverified_work_count: u64,
    /// Identities and explicit liveness of remaining obligations.
    /// 剩余义务的身份和明确存活状态。
    remaining_work: Vec<RemainingWork>,
    /// Questions the caller must answer.
    /// 调用方必须回答的问题。
    pending_questions: Vec<PendingQuestion>,
    /// Per-target inspection errors that must not imply completion.
    /// 不能被解释为完成的逐目标检查错误。
    check_errors: Vec<CheckError>,
}

/// Represents the declared optional identifiers across the inbox message variants.
/// 表示收件消息各变体中声明的可选身份字段。
#[derive(Deserialize)]
struct ReadyMessage {
    /// Canonical inbox variant.
    /// 规范收件变体。
    kind: String,
    /// Remote participant used for navigation or follow-up.
    /// 用于导航或跟进的远端参与方。
    session_id: String,
    /// Question/answer correlation; absent for other variants.
    /// 问答关联；其他变体不存在。
    question_id: Option<String>,
    /// Notification correlation; absent for other variants.
    /// 通知关联；其他变体不存在。
    notification_id: Option<String>,
    /// Cross-session task/guidance correlation.
    /// 跨会话任务或引导关联。
    delivery_id: Option<String>,
    /// Child execution correlation.
    /// 子任务执行关联。
    execution_id: Option<String>,
    /// Actual result outcome, present for answers and task results.
    /// 实际结果状态，仅问答和任务结果携带。
    outcome: Option<String>,
}

/// One terminal observation and its durable acknowledgement identity.
/// 一条终态观测及其可靠确认身份。
#[derive(Deserialize)]
struct WorkReport {
    /// Immutable work correlation.
    /// 不可变工作关联。
    work_id: String,
    /// Observed participant.
    /// 被观测参与方。
    session_id: String,
    /// Actual terminal outcome.
    /// 实际终态。
    outcome: String,
    /// Optional producer-declared failure reason; unknown remains explicit.
    /// 生产者声明的可选失败原因；未知保持明确。
    reason: Option<String>,
    /// Question correlation when this report concerns a question.
    /// 当报告涉及问题时的问答关联。
    question_id: Option<String>,
}

/// One remaining obligation, preserving unchecked states.
/// 一条剩余义务，保留未检查状态。
#[derive(Deserialize)]
struct RemainingWork {
    /// Immutable work identity.
    /// 不可变工作身份。
    work_id: String,
    /// Target participant.
    /// 目标参与方。
    session_id: String,
    /// Running, unchecked, or check_failed as observed by the owner.
    /// 所有者观测到的运行、未检查或检查失败状态。
    state: String,
    /// Question, child task, or session task.
    /// 问题、子任务或会话任务。
    kind: String,
}

/// A reply obligation already delivered into canonical context.
/// 已投递到规范上下文中的回复义务。
#[derive(Deserialize)]
struct PendingQuestion {
    /// Stable ID required by reply_question.
    /// reply_question 必需的稳定标识。
    question_id: String,
    /// Source participant.
    /// 来源参与方。
    session_id: String,
    /// Declared next action.
    /// 声明的后续操作。
    action: String,
}

/// Target inspection failure which does not settle the work.
/// 不会结算工作的目标检查失败。
#[derive(Deserialize)]
struct CheckError {
    /// Failed work inspection identity.
    /// 检查失败的工作身份。
    work_id: String,
    /// Failed participant lookup.
    /// 查找失败的参与方。
    session_id: String,
    /// Stable failure code.
    /// 稳定失败代码。
    code: String,
    /// Whether inspection may be retried.
    /// 是否可以重试检查。
    retryable: bool,
    /// Actual runtime diagnostic.
    /// 实际运行时诊断。
    detail: String,
}

/// Renders readiness, terminal facts and obligations without duplicating delivered message bodies.
/// 渲染就绪、终态事实及义务，不复制已投递消息正文。
/// Receives the typed owner snapshot and returns English conversation Markdown.
/// 接收所有者的类型化快照，返回英文会话 Markdown。
fn wait_markdown(snapshot: &WaitSnapshot) -> String {
    let mut body = format!("# Agent wait: {}\n", inline_code(&snapshot.status));
    if snapshot.interrupted {
        body.push_str("\nUser input interrupted this wait.\n");
    }
    if snapshot.context_messages_pending {
        body.push_str("\nReady message bodies are delivered separately in conversation context.\n");
    }
    if !snapshot.messages.is_empty() {
        body.push_str("\n## Ready messages\n");
        for message in &snapshot.messages {
            body.push_str(&format!(
                "\n- {}; session {}",
                inline_code(&message.kind),
                inline_code(&message.session_id)
            ));
            for (label, identity) in [
                ("question", &message.question_id),
                ("notification", &message.notification_id),
                ("delivery", &message.delivery_id),
                ("execution", &message.execution_id),
                ("outcome", &message.outcome),
            ] {
                if let Some(identity) = identity {
                    body.push_str(&format!("; {label} {}", inline_code(identity)));
                }
            }
        }
        body.push('\n');
    }
    if !snapshot.reports.is_empty() {
        body.push_str("\n## Settled work\n");
        for report in &snapshot.reports {
            body.push_str(&format!(
                "\n- {}: {}; session {}; reason {}",
                inline_code(&report.work_id),
                inline_code(&report.outcome),
                inline_code(&report.session_id),
                inline_code(report.reason.as_deref().unwrap_or("unknown"))
            ));
            if let Some(question) = &report.question_id {
                body.push_str(&format!("; question {}", inline_code(question)));
            }
        }
        body.push('\n');
    }
    body.push_str(&format!(
        "\nRunning: {}. Unverified: {}.\n",
        snapshot.pending_work_count, snapshot.unverified_work_count
    ));
    for work in &snapshot.remaining_work {
        body.push_str(&format!(
            "\n- {}: {}; kind {}; session {}",
            inline_code(&work.work_id),
            inline_code(&work.state),
            inline_code(&work.kind),
            inline_code(&work.session_id)
        ));
    }
    if !snapshot.pending_questions.is_empty() {
        body.push_str("\n\n## Reply required\n");
        for question in &snapshot.pending_questions {
            body.push_str(&format!(
                "\n- {}: question {}; session {}",
                inline_code(&question.action),
                inline_code(&question.question_id),
                inline_code(&question.session_id)
            ));
        }
    }
    if !snapshot.check_errors.is_empty() {
        body.push_str("\n\n## Inspection failures (work is not known to be finished)\n");
        for error in &snapshot.check_errors {
            body.push_str(&format!(
                "\n- {}; session {}; {}; retryable {}: {}",
                inline_code(&error.work_id),
                inline_code(&error.session_id),
                inline_code(&error.code),
                error.retryable,
                inline_code(&error.detail)
            ));
        }
    }
    body.push('\n');
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The receipt keeps the delivery and notification identities that join the card to its live
    /// delivery state.
    /// 回执保留将工具卡关联到实时投递状态的投递与通知身份。
    #[test]
    fn receipts_carry_the_identities_that_join_live_delivery_state() {
        let output = json!({"operation_id":"op","notification_id":"n1","delivery_id":"d1",
            "revision":1,"status":"accepted",
            "notification":{"target":{"session_id":"s1"},"state":"accepted"},
            "delivery_error":null});
        let result = project(super::super::SEND_NOTIFICATION_TOOL_ID, output).unwrap();
        let host = result.host_result.expect("collaboration receipt");
        assert_eq!(host.kind, "collaboration_receipt_v1");
        assert_eq!(host.payload["notification_id"], "n1");
        assert_eq!(host.payload["delivery_id"], "d1");
        assert!(host.payload.get("question_id").is_none());
    }

    /// Verifies stopped work, unknown liveness and reply actions survive the public projection.
    /// 验证已停止工作、未知存活状态及回复操作经过公开投影后仍保留。
    #[test]
    fn wait_result_preserves_obligations_and_minimal_acknowledgements() {
        let result = project(WAIT_AGENTS_RESULT_TOOL_ID, json!({
            "status":"messages", "interrupted":false, "context_messages_pending":true,
            "messages":[{"kind":"question","question_id":"q1","session_id":"s1"}],
            "reports":[{"work_id":"task:d1","session_id":"s2","outcome":"stopped","reason":"user_stopped"}],
            "pending_work_count":0,"unverified_work_count":1,
            "remaining_work":[{"work_id":"child:e1","session_id":"s3","kind":"child_task","state":"check_failed"}],
            "pending_questions":[{"question_id":"q1","session_id":"s1","action":"reply_question"}],
            "check_errors":[{"work_id":"child:e1","session_id":"s3","code":"target_check_failed","retryable":true,"detail":"unavailable"}],
            "operation_id":"internal-operation"
        })).unwrap();
        let body = result.model_output.as_str().unwrap();
        for fact in [
            "user_stopped",
            "q1",
            "reply_question",
            "Unverified: 1",
            "check_failed",
            "unavailable",
        ] {
            assert!(body.contains(fact), "Missing wait fact: {fact}");
        }
        assert!(!body.contains("internal-operation"));
        let host = result.host_result.unwrap();
        assert_eq!(host.kind, WAIT_RESULT_ACK_KIND);
        assert_eq!(
            host.payload["reports"],
            json!([{"work_id":"task:d1","outcome":"stopped","reason":"user_stopped"}])
        );
        assert_eq!(host.payload["status"], "messages");
    }
}
