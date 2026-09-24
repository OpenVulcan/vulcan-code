//! Unified waiting through the real authenticated tool port.
//! 通过真实已认证工具端口验证统一等待。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_QUESTION_TOOL_ID, AskQuestionArgs, WAIT_AGENTS_RESULT_TOOL_ID, WaitAgentsResultArgs,
};

/// Human queueing preserves the wait, while promoting that input into the current turn releases it.
/// 用户排队保留等待，将该输入提升为当前回合插队后解除等待。
#[tokio::test]
async fn unified_wait_distinguishes_human_queue_from_interjection() {
    use vulcan_session_core::v1::config::QueuedInputIdentity;
    use vulcan_session_core::v1::document::ClientInputSubmission;
    use vulcan_session_core::v1::ids::ClientActionId;
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let _owner = dispatch_owner::DispatchOwner::acquire(
        &dispatch.invocation.state,
        dispatch.target.session_id(),
    )
    .unwrap();
    dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Continue working".into(),
        }))
        .await
        .unwrap();
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let queued = dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.enqueue_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("user-next-task").unwrap(),
                        "New direction",
                        vec![],
                    ),
                    30,
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    let first = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 1 },
        ))
        .await
        .unwrap();
    assert_eq!(
        first["status"], "timeout",
        "future user work must not interrupt the active wait"
    );
    let steering = async {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        dispatch
            .source()
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.promote_queued_input_to_pending_turn(
                        QueuedInputIdentity {
                            input_id: queued.input_id,
                            client_action_id: queued.client_action_id,
                        },
                        31,
                    )
                },
            )
            .unwrap();
    };
    let waiting = dispatch.invoke(AgentCollaborationCommand::WaitAgentsResult(
        WaitAgentsResultArgs { timeout_ms: 60_000 },
    ));
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(waiting, steering)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap()["status"], "interrupted");
}

/// Lost queued and claimed tasks both yield canonical terminal results without model-side polling.
/// 丢失执行的排队及已认领任务都产出规范终态结果，无需模型侧轮询。
#[tokio::test]
async fn unified_wait_settles_lost_queued_and_claimed_tasks() {
    use vulcan_agent_runtime::tooling::collaboration::{
        SEND_SESSION_MESSAGE_TOOL_ID, SendSessionMessageArgs, SessionMessageDeliveryMode,
    };
    for claimed in [false, true] {
        let dispatch = native_dispatch_fixture(false, SEND_SESSION_MESSAGE_TOOL_ID).await;
        let sent = dispatch
            .invoke(AgentCollaborationCommand::SendSessionMessage(
                SendSessionMessageArgs {
                    session_id: dispatch.target.session_id().to_string(),
                    message: "Return a result".into(),
                    delivery_mode: SessionMessageDeliveryMode::Queue,
                },
            ))
            .await
            .unwrap();
        let delivery_id = vulcan_session_core::v1::collaboration::CollaborationDeliveryId::try_new(
            sent["delivery_id"].as_str().unwrap(),
        )
        .unwrap();
        if claimed {
            dispatch
                .target
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.claim_task_message_before_api(
                            CollaborationOperationId::try_new("claim-lost-task").unwrap(),
                            delivery_id.clone(),
                            30,
                        )
                    },
                )
                .unwrap();
        }
        let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            dispatch.invoke(AgentCollaborationCommand::WaitAgentsResult(
                WaitAgentsResultArgs { timeout_ms: 60_000 },
            )),
        )
        .await
        .expect("lost task must settle without consuming the timeout")
        .unwrap();
        assert_eq!(result["status"], "messages", "claimed={claimed}: {result}");
        assert!(
            result["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| message["delivery_id"] == delivery_id.as_str()
                    && message["outcome"] == "abandoned"),
            "claimed={claimed}: {result}"
        );
        dispatch
            .target
            .read(|document| {
                assert!(
                    document
                        .collaboration_state()
                        .task_results
                        .contains_key(&delivery_id)
                )
            })
            .unwrap();
    }
}

/// Empty inboxes return immediately despite a long timeout, without following unrelated sessions.
/// 空收件箱在长超时下也立即返回，不跟随无关会话。
#[tokio::test]
async fn unified_wait_without_related_work_returns_immediately() {
    let dispatch = native_dispatch_fixture(false, WAIT_AGENTS_RESULT_TOOL_ID).await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        dispatch.invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        )),
    )
    .await
    .expect("no work must not consume the wait budget")
    .unwrap();
    assert_eq!(result["status"], "no_pending_work");
    assert_eq!(result["pending_work_count"], 0);
    assert!(result.get("revision").is_none());
}

/// A persisted pending question with no physical target owner cannot hold a waiter indefinitely.
/// 持久待答问题在没有物理目标所有者时不能无限占用等待者。
#[tokio::test]
async fn unified_wait_reports_missing_execution_without_terminal_marker() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Report the result.".into(),
        }))
        .await
        .unwrap();
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        dispatch.invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        )),
    )
    .await
    .expect("missing execution must release the waiter")
    .unwrap();
    assert_eq!(result["status"], "messages");
    assert!(
        result["reports"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |report| report["session_id"] == dispatch.target.session_id().to_string()
                    && report["outcome"] == "abandoned"
            )
    );
}

/// A real handoff keeps a question waitable; dropping it wakes the waiter without a terminal event.
/// 真实交接使问题仍可等待；释放交接会唤醒等待方，无需终态事件。
#[tokio::test]
async fn unified_wait_rechecks_lost_owner_and_preserves_pending_question() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let owner = dispatch_owner::DispatchOwner::acquire(
        &dispatch.invocation.state,
        dispatch.target.session_id(),
    )
    .unwrap();
    dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Wait for my answer".into(),
        }))
        .await
        .unwrap();
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let first = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 1 },
        ))
        .await
        .unwrap();
    assert_eq!(
        first["status"], "timeout",
        "in-flight dispatch must not be abandoned"
    );
    let release = async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        drop(owner);
    };
    let waiting = dispatch.invoke(AgentCollaborationCommand::WaitAgentsResult(
        WaitAgentsResultArgs { timeout_ms: 60_000 },
    ));
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(waiting, release)
    })
    .await
    .expect("lost ownership must be detected without a stop notification");
    assert_eq!(result.unwrap()["reports"][0]["outcome"], "abandoned");
    dispatch
        .source()
        .read(|document| {
            assert!(
                document
                    .collaboration_state()
                    .questions
                    .values()
                    .all(|question| question.state == CollaborationQuestionState::Pending)
            )
        })
        .unwrap();
}

/// Explicit user stop wins even while a scheduled wakeup still exists.
/// 即使仍有已调度唤醒，明确的用户停止也优先生效。
#[tokio::test]
async fn unified_wait_reports_user_stop_during_wait() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let _owner = dispatch_owner::DispatchOwner::acquire(
        &dispatch.invocation.state,
        dispatch.target.session_id(),
    )
    .unwrap();
    dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Reply when ready".into(),
        }))
        .await
        .unwrap();
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let stop = async {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        dispatch
            .target
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.set_collaboration_run_control(
                        vulcan_session_core::v1::collaboration::SetRunControlInput {
                            operation_id: CollaborationOperationId::try_new(
                                "stop-during-unified-wait",
                            )
                            .unwrap(),
                            state: CollaborationRunControl::Stopped,
                            changed_at_ms: 40,
                        },
                    )
                },
            )
            .unwrap();
    };
    let waiting = dispatch.invoke(AgentCollaborationCommand::WaitAgentsResult(
        WaitAgentsResultArgs { timeout_ms: 60_000 },
    ));
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(waiting, stop)
    })
    .await
    .expect("user stop must end the wait without another model poll");
    assert_eq!(result.unwrap()["reports"][0]["outcome"], "stopped");
}

/// An already accepted incoming question wakes the shared inbox and remains available after cancellation.
/// 已接纳的传入问题唤醒共享收件箱，取消调用后仍可投递。
#[tokio::test]
async fn unified_wait_receives_incoming_question_without_consuming_it() {
    let dispatch = native_dispatch_fixture(false, WAIT_AGENTS_RESULT_TOOL_ID).await;
    let target_turn = dispatch
        .target
        .submit_root_input(
            vulcan_session_core::v1::document::ClientInputSubmission::new(
                vulcan_session_core::v1::ids::ClientActionId::try_new("incoming-question-root")
                    .unwrap(),
                "Ask for direction",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value
        .value();
    let source_turn = dispatch.invocation.source_turn_id;
    let question_id = super::wake::admit_incoming_question(&dispatch, source_turn, target_turn);
    for _ in 0..2 {
        let result = dispatch
            .invoke(AgentCollaborationCommand::WaitAgentsResult(
                WaitAgentsResultArgs { timeout_ms: 60_000 },
            ))
            .await
            .unwrap();
        assert_eq!(result["status"], "messages");
        assert_eq!(result["context_messages_pending"], true);
    }
    dispatch
        .source()
        .read(|document| {
            assert!(
                document
                    .collaboration_state()
                    .inbound_deliveries
                    .values()
                    .any(|delivery| delivery.question_id == question_id
                        && delivery.state == CollaborationDeliveryState::Accepted)
            );
        })
        .unwrap();
}

/// A missing child is durably terminalized; acknowledgement follows successful tool persistence only.
/// 缺失子任务被可靠终态化；确认仅跟随成功工具持久化。
#[tokio::test]
async fn unified_wait_releases_missing_child_and_acknowledges_only_committed_report() {
    use vulcan_session_core::v1::{
        collaboration::*,
        document::ToolCompletion,
        event::{ToolCallStatus, ToolPayload},
        ids::SubagentTaskId,
    };
    let dispatch = native_dispatch_fixture(false, WAIT_AGENTS_RESULT_TOOL_ID).await;
    let child_id = Uuid::new_v4().to_string();
    let execution_id = CollaborationExecutionId::try_new("lost-child-execution").unwrap();
    let task_id = SubagentTaskId::try_new(&child_id).unwrap();
    dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    operation_id: CollaborationOperationId::try_new("lost-child-reservation")
                        .unwrap(),
                    source_tool_name: "spawn_agent".into(),
                    execution_id: execution_id.clone(),
                    task_id: task_id.clone(),
                    parent_turn_id: document.active_turn().unwrap().turn_id,
                    child: CollaborationParticipant::new(
                        &child_id,
                        dispatch.invocation.source_session_id.to_string(),
                        dispatch.invocation.source_locator.project_id(),
                        1,
                        None,
                        Some(task_id.clone()),
                    )
                    .unwrap(),
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    // A zero internal budget deterministically expires during repair; production admission stays unchanged.
    // 内部零预算确保修复时已到截止点；生产准入规则保持不变。
    let first = result_wait::wait_agents_result(&dispatch.invocation, 0)
        .await
        .unwrap();
    assert_eq!(first["reports"][0]["outcome"], "abandoned");
    dispatch
        .source()
        .read(|document| {
            assert_eq!(
                document.collaboration_state().child_tasks[&execution_id].state,
                CollaborationChildTaskState::Terminal
            );
        })
        .unwrap();
    let retry = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert_eq!(
        retry["reports"], first["reports"],
        "returning a tool value alone must not consume it"
    );
    let context = dispatch.context();
    dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.start_prepared_tool(context.api_call_id, context.tool_call_id, 5),
        )
        .unwrap();
    dispatch.source().mutate_durable(SessionMutationAdmission::ExistingTurn, SessionFlushReason::ToolBoundary,
        |document| document.finish_tool(context.api_call_id, context.tool_call_id, ToolCompletion {
            output: Some(ToolPayload::try_new(json!({"is_error":false,"tool_name":WAIT_AGENTS_RESULT_TOOL_ID,"model_output":first.to_string()})).unwrap()),
            changeset: None, status: ToolCallStatus::Completed, finished_at_ms: 6, external_refs: vec![], generated_resources: vec![],
        })).unwrap();
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let repeated = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert_eq!(repeated["status"], "no_pending_work");
    assert_eq!(repeated["reports"], json!([]));
}
