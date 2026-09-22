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
    assert_eq!(first["status"], "timeout");
    assert_eq!(first["question_id"], question_id);
    // The timeout must not settle, cancel or otherwise consume the question.
    // 超时不得结算、撤销或以其它方式消费该问题。
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
    // A second wait reuses an independent real ToolCall and still observes the same question.
    // 第二次等待复用一条独立的真实工具调用，并仍能观察到同一问题。
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let second = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 1,
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

/// A caller-supplied cursor is the wait baseline: a baseline below the current revision returns
/// the snapshot immediately, while a timeout reports the revision it reached without advancing it.
/// 调用方给出的游标即等待基线：低于当前修订的基线立即返回快照；超时则报告其到达的修订且不推进它。
///
/// The reported revision is the owning session's durable revision, so re-waiting from it is not a
/// replay of an already-observed question revision; the observable contract is that a timeout never
/// mutates the question and never advances the caller's cursor on its own.
/// 上报的修订是所属会话的可靠修订，因此以它重新等待并不是对已观察问题修订的重放；可观察契约是
/// 超时绝不修改问题，也不自行推进调用方的游标。
#[tokio::test]
async fn question_wait_cursor_selects_the_baseline_without_advancing_the_question() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "A cursor below the current revision returns at once.".into(),
        }))
        .await
        .unwrap();
    let question_id = asked["question_id"].as_str().unwrap().to_owned();
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    // A zero baseline is below the owning session's current revision, so the snapshot returns now.
    // 零基线低于所属会话的当前修订，因此立即返回快照。
    let immediate = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: Some(0),
            timeout_ms: 1,
        }))
        .await
        .unwrap();
    assert_eq!(immediate["status"], "ok");
    assert_eq!(immediate["question_id"], question_id);
    let immediate_revision = immediate["revision"].as_u64().expect("question revision");
    assert!(
        immediate_revision > 0,
        "the reported revision is the owning session's durable revision, not a zero cursor"
    );
    // Re-waiting from that exact revision keeps the question untouched and still reports the snapshot.
    // 从该精确修订重新等待不修改问题，并仍上报快照。
    let dispatch = dispatch.retool(WAIT_QUESTION_TOOL_ID).await;
    let resumed = dispatch
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: Some(immediate_revision),
            timeout_ms: 1,
        }))
        .await
        .unwrap();
    assert_eq!(resumed["question_id"], question_id);
    assert_eq!(
        resumed["question_state"],
        serde_json::to_value(
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Pending
        )
        .unwrap()
    );
    // Neither wait may settle, answer or cancel the question.
    // 两次等待都不得结算、回答或撤销该问题。
    let untouched = dispatch
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
        untouched,
        Some((
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Pending,
            false
        ))
    );
}

/// Cancelling after a timed-out wait must settle the question and end every later wait.
/// 超时等待之后撤销必须结算该问题，并结束其后的每次等待。
#[tokio::test]
async fn cancel_after_timeout_settles_the_question_and_blocks_further_waiting() {
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
    let timed_out = waiting
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 1,
        }))
        .await
        .unwrap();
    assert_eq!(timed_out["status"], "timeout");
    // The source owns the question and may cancel it after its own wait timed out.
    // 来源拥有该问题，因此可在自己的等待超时后撤销它。
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
    // A later wait on the settled question returns the settled snapshot instead of pending.
    // 对已结算问题的后续等待返回已结算快照，而不是待处理状态。
    let after_cancel = cancelling.retool(WAIT_QUESTION_TOOL_ID).await;
    let observed = after_cancel
        .invoke(AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
            question_id: question_id.clone(),
            after_revision: None,
            timeout_ms: 1,
        }))
        .await
        .unwrap();
    assert_eq!(observed["status"], "ok");
    assert_eq!(observed["question_id"], question_id);
    assert_eq!(
        observed["question_state"],
        serde_json::to_value(
            vulcan_session_core::v1::collaboration::CollaborationQuestionState::Cancelled
        )
        .unwrap()
    );
}
