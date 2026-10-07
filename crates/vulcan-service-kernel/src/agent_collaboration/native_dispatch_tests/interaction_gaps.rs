//! Interaction regressions through canonical storage and the authenticated wait port.
//! 通过规范存储与已认证等待端口验证交互回归。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_QUESTION_TOOL_ID, AskQuestionArgs, WAIT_AGENTS_RESULT_TOOL_ID, WAIT_RESULT_ACK_KIND,
    WaitAgentsResultArgs,
};
use vulcan_session_core::v1::{
    collaboration::SetRunControlInput,
    document::ToolCompletion,
    event::{ToolCallStatus, ToolPayload},
    ids::SubagentTaskId,
};

/// Persists the exact successful `result` for the fixture's admitted wait invocation.
/// 为夹具准入的等待调用持久化精确成功 `result`。
/// Returns no value; subsequent waits must observe the real durable acknowledgement.
/// 无返回值；后续等待必须观测真实可靠确认。
fn commit_wait(dispatch: &NativeDispatch, result: &Value) {
    // This is the same native tool envelope used by the production bridge.
    // 此信封与生产桥使用的原生工具信封相同。
    let context = dispatch.context();
    dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.start_prepared_tool(context.api_call_id, context.tool_call_id, 30),
        )
        .unwrap();
    dispatch.source().mutate_durable(SessionMutationAdmission::ExistingTurn, SessionFlushReason::ToolBoundary,
        |document| document.finish_tool(context.api_call_id, context.tool_call_id, ToolCompletion {
            output: Some(ToolPayload::try_new(json!({"is_error":false,"tool_name":WAIT_AGENTS_RESULT_TOOL_ID,
                "model_output":"# Agent wait",
                "host_result":{"kind":WAIT_RESULT_ACK_KIND,
                    "payload":{"format":"markdown","status":result["status"],"reports":result["reports"],
                        "interrupted":false,"context_messages_pending":false}}
            })).unwrap()),
            changeset: None, status: ToolCallStatus::Completed, finished_at_ms: 31, external_refs: vec![], generated_resources: vec![],
        })).unwrap();
}

/// Acknowledged absence suppresses repeated reports but never hides the same question's restored owner.
/// 已确认缺失抑制重复报告，但绝不隐藏同一问题恢复后的所有者。
#[tokio::test]
async fn unified_wait_reobserves_question_after_acknowledged_abandonment() {
    // Send through the real port; the offline target has no surviving execution owner.
    // 通过真实端口发送；离线目标没有存活执行所有者。
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Reply after recovery".into(),
        }))
        .await
        .unwrap();
    // Bind a new tool identity before acknowledging its report.
    // 确认报告前绑定新的工具身份。
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let result = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert!(
        result["reports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|report| report["outcome"] == "abandoned")
    );
    commit_wait(&dispatch, &result);
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let quiet = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 1 },
        ))
        .await
        .unwrap();
    assert_eq!(quiet["status"], "no_pending_work");
    assert_eq!(quiet["reports"], json!([]));
    // The real preparation guard represents resumed handling of that same accepted question.
    // 真实准备守卫代表恢复处理同一个已接纳问题。
    let _owner = dispatch_owner::DispatchOwner::acquire(
        &dispatch.invocation.state,
        dispatch.target.session_id(),
    )
    .unwrap();
    let resumed = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 1 },
        ))
        .await
        .unwrap();
    assert_eq!(resumed["status"], "timeout");
    assert_eq!(resumed["pending_work_count"], 1);
    assert!(
        resumed["remaining_work"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |work| work["session_id"] == dispatch.target.session_id().to_string()
                    && work["state"] == "running"
            )
    );
    // A later explicit stop is a different observation and must not be hidden by the earlier report.
    // 之后明确停止属于不同观测，不能被此前报告隐藏。
    drop(_owner);
    dispatch
        .target
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop-after-recovery").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 40,
        })
        .unwrap();
    let stopped = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert!(
        stopped["reports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|report| report["outcome"] == "stopped")
    );
}

/// Corrupt target storage stays unresolved while a different participant's question still arrives.
/// 目标存储损坏保持未确认，同时另一参与者的问题仍正常到达。
#[tokio::test]
async fn unified_wait_isolates_corrupt_target_and_preserves_ready_question() {
    // The child has a real source reservation but an unreadable metadata file.
    // 子任务有真实来源预留，但元数据文件不可读。
    let dispatch = native_dispatch_fixture(false, WAIT_AGENTS_RESULT_TOOL_ID).await;
    let child_id = Uuid::new_v4().to_string();
    let execution = CollaborationExecutionId::try_new("unreadable-child-execution").unwrap();
    let task = SubagentTaskId::try_new(&child_id).unwrap();
    let locator = vulcan_workspace_store::workspace_store::locate_embedded_task_session_v1(
        &dispatch.source().storage_locator().unwrap(),
        &child_id,
    )
    .unwrap();
    std::fs::create_dir_all(locator.session_directory()).unwrap();
    std::fs::write(
        locator.session_directory().join("metadata.json"),
        "{corrupt",
    )
    .unwrap();
    dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    operation_id: CollaborationOperationId::try_new("unreadable-child-reservation")
                        .unwrap(),
                    source_tool_name: "spawn_agent".into(),
                    execution_id: execution.clone(),
                    task_id: task.clone(),
                    parent_turn_id: document.active_turn().unwrap().turn_id,
                    child: CollaborationParticipant::new(
                        &child_id,
                        dispatch.invocation.source_session_id.to_string(),
                        dispatch.invocation.source_locator.project_id(),
                        1,
                        None,
                        Some(task),
                    )
                    .unwrap(),
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    let first = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert_eq!(first["status"], "check_failed");
    assert_eq!(first["unverified_work_count"], 1);
    assert_eq!(first["reports"], json!([]));
    assert!(!first["check_errors"].as_array().unwrap().is_empty());
    dispatch
        .source()
        .read(|document| {
            assert_ne!(
                document.collaboration_state().child_tasks[&execution].state,
                vulcan_session_core::v1::collaboration::CollaborationChildTaskState::Terminal
            )
        })
        .unwrap();
    // A separate incoming question must not be discarded because another target is unreadable.
    // 不能因另一目标不可读而丢弃独立的传入问题。
    let target_turn = dispatch
        .target
        .submit_root_input(
            vulcan_session_core::v1::document::ClientInputSubmission::new(
                vulcan_session_core::v1::ids::ClientActionId::try_new("ask-alongside-corruption")
                    .unwrap(),
                "Ask",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value
        .value();
    let question = super::wake::admit_incoming_question(
        &dispatch,
        dispatch.invocation.source_turn_id,
        target_turn,
    );
    let mixed = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert_eq!(mixed["status"], "messages");
    assert!(
        mixed["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["question_id"] == question.as_str())
    );
    assert_eq!(mixed["unverified_work_count"], 1);
    assert_eq!(mixed["reports"], json!([]));
}

/// Mutual questions yield to an already projected incoming question instead of circularly waiting.
/// 相互提问时优先处理已经投影的传入问题，避免循环等待。
/// Uses real request boundaries so readiness is not confused with an unconsumed inbox event.
/// 使用真实请求边界，避免把待答状态混同为尚未消费的收件事件。
#[tokio::test]
async fn unified_wait_breaks_mutual_question_wait_after_projection() {
    let dispatch = native_dispatch_fixture(false, ASK_QUESTION_TOOL_ID).await;
    let target_turn = dispatch
        .target
        .submit_root_input(
            vulcan_session_core::v1::document::ClientInputSubmission::new(
                vulcan_session_core::v1::ids::ClientActionId::try_new("mutual-target").unwrap(),
                "Target work",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value
        .value();
    let _owner = dispatch_owner::DispatchOwner::acquire(
        &dispatch.invocation.state,
        dispatch.target.session_id(),
    )
    .unwrap();
    dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Need your answer".into(),
        }))
        .await
        .unwrap();
    let dispatch = dispatch.retool(WAIT_AGENTS_RESULT_TOOL_ID).await;
    let question = super::wake::admit_incoming_question(
        &dispatch,
        dispatch.invocation.source_turn_id,
        target_turn,
    );
    let received = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert_eq!(received["status"], "messages");
    assert_eq!(received["pending_work_count"], 1);
    commit_wait(&dispatch, &received);
    dispatch
        .source()
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| document.interrupt_active_api_for_process(40),
        )
        .unwrap();
    projection::project_collaboration_before_api(&dispatch.invocation.state, dispatch.source(), 41)
        .await
        .unwrap();
    // Projection already closed the old API; admit directly rather than interrupting it twice.
    // 投影时旧 API 已经关闭；直接准入，避免重复中断。
    let (context, tool_id) =
        admit_source_tool_call(&dispatch.invocation, WAIT_AGENTS_RESULT_TOOL_ID, 42).await;
    let mut dispatch = dispatch;
    dispatch.invocation.source_api_call_id = Some(context.api_call_id);
    dispatch.invocation.operation_id = stable_operation_id(&context, tool_id);
    dispatch.invocation.source_tool_id = tool_id.to_owned();
    dispatch.tool_id = tool_id.to_owned();
    dispatch.context = context;
    let attention = dispatch
        .invoke(AgentCollaborationCommand::WaitAgentsResult(
            WaitAgentsResultArgs { timeout_ms: 60_000 },
        ))
        .await
        .unwrap();
    assert_eq!(attention["status"], "needs_reply");
    assert_eq!(attention["pending_work_count"], 1);
    assert!(attention["messages"].as_array().unwrap().is_empty());
    assert!(
        attention["pending_questions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["question_id"] == question.as_str())
    );
}
