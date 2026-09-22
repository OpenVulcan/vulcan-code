//! Real port dispatch for the collaboration commands that previously had no coverage.
//! 此前缺少覆盖的协作命令的真实端口分发测试。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_QUESTION_TOOL_ID, AskQuestionArgs, CANCEL_QUESTION_TOOL_ID, CancelQuestionArgs,
    LIST_SESSIONS_TOOL_ID, ListSessionsArgs, READ_SESSION_TOOL_ID, ReadSessionArgs,
    SEND_NOTIFICATION_TOOL_ID, SendNotificationArgs, WAIT_QUESTION_TOOL_ID, WaitQuestionArgs,
};

/// Asking one authorized session must reach the durable question record through the real port.
/// 向一个获授权会话提问必须通过真实端口到达可靠问题记录。
#[tokio::test]
async fn native_dispatch_ask_question_records_question_and_delivery() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let observed = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Which direction should this take?".into(),
        }))
        .await
        .unwrap();
    assert_eq!(observed["status"], "ok");
    assert_eq!(observed["operation_id"], dispatch.operation_id());
    assert_eq!(observed["question_id"].as_str().is_some(), true);
    // The source must own exactly one durable outbound delivery for this question.
    // 来源必须为该问题持有且仅持有一条可靠出站投递。
    let (deliveries, questions) = dispatch
        .source()
        .read(|document| {
            (
                document.collaboration_state().outbound_deliveries.len(),
                document.collaboration_state().questions.len(),
            )
        })
        .unwrap();
    assert_eq!((deliveries, questions), (1, 1));
}

/// Listing and reading sessions must project their real bounded shapes through the port.
/// 列出与读取会话必须通过端口投影其真实有界形状。
#[tokio::test]
async fn native_dispatch_list_and_read_session_project_bounded_ranges() {
    let dispatch = native_dispatch_fixture(false, LIST_SESSIONS_TOOL_ID).await;
    let listed = dispatch
        .invoke(AgentCollaborationCommand::ListSessions(ListSessionsArgs {
            limit: 10,
            cursor: None,
        }))
        .await
        .unwrap();
    assert_eq!(listed["status"], "ok");
    assert_eq!(
        listed["project_id"],
        dispatch.invocation.source_locator.project_id()
    );
    assert_eq!(listed["operation_id"], dispatch.operation_id());
    let sessions = listed["sessions"].as_array().expect("session list");
    let target_entry = sessions
        .iter()
        .find(|entry| entry["session_id"] == dispatch.target.session_id().to_string())
        .expect("the authorized target session must be listed");
    assert_eq!(target_entry["archived"], false);
    assert_eq!(
        target_entry["project_id"],
        dispatch.invocation.source_locator.project_id()
    );
    // Reading the authorized target must return its own durable events.
    // 读取获授权目标必须返回其自身可靠事件。
    let dispatch = dispatch.retool(READ_SESSION_TOOL_ID).await;
    let read = dispatch
        .invoke(AgentCollaborationCommand::ReadSession(ReadSessionArgs {
            session_id: dispatch.target.session_id().to_string(),
            after_seq: None,
            limit: 10,
        }))
        .await
        .unwrap();
    assert_eq!(read["status"], "ok");
    assert_eq!(read["session_id"], dispatch.target.session_id().to_string());
    assert_eq!(read["operation_id"], dispatch.operation_id());
    // The fixture target owns no history yet, so an exact empty bounded range is the real result.
    // 夹具目标尚无历史，因此精确的有界空区间就是真实结果。
    assert!(read["events"].as_array().expect("event range").is_empty());
    assert!(read["document_revision"].as_u64().is_some());
    assert!(read["next_seq"].as_u64().is_some());
}

/// A one-way notification must be durably accepted without creating a reply obligation.
/// 单向通知必须被可靠接纳，且不创建回复义务。
#[tokio::test]
async fn native_dispatch_send_notification_records_without_reply_contract() {
    let dispatch = native_dispatch_fixture(false, SEND_NOTIFICATION_TOOL_ID).await;
    let observed = dispatch
        .invoke(AgentCollaborationCommand::SendNotification(
            SendNotificationArgs {
                target_session_id: dispatch.target.session_id().to_string(),
                message: "Build finished; no reply needed.".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(observed["status"], "accepted");
    assert_eq!(observed["operation_id"], dispatch.operation_id());
    assert!(observed["notification_id"].as_str().is_some());
    // A notification never owns a question record or an answer obligation.
    // 通知绝不持有问题记录或回答义务。
    let questions = dispatch
        .source()
        .read(|document| document.collaboration_state().questions.len())
        .unwrap();
    assert_eq!(questions, 0);
}

/// Cancelling the source's own pending question must settle it through the real port.
/// 撤销来源自己的待处理问题必须通过真实端口完成结算。
#[tokio::test]
async fn native_dispatch_cancel_question_settles_pending_question() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Answer before I cancel.".into(),
        }))
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    let dispatch = dispatch.retool(CANCEL_QUESTION_TOOL_ID).await;
    let cancelled = dispatch
        .invoke(AgentCollaborationCommand::CancelQuestion(
            CancelQuestionArgs {
                question_id: question_id.clone(),
                reason: "No longer required".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(cancelled["status"], "ok");
    assert_eq!(cancelled["question_id"], question_id);
    assert_eq!(cancelled["operation_id"], dispatch.operation_id());
    // The durable source record must be the sole authority for the settled state.
    // 可靠来源记录必须是该结算状态的唯一权威。
    let settled = dispatch
        .source()
        .read(|document| {
            document
                .collaboration_state()
                .questions
                .values()
                .next()
                .map(|question| question.state.clone())
        })
        .unwrap();
    assert_eq!(
        settled,
        Some(vulcan_session_core::v1::collaboration::CollaborationQuestionState::Cancelled)
    );
}

/// Waiting on the caller's own unresolved question must return a bounded pending snapshot.
/// 等待调用方自己未解决的问题必须返回有界的待处理快照。
#[tokio::test]
async fn native_dispatch_wait_question_returns_bounded_pending_snapshot() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Waiting keeps the original cursor.".into(),
        }))
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let waited = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 1,
        }))
        .await
        .unwrap();
    // A timeout returns the pending snapshot and must not settle or consume the question.
    // 超时返回待处理快照，且不得结算或消费该问题。
    assert_eq!(waited["status"], "timeout");
    assert_eq!(waited["question_id"], question_id);
    assert_eq!(waited["operation_id"], dispatch.operation_id());
    let still_pending = dispatch
        .source()
        .read(|document| {
            document
                .collaboration_state()
                .questions
                .values()
                .next()
                .map(|question| question.state.clone())
        })
        .unwrap();
    assert_eq!(
        still_pending,
        Some(vulcan_session_core::v1::collaboration::CollaborationQuestionState::Pending)
    );
}
