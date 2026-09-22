//! Wait-combination coverage for questions owned by the source session.
//! 来源会话自有问题的等待组合覆盖。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_QUESTION_TOOL_ID, AskQuestionArgs, CANCEL_QUESTION_TOOL_ID, CancelQuestionArgs,
    WAIT_QUESTION_TOOL_ID, WaitQuestionArgs,
};

/// A timeout must end only that wait: a later wait on the same question must still observe it.
/// 超时只结束该次等待：同一问题上的后续等待仍必须能观察到它。
///
/// The question stays pending and unconsumed, so re-waiting is the documented recovery path.
/// 问题保持待处理且未被消费，因此重新等待是文档化的恢复路径。
#[tokio::test]
async fn question_wait_timeout_does_not_consume_the_question_for_a_later_wait() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "First wait times out, second must still see me.".into(),
        }))
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let first = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 1,
        }))
        .await
        .unwrap();
    // A cursorless wait declares no baseline, so it observes the moment it resumes and reports timeout.
    // 无游标等待不声明基线，因此它观察到恢复的那一刻并报告超时。
    assert_eq!(first["status"], "timeout");
    assert_eq!(first["question_id"], question_id);
    // Observing the question must not settle, cancel or otherwise consume it.
    // 观察该问题不得结算、撤销或以其它方式消费它。
    let after_timeout = dispatch
        .source()
        .read(|document| {
            document
                .collaboration_state()
                .questions
                .values()
                .next()
                .map(|question| (question.state.clone(), question.answer.is_some()))
        })
        .unwrap();
    assert_eq!(
        after_timeout,
        Some((
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Pending,
            false
        ))
    );
    // Resuming from the returned cursor keeps waiting, so this wait must end on its own timeout and
    // still report the unchanged question.
    // 从返回的游标恢复会继续等待，因此本次等待必须在其自身超时后结束，并仍报告未变化的问题。
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let second = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: first["revision"].as_u64(),
            timeout_ms: 50,
        }))
        .await
        .unwrap();
    assert_eq!(second["status"], "timeout");
    assert_eq!(second["question_id"], question_id);
    // Each wait owns its own operation identity; the second is not a replay of the first.
    // 每次等待持有自己的操作身份；第二次不是第一次的重放。
    assert_ne!(
        first["operation_id"], second["operation_id"],
        "each wait attempt must carry its own operation identity"
    );
}

/// A returned cursor resumes the wait instead of waking it: an unchanged question keeps waiting.
/// 返回的游标用于恢复等待而不是立刻唤醒：问题没有变化时必须持续等待。
///
/// # Notes
/// 说明
/// The cursor is the question's own collaboration state, never the owning session's revision. A session
/// revision is advanced by the waiter's own ToolCall, so the previous revision-based cursor made every
/// resumed wait return an unchanged pending snapshot immediately.
/// 游标是问题自身的协作状态，绝不是所属会话的修订号。会话修订会被等待者自身的工具调用推进，因此
/// 先前的基于修订号的游标会让每次恢复的等待都立即返回未变化的待处理快照。
#[tokio::test]
async fn question_wait_cursor_resumes_instead_of_waking_on_session_revision() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "A returned cursor must keep waiting.".into(),
        }))
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    // A cursorless wait declares no baseline, so it ends on its own deadline.
    // 无游标等待不声明基线，因此在其自身截止时间结束。
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let first = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 50,
        }))
        .await
        .unwrap();
    assert_eq!(first["status"], "timeout");
    assert_eq!(first["question_id"], question_id);
    let cursor = first["revision"].as_u64().expect("question wait cursor");
    // Resuming from that exact cursor must keep waiting while the question has not moved.
    // 从该精确游标恢复时，问题未发生任何变化，因此必须继续等待。
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let resumed = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: Some(cursor),
            timeout_ms: 50,
        }))
        .await
        .unwrap();
    assert_eq!(
        resumed["status"], "timeout",
        "an unchanged question must not wake a resumed wait"
    );
    assert_eq!(resumed["revision"].as_u64(), Some(cursor));
    assert_eq!(
        resumed["question_state"],
        serde_json::to_value(
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Pending
        )
        .unwrap()
    );
    // Neither wait may settle, answer or cancel the question.
    // 两次等待都不得结算、回答或撤销该问题。
    assert_eq!(
        dispatch
            .source()
            .read(|document| {
                document
                    .collaboration_state()
                    .questions
                    .values()
                    .next()
                    .map(|question| (question.state.clone(), question.answer.is_some()))
            })
            .unwrap(),
        Some((
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Pending,
            false
        ))
    );
    // A real collaboration change advances the cursor, so the resumed wait returns at once.
    // 真实的协作变化会推进游标，因此恢复的等待立即返回。
    let question_source = dispatch
        .source()
        .read(|document| {
            document
                .collaboration_state()
                .questions
                .values()
                .next()
                .unwrap()
                .source
                .clone()
        })
        .unwrap();
    dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.cancel_question(CancelQuestionInput {
                    operation_id:
                        vulcan_session_core::v1::collaboration::CollaborationOperationId::try_new(
                            "wait-cursor-cancel",
                        )
                        .unwrap(),
                    question_id:
                        vulcan_session_core::v1::collaboration::CollaborationQuestionId::try_new(
                            question_id.clone(),
                        )
                        .unwrap(),
                    requester: question_source,
                    reason: String::from("Cursor must advance on a real change"),
                    cancelled_at_ms: 30,
                })
            },
        )
        .unwrap();
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let changed = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: Some(cursor),
            timeout_ms: 50,
        }))
        .await
        .unwrap();
    assert_eq!(changed["status"], "ok");
    assert_ne!(
        changed["revision"].as_u64(),
        Some(cursor),
        "a settled question must advance the wait cursor"
    );
    assert_eq!(
        changed["question_state"],
        serde_json::to_value(
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Cancelled
        )
        .unwrap()
    );
}

/// Cancelling after an observed wait must settle the question and end every later wait.
/// 观察等待之后撤销必须结算该问题，并结束其后的每次等待。
#[tokio::test]
async fn cancel_after_observation_settles_the_question_and_blocks_further_waiting() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Cancel ends this question for good.".into(),
        }))
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    let waiting = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    // A cursorless wait ends on its own deadline and still returns the cursor for later resumes.
    // 无游标等待在其自身截止时间结束，并仍返回供后续恢复使用的游标。
    let observed = waiting
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 50,
        }))
        .await
        .unwrap();
    assert_eq!(observed["status"], "timeout");
    let observed_cursor = observed["revision"].as_u64();
    // The source owns the question and may cancel it after its own observation.
    // 来源拥有该问题，因此可在自己观察之后撤销它。
    let cancelling = waiting.retool(CANCEL_QUESTION_TOOL_ID).await;
    let cancelled = cancelling
        .invoke(AgentCollaborationCommand::CancelQuestion(
            CancelQuestionArgs {
                question_id: question_id.clone(),
                reason: "Timed out and no longer required".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(cancelled["status"], "ok");
    assert_eq!(cancelled["question_id"], question_id);
    let settled = cancelling
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
    // A later wait resumed from the pre-cancellation cursor returns the settled snapshot at once.
    // 从撤销前游标恢复的后续等待立即返回已结算快照。
    let after_cancel = cancelling.retool(WAIT_QUESTION_TOOL_ID).await;
    let observed_after_cancel = after_cancel
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: observed_cursor,
            timeout_ms: 50,
        }))
        .await
        .unwrap();
    assert_eq!(observed_after_cancel["status"], "ok");
    assert_eq!(observed_after_cancel["question_id"], question_id);
    assert_ne!(observed_after_cancel["revision"].as_u64(), observed_cursor);
    assert_eq!(
        observed_after_cancel["question_state"],
        serde_json::to_value(
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Cancelled
        )
        .unwrap()
    );
}
