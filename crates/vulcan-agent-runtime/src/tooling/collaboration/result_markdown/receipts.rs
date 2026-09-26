//! Human-readable public receipts, with routing and internal revisions omitted.
//! 可读公开回执，省略内部路由和修订。

use super::*;
use crate::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, ASK_QUESTION_TOOL_ID, CANCEL_QUESTION_TOOL_ID, LIST_SESSIONS_TOOL_ID,
    REPLY_QUESTION_TOOL_ID, SEND_NOTIFICATION_TOOL_ID, SEND_SESSION_MESSAGE_TOOL_ID,
};
use vulcan_tools_manager::result_markdown::fenced_code;

/// Decodes an exact service receipt; returns a contextual contract failure rather than guessed fields.
/// 解码精确服务回执；返回带上下文的契约失败，不猜测字段。
fn decode<T: serde::de::DeserializeOwned>(tool: &str, output: &Value) -> Result<T, String> {
    serde_json::from_value(output.clone())
        .map_err(|error| format!("{tool} result projection failed: {error}."))
}

/// Renders supported public receipts; unknown tools retain their existing contract.
/// 渲染支持的公开回执；其他工具保留原契约。
/// Takes the tool and service DTO; returns optional Markdown or a projection error.
/// 接收工具及服务传输对象，返回可选 Markdown 或投影错误。
pub(super) fn render(tool: &str, output: &Value) -> Result<Option<String>, String> {
    let body = match tool {
        ASK_PARENT_TOOL_ID
        | ASK_QUESTION_TOOL_ID
        | REPLY_QUESTION_TOOL_ID
        | CANCEL_QUESTION_TOOL_ID => {
            let receipt: QuestionReceipt = decode(tool, output)?;
            let counterpart = if tool == REPLY_QUESTION_TOOL_ID {
                &receipt.source
            } else {
                &receipt.target
            };
            let mut body = format!(
                "# Question: {}\n\nQuestion: {}\n\nSession: {}\n\nState: {}\n",
                inline_code(&receipt.status),
                inline_code(&receipt.question_id),
                inline_code(&counterpart.session_id),
                inline_code(&receipt.question_state)
            );
            if let Some(delivery) = &receipt.delivery {
                body.push_str(&format!(
                    "\nDelivery {}: {}\n",
                    inline_code(&delivery.delivery_id),
                    inline_code(&delivery.state)
                ));
                append_failure(&mut body, &delivery.failure);
            }
            if let Some(answer) = &receipt.answer {
                body.push_str(&format!(
                    "\nAnswer delivery: {}\n",
                    inline_code(&answer.delivery.state)
                ));
                append_failure(&mut body, &answer.delivery.failure);
                // The reply tool already supplied its own body. Other receipts may reveal an existing winning answer.
                // 回复工具已提交自身正文；其他回执可能揭示已有的胜出答案。
                if tool != REPLY_QUESTION_TOOL_ID {
                    if let Some(answer_body) = &answer.body {
                        body.push_str(&format!("\n## Answer\n\n{}\n", fenced_code(answer_body)));
                    }
                }
            }
            if let Some(unable) = receipt.unable_to_answer {
                body.push_str(&format!("\nUnable to answer: {unable}.\n"));
            }
            if let Some(error) = &receipt.repair_error {
                body.push_str(&format!("\nDelivery requires repair: {}. The question is already recorded; do not submit it again.\n", inline_code(error)));
            }
            body
        }
        SEND_SESSION_MESSAGE_TOOL_ID => {
            let receipt: TaskReceipt = decode(tool, output)?;
            let mut body = format!(
                "# Message: {}\n\nDelivery: {}\n\nSession: {}\n\nMode: {}\n\nThis receipt confirms recording or acceptance, not task completion.\n",
                inline_code(&receipt.status),
                inline_code(&receipt.delivery_id),
                inline_code(&receipt.record.message.target.session_id),
                inline_code(&receipt.message_kind)
            );
            append_failure(&mut body, &receipt.delivery_error);
            body
        }
        SEND_NOTIFICATION_TOOL_ID => {
            let receipt: NotificationReceipt = decode(tool, output)?;
            let mut body = format!(
                "# Notification: {}\n\nNotification: {}\n\nDelivery: {}\n\nSession: {}\n",
                inline_code(&receipt.status),
                inline_code(&receipt.notification_id),
                inline_code(&receipt.delivery_id),
                inline_code(&receipt.notification.target.session_id)
            );
            append_failure(&mut body, &receipt.notification.failure);
            append_failure(&mut body, &receipt.delivery_error);
            body
        }
        LIST_SESSIONS_TOOL_ID => {
            let page: SessionPage = decode(tool, output)?;
            let mut body = format!("# Sessions\n\nProject: {}\n", inline_code(&page.project_id));
            if page.sessions.is_empty() {
                body.push_str("\nNo sessions.\n");
            }
            for session in page.sessions {
                body.push_str(&format!(
                    "\n- {} — {}",
                    inline_code(&session.session_id),
                    inline_code(&session.title)
                ));
                if session.archived {
                    body.push_str("; archived");
                }
                if session.unread != 0 {
                    body.push_str("; unread");
                }
                body.push_str(&format!("; updated_at_ms {}", session.updated_at_ms));
            }
            if let Some(cursor) = page.next_cursor {
                body.push_str(&format!("\n\nNext cursor: {}\n", inline_code(&cursor)));
            }
            body.push('\n');
            body
        }
        _ => return Ok(None),
    };
    Ok(Some(body))
}

/// Appends an actual optional diagnostic; absent errors produce no filler.
/// 追加实际可选诊断；不存在错误时不生成占位内容。
fn append_failure(body: &mut String, failure: &Option<String>) {
    if let Some(failure) = failure {
        body.push_str(&format!("\nFailure: {}\n", inline_code(failure)));
    }
}

/// Model-actionable subset of a participant locator.
/// 参与方定位中模型可操作的子集。
#[derive(Deserialize)]
struct Participant {
    /// Session identity accepted by public collaboration tools.
    /// 公开协作工具接受的会话身份。
    session_id: String,
}

/// Public question receipt, including explicit delivery failures and repair information.
/// 公开问题回执，包含明确投递失败和修复信息。
#[derive(Deserialize)]
struct QuestionReceipt {
    /// Actual operation result.
    /// 实际操作结果。
    status: String,
    /// Stable follow-up identity.
    /// 稳定后续操作身份。
    question_id: String,
    /// Current authoritative settlement state.
    /// 当前权威结算状态。
    question_state: String,
    /// Original requester.
    /// 原请求方。
    source: Participant,
    /// Answering participant.
    /// 回答方。
    target: Participant,
    /// Producer-declared optional outbound delivery.
    /// 生产者声明的可选出站投递。
    delivery: Option<QuestionDelivery>,
    /// Full or redacted answer, as authorized by the service.
    /// 服务授权的完整或已遮蔽答案。
    answer: Option<Answer>,
    /// Present only on reply receipts.
    /// 仅在回复回执中存在。
    unable_to_answer: Option<bool>,
    /// Present when a recorded question requires durable repair.
    /// 已记录问题需要可靠修复时存在。
    repair_error: Option<String>,
}

/// Delivery fields required to distinguish recorded, accepted and failed questions.
/// 用于区分已记录、已接纳和失败问题的投递字段。
#[derive(Deserialize)]
struct QuestionDelivery {
    /// Stable delivery correlation.
    /// 稳定投递关联。
    delivery_id: String,
    /// Actual delivery state.
    /// 实际投递状态。
    state: String,
    /// Optional failure code.
    /// 可选失败代码。
    failure: Option<String>,
}

/// Answer subset which also accepts the explicit service redaction variant.
/// 答案子集，同时接受服务明确的遮蔽变体。
#[derive(Deserialize)]
struct Answer {
    /// Absent when the service has revoked body disclosure.
    /// 服务撤销正文披露时不存在。
    body: Option<String>,
    /// Delivery facts remain visible even when the body is redacted.
    /// 即使正文被遮蔽，投递事实仍可见。
    delivery: AnswerDelivery,
}

/// Non-sensitive answer delivery outcome.
/// 不含敏感内容的答案投递结果。
#[derive(Deserialize)]
struct AnswerDelivery {
    /// Actual delivery state.
    /// 实际投递状态。
    state: String,
    /// Explicit failure when known.
    /// 已知时的明确失败。
    failure: Option<String>,
}

/// Receipt returned after recording or delivering a task message.
/// 记录或投递任务消息后返回的回执。
#[derive(Deserialize)]
struct TaskReceipt {
    /// Recorded or accepted, never inferred execution state.
    /// 已记录或已接纳，绝不推断执行状态。
    status: String,
    /// Stable delivery identity.
    /// 稳定投递身份。
    delivery_id: String,
    /// Declared queue/steering semantics.
    /// 声明的排队或引导语义。
    message_kind: String,
    /// Original intent containing the authoritative target.
    /// 含权威目标的原始意图。
    record: TaskRecord,
    /// Recoverable delivery failure after successful recording.
    /// 成功记录后的可恢复投递失败。
    delivery_error: Option<String>,
}

/// Exact nesting of the source-owned message intent.
/// 来源所拥有的消息意图的精确嵌套。
#[derive(Deserialize)]
struct TaskRecord {
    /// Original message, independent of any later acceptance copy.
    /// 原始消息，独立于后续接纳副本。
    message: TargetMessage,
}

/// Message target subset; bodies remain in existing collaboration events.
/// 消息目标子集；正文保留在既有协作事件中。
#[derive(Deserialize)]
struct TargetMessage {
    /// Host-authenticated target.
    /// 宿主认证的目标。
    target: Participant,
}

/// Notification operation receipt.
/// 通知操作回执。
#[derive(Deserialize)]
struct NotificationReceipt {
    /// Actual recorded notification state.
    /// 实际记录的通知状态。
    status: String,
    /// Stable notification identity.
    /// 稳定通知身份。
    notification_id: String,
    /// Stable delivery identity.
    /// 稳定投递身份。
    delivery_id: String,
    /// Source-authoritative notification facts.
    /// 来源权威通知事实。
    notification: NotificationRecord,
    /// Immediate delivery attempt failure, if any.
    /// 本次投递尝试的可选失败。
    delivery_error: Option<String>,
}

/// Notification target and persisted failure, without duplicate body.
/// 通知目标和持久失败，不携带重复正文。
#[derive(Deserialize)]
struct NotificationRecord {
    /// Host-authenticated recipient.
    /// 宿主认证的接收者。
    target: Participant,
    /// Source-persisted failure code.
    /// 来源持久化的失败代码。
    failure: Option<String>,
}

/// Bounded authorized session listing.
/// 有界授权会话列表。
#[derive(Deserialize)]
struct SessionPage {
    /// Scope of every returned session.
    /// 所有返回会话的范围。
    project_id: String,
    /// Authorized indexed entries.
    /// 已授权索引条目。
    sessions: Vec<SessionEntry>,
    /// Exact continuation cursor, absent at the end.
    /// 精确续读游标，结束时不存在。
    next_cursor: Option<String>,
}

/// Selection facts needed to identify and navigate a session.
/// 识别和导航会话所需的选择事实。
#[derive(Deserialize)]
struct SessionEntry {
    /// Public follow-up identity.
    /// 公开后续操作身份。
    session_id: String,
    /// User-visible session title.
    /// 用户可见会话标题。
    title: String,
    /// Whether the session is archived.
    /// 会话是否归档。
    archived: bool,
    /// Whether unread activity exists.
    /// 是否存在未读活动。
    unread: i64,
    /// Exact update time for temporal selection.
    /// 用于按时间选择的精确更新时间。
    updated_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures a persisted task body and its acceptance copy never enter the receipt.
    /// 确保持久任务正文及接纳副本绝不进入回执。
    #[test]
    fn task_receipt_keeps_identity_and_delivery_failure_without_echoing_input() {
        let output = json!({"status":"recorded","delivery_id":"d1","message_kind":"task_start",
            "record":{"message":{"target":{"session_id":"s1"},"body":"unique-original-body"},
                "acceptance":{"message":{"body":"unique-original-body"}}},
            "delivery_error":"target unavailable"});
        let body = render(SEND_SESSION_MESSAGE_TOOL_ID, &output)
            .unwrap()
            .unwrap();
        assert!(body.contains("d1") && body.contains("s1") && body.contains("target unavailable"));
        assert!(body.contains("not task completion"));
        assert!(!body.contains("unique-original-body"));
    }

    /// Checks redacted answer delivery and failed post-record repair remain actionable.
    /// 检查遮蔽答案投递和记录后修复失败仍可操作。
    #[test]
    fn question_receipt_preserves_redacted_answer_and_repair_failure() {
        let output = json!({"status":"repair_required","question_id":"q1","question_state":"answered",
            "source":{"session_id":"caller"},"target":{"session_id":"responder"},
            "delivery":{"delivery_id":"d1","state":"accepted"},
            "answer":{"delivery":{"state":"failed","failure":"grant_revoked"}},
            "repair_error":"storage unavailable"});
        let body = render(ASK_QUESTION_TOOL_ID, &output).unwrap().unwrap();
        for fact in [
            "q1",
            "responder",
            "answered",
            "grant_revoked",
            "storage unavailable",
            "do not submit it again",
        ] {
            assert!(body.contains(fact), "Missing question receipt fact: {fact}");
        }
        assert!(!body.contains("## Answer"));
    }

    /// Keeps the exact opaque cursor and session-selection facts.
    /// 保留精确不透明游标和会话选择事实。
    #[test]
    fn session_page_retains_navigation_and_archived_state() {
        let body = render(
            LIST_SESSIONS_TOOL_ID,
            &json!({"project_id":"p1","sessions":[{
            "session_id":"s1","title":"A`B","archived":true,"unread":1,"updated_at_ms":42
        }],"next_cursor":"opaque-cursor"}),
        )
        .unwrap()
        .unwrap();
        for fact in ["s1", "A`B", "archived", "unread", "42", "opaque-cursor"] {
            assert!(
                body.contains(fact),
                "Missing session selection fact: {fact}"
            );
        }
    }
}
