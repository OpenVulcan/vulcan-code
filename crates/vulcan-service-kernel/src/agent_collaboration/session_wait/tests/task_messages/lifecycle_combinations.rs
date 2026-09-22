//! Stop, MainOnly child-to-parent, unanswered-obligation and late-message combinations.
//! 停止、MainOnly 子到主、未回复义务与迟到消息的组合。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, ASK_QUESTION_TOOL_ID, AskParentArgs, AskQuestionArgs,
    SEND_NOTIFICATION_TOOL_ID, SendNotificationArgs,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationDeliveryFailureReason, CollaborationExecutionId, CollaborationNotificationId,
    CollaborationNotificationState, ReserveChildTaskInput, SetRunControlInput,
};
use vulcan_session_core::v1::document::{ClientInputSubmission, TurnOrigin};
use vulcan_session_core::v1::ids::ClientActionId;

/// Admits one real source ToolCall for `tool_id` on the invocation's open business turn.
/// 在调用的开放业务回合上为 `tool_id` 准入一条真实来源工具调用。
/// Starts a real Core physical API, prepares the tool event and returns its exact context.
/// 启动真实 Core 物理 API、准备工具事件并返回其精确上下文。
async fn admit_real_tool_call(
    invocation: &ValidatedInvocation,
    tool_id: &'static str,
    stream_label: &str,
    now_ms: i64,
) -> VulcanToolInvocationContext {
    let input = invocation
        .source_handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms,
            }
        })
        .unwrap();
    let api_call_id = invocation
        .source_handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    let request_stream_id = RequestStreamId::try_new(stream_label).unwrap();
    let tool_call_id = invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    tool_id,
                    ToolPayload::empty(),
                    None,
                    now_ms,
                )
            },
        )
        .unwrap()
        .value;
    let tool_seq = invocation
        .source_handle
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id => {
                        Some(event.seq)
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    VulcanToolInvocationContext {
        session_id: invocation.source_session_id.to_string(),
        request_stream_id,
        api_call_id,
        tool_call_id,
        tool_seq,
        controller_tool_call_id: format!("lifecycle-combination-{tool_id}"),
        provider_tool_call_id: None,
    }
}

/// A stopped target must terminally fail its queued task instead of auto-resuming business.
/// 已停止目标必须把排队任务终结为失败，绝不自动恢复业务。
/// Every stop state is driven through the real idle-wake entry and asserted on real files.
/// 每个停止状态都通过真实空闲唤醒入口驱动，并在真实文件上断言。
#[tokio::test]
async fn stopped_target_wakeup_fails_queued_task_without_resuming_business() {
    for (control, label) in [
        (CollaborationRunControl::Stopped, "stopped"),
        (CollaborationRunControl::MainOnly, "main-only"),
        (
            CollaborationRunControl::MainAndSubagents,
            "main-and-subagents",
        ),
    ] {
        // Each stop state owns an independent real store and a real native message dispatch.
        // 每个停止状态持有独立的真实存储与真实原生消息分发。
        let (_root, invocation, target, context) = message_fixture(false).await;
        let sent = send(
            &invocation,
            &target,
            context,
            SessionMessageDeliveryMode::Queue,
            "Queued work",
        )
        .await
        .unwrap();
        let delivery_id =
            CollaborationDeliveryId::try_new(sent["delivery_id"].as_str().unwrap()).unwrap();
        // Close the receiving business turn so only the stop policy can prevent queued dispatch.
        // 关闭接收业务回合，使只有停止策略能阻止排队分发。
        target
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.close_turn(
                        vulcan_session_core::v1::event::TurnOutcome::Completed,
                        1,
                        5,
                    )
                },
            )
            .unwrap();
        target
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new(format!("stop-{label}")).unwrap(),
                state: control,
                changed_at_ms: 6,
            })
            .unwrap();
        {
            let mut state = invocation.state.lock().unwrap();
            state.agent_runtime = Some(crate::tests::test_support::test_agent_runtime_bridge());
        }
        let user_inputs = target
            .read(|document| {
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                    .count()
            })
            .unwrap();
        let revision = target.read(|document| document.durable_revision()).unwrap();
        // The real idle-wake entry must settle the queued task, never start it.
        // 真实空闲唤醒入口必须结算排队任务，绝不启动它。
        let started = crate::agent_collaboration::wake_session_tasks_if_idle(
            &crate::ServiceEventEmitter::new(vec![]),
            &invocation.state,
            &invocation.controller_runtime,
            target.session_id(),
        )
        .await
        .unwrap();
        assert!(
            !started,
            "a stopped target must not resume its queued business task"
        );
        target
            .read(|document| {
                assert_eq!(document.collaboration_run_control(), control);
                assert_eq!(
                    document.collaboration_state().task_message_effects[&delivery_id].outcome,
                    CollaborationTaskMessageEffectKind::Failed {
                        reason: CollaborationDeliveryFailureReason::TargetStopped
                    }
                );
                // The original acceptance stays queryable; only dispatch is terminally refused.
                // 原接纳记录保持可查询；只有分发被终态拒绝。
                assert!(
                    document
                        .collaboration_state()
                        .inbound_task_messages
                        .contains_key(&delivery_id)
                );
                assert!(document.active_turn().is_none_or(|turn| turn.closed));
                assert!(document.active_api().is_none());
                assert_eq!(
                    document
                        .events()
                        .iter()
                        .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                        .count(),
                    user_inputs
                );
                assert_eq!(document.durable_revision(), revision + 1);
                assert_eq!(document.memory_revision(), document.durable_revision());
            })
            .unwrap();
        assert!(!crate::webui_session_kernel_has_active_turn(
            &invocation.state.lock().unwrap(),
            target.session_id()
        ));
        // The real session file must carry both the stop control and the terminal failure.
        // 真实会话文件必须同时携带停止控制与终态失败。
        let loaded = load_session_v1_for_runtime(&target.storage_locator().unwrap())
            .unwrap()
            .document;
        assert_eq!(loaded.collaboration_run_control(), control);
        assert_eq!(
            loaded.collaboration_state().task_message_effects[&delivery_id].outcome,
            CollaborationTaskMessageEffectKind::Failed {
                reason: CollaborationDeliveryFailureReason::TargetStopped
            }
        );
    }
}

/// A stopped main session admits its owned child's question without resuming Goal or task.
/// 已停止主会话只接纳其所属子任务的问题，不恢复 Goal，也不新建业务任务。
/// The real embedded child asks through the native port; a task message stays refused.
/// 真实嵌入子任务通过原生端口提问；任务消息仍被拒绝。
#[tokio::test]
async fn main_only_parent_admits_owned_child_question_without_resuming_goal_or_task() {
    use vulcan_agent_runtime::tooling::collaboration::{
        SEND_SESSION_MESSAGE_TOOL_ID, SendSessionMessageArgs,
    };

    let (_root, invocation, parent, _context) = message_fixture(false).await;
    // The production creator publishes one real embedded child and its usage ownership.
    // 生产创建器发布一个真实嵌入子任务及其用量归属。
    let child_id = Uuid::new_v4();
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: parent.session_id(),
            parent_session_id: parent.session_id(),
            task_id: child_id,
            execution_id: String::from("main-only-child"),
            prompt: String::from("Child work under a stopped main session"),
            display_title: String::from("main-only-child"),
            is_resumed: false,
            admitted_at_ms: 5,
        },
    )
    .unwrap();
    let child_invocation = recovery_invocation(
        invocation.state.clone(),
        invocation.controller_runtime.clone(),
        invocation.registry.clone(),
        child_id,
        child.clone(),
    )
    .unwrap();
    // The parent reservation must be the production reducer's own record, not a test flag.
    // 父预留必须是生产归约器自身的记录，而不是测试标记。
    let child_identity =
        participants::participant_for_handle(&child_invocation, child_id, &child, None)
            .await
            .unwrap();
    let parent_turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    source_tool_name: String::from("spawn_agent"),
                    operation_id: CollaborationOperationId::try_new("reserve-main-only-child")
                        .unwrap(),
                    execution_id: CollaborationExecutionId::try_new("main-only-child").unwrap(),
                    task_id: child_identity.task_id.clone().unwrap(),
                    parent_turn_id: parent_turn,
                    child: child_identity,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    // The user Stop freezes main execution while the child keeps its owned QA exception.
    // 用户停止冻结主执行，同时子任务保留其所属答疑例外。
    parent
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("main-only-stop").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: 6,
        })
        .unwrap();
    // Capture the complete Goal/Plan mode, retained turn, human inputs and queued work.
    // 捕获完整 Goal/Plan 模式、保留回合、人类输入与排队工作。
    let before = parent
        .read(|document| {
            (
                document.metadata.mode.clone(),
                document.active_turn().unwrap().turn_id,
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                    .count(),
                document.metadata.input_state.queued_inputs.len(),
            )
        })
        .unwrap();
    let context = admit_real_tool_call(
        &child_invocation,
        ASK_PARENT_TOOL_ID,
        "lifecycle-ask-parent-stream",
        20,
    )
    .await;
    let runtime = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        crate::ServiceEventEmitter::new(vec![]),
    );
    let asked = runtime
        .invoke(
            context.clone(),
            AgentCollaborationCommand::AskParent(AskParentArgs {
                question: "May I continue?".into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(asked["status"], "ok");
    let delivery_id =
        CollaborationDeliveryId::try_new(asked["delivery"]["delivery_id"].as_str().unwrap())
            .unwrap();
    // The exception must retain the stopped control, the Goal facts and the original business task.
    // 该例外必须保留停止控制、Goal 事实以及原业务任务。
    parent
        .read(|document| {
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainOnly
            );
            assert_eq!(
                (
                    document.metadata.mode.clone(),
                    document.active_turn().unwrap().turn_id,
                    document
                        .events()
                        .iter()
                        .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                        .count(),
                    document.metadata.input_state.queued_inputs.len(),
                ),
                before,
                "the MainOnly QA exception changed the parent Goal or business task facts"
            );
            let inbound = &document.collaboration_state().inbound_deliveries[&delivery_id];
            assert_eq!(inbound.state, CollaborationDeliveryState::Accepted);
            assert_eq!(
                inbound.execution_purpose,
                CollaborationExecutionPurpose::InTurnQuestion
            );
            assert_eq!(inbound.bound_turn_id, Some(parent_turn));
            assert!(inbound.settlement.is_none());
            assert!(
                document
                    .collaboration_state()
                    .question_executions
                    .is_empty()
            );
            assert!(document.active_api().is_none());
        })
        .unwrap();
    // The child owns the pending question and the outbound acceptance for that exact delivery.
    // 子任务持有待处理问题以及该精确投递的出站接纳。
    child
        .read(|document| {
            assert!(
                document
                    .collaboration_state()
                    .questions
                    .values()
                    .any(|question| {
                        question.target.session_id == parent.session_id().to_string()
                            && question.state == CollaborationQuestionState::Pending
                    })
            );
            let outbound = document
                .collaboration_state()
                .outbound_deliveries
                .values()
                .find(|delivery| delivery.delivery_id == delivery_id)
                .expect("the child must own the outbound delivery");
            assert_eq!(outbound.state, CollaborationDeliveryState::Accepted);
        })
        .unwrap();
    // A task intent is outside the QA exception, so it can never create a parent business task.
    // 任务意图不在答疑例外之内，因此绝不能新建父业务任务。
    child
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.interrupt_active_api(21),
        )
        .unwrap();
    let task_context = admit_real_tool_call(
        &child_invocation,
        SEND_SESSION_MESSAGE_TOOL_ID,
        "lifecycle-task-message-stream",
        22,
    )
    .await;
    let rejected = runtime
        .invoke(
            task_context,
            AgentCollaborationCommand::SendSessionMessage(SendSessionMessageArgs {
                session_id: parent.session_id().to_string(),
                delivery_mode: SessionMessageDeliveryMode::Queue,
                message: "New parent task".into(),
            }),
        )
        .await
        .unwrap_err();
    assert!(
        rejected.contains("stopped"),
        "a stopped main session must reject new task intent: {rejected}"
    );
    parent
        .read(|document| {
            assert!(
                document
                    .collaboration_state()
                    .inbound_task_messages
                    .is_empty()
            );
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainOnly
            );
            let turn = document.active_turn().unwrap();
            assert_eq!(turn.turn_id, parent_turn);
            assert!(!turn.closed);
            assert_eq!(turn.origin, TurnOrigin::UserRoot);
        })
        .unwrap();
    // The QA exception cannot widen to an unrelated root sender while the same Stop control holds.
    // 同一停止控制下，答疑例外不能扩大到无关的根发送者。
    let parent_deliveries = parent
        .read(|document| document.collaboration_state().inbound_deliveries.len())
        .unwrap();
    invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.interrupt_active_api(23),
        )
        .unwrap();
    let stranger_context = admit_real_tool_call(
        &invocation,
        ASK_QUESTION_TOOL_ID,
        "lifecycle-stranger-ask-stream",
        24,
    )
    .await;
    let stranger = runtime
        .invoke(
            stranger_context,
            AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
                target_session_id: parent.session_id().to_string(),
                question: "Unrelated question".into(),
            }),
        )
        .await
        .unwrap_err();
    assert!(
        stranger.to_ascii_lowercase().contains("child"),
        "a stopped main session must reject an unrelated root sender: {stranger}"
    );
    parent
        .read(|document| {
            assert_eq!(
                document.collaboration_state().inbound_deliveries.len(),
                parent_deliveries
            );
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainOnly
            );
            assert_eq!(document.active_turn().unwrap().turn_id, parent_turn);
        })
        .unwrap();
    assert!(
        invocation
            .source_handle
            .read(|document| document.collaboration_state().questions.is_empty())
            .unwrap(),
        "a rejected stranger question must not be recorded on its source"
    );
    // The real parent file must carry the accepted question and the retained stopped control.
    // 真实父文件必须携带已接纳问题与保留的停止控制。
    let loaded = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        loaded.collaboration_run_control(),
        CollaborationRunControl::MainOnly
    );
    assert_eq!(
        loaded.collaboration_state().inbound_deliveries[&delivery_id].state,
        CollaborationDeliveryState::Accepted
    );
    assert_eq!(loaded.metadata.mode, before.0);
}

/// Unanswered questions and notifications must not block completion, and a later question must not
/// revive the completed business task.
/// 未回答问题与通知不得阻塞完成，后续问题不得复活已完成的业务任务。
/// Completion itself is driven through the real service entry used by the stream finisher.
/// 完成本身通过流收尾器使用的真实服务入口驱动。
#[tokio::test]
async fn unanswered_question_and_notification_do_not_block_completion_and_late_question_does_not_revive()
 {
    use crate::session_runtime_registry_v1::SuccessfulTurnSettlement;

    let dispatch = crate::agent_collaboration::native_dispatch_tests::native_dispatch_fixture(
        false,
        ASK_QUESTION_TOOL_ID,
    )
    .await;
    // The receiving session owns a real running business turn before any inbound obligation.
    // 接收会话在任何入站义务之前持有真实运行中的业务回合。
    dispatch
        .target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("completion-target").unwrap(),
                "Running business",
                vec![],
            ),
            2,
        )
        .unwrap();
    dispatch
        .target
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("completion-running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 3,
        })
        .unwrap();
    let asked = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Unanswered question".into(),
        }))
        .await
        .unwrap();
    assert_eq!(asked["status"], "ok");
    let question_delivery =
        CollaborationDeliveryId::try_new(asked["delivery"]["delivery_id"].as_str().unwrap())
            .unwrap();
    // A one-way notification reaches the same running business session.
    // 一条单向通知到达同一个运行中的业务会话。
    let dispatch = dispatch.retool(SEND_NOTIFICATION_TOOL_ID).await;
    let notified = dispatch
        .invoke(AgentCollaborationCommand::SendNotification(
            SendNotificationArgs {
                target_session_id: dispatch.target.session_id().to_string(),
                message: "No reply required".into(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(notified["status"], "accepted");
    let notification_id =
        CollaborationNotificationId::try_new(notified["notification_id"].as_str().unwrap())
            .unwrap();
    dispatch
        .target
        .read(|document| {
            assert_eq!(
                document.collaboration_state().inbound_deliveries[&question_delivery].state,
                CollaborationDeliveryState::Accepted
            );
            assert!(
                document.collaboration_state().inbound_deliveries[&question_delivery]
                    .settlement
                    .is_none()
            );
            assert_eq!(
                document.collaboration_state().inbound_notifications[&notification_id].state,
                CollaborationNotificationState::Accepted
            );
        })
        .unwrap();
    // The business response finishes, then the real completion gate must close the turn itself.
    // 业务响应先结束，随后真实完成门必须自行关闭该回合。
    super::results::finish_business_response(&dispatch.target);
    assert!(matches!(
        dispatch
            .target
            .complete_successful_turn(9)
            .await
            .unwrap()
            .value,
        SuccessfulTurnSettlement::Completed(_)
    ));
    let user_inputs = dispatch
        .target
        .read(|document| {
            assert!(document.active_turn().is_none_or(|turn| turn.closed));
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::NaturallyIdle
            );
            // Both unanswered obligations survive completion instead of being settled or consumed.
            // 两项未回复义务在完成后继续保留，既未结算也未被消费。
            assert_eq!(
                document.collaboration_state().inbound_deliveries[&question_delivery].state,
                CollaborationDeliveryState::Accepted
            );
            assert!(
                document.collaboration_state().inbound_deliveries[&question_delivery]
                    .settlement
                    .is_none()
            );
            assert_eq!(
                document.collaboration_state().inbound_notifications[&notification_id].state,
                CollaborationNotificationState::Accepted
            );
            assert!(
                document
                    .collaboration_state()
                    .inbound_task_messages
                    .is_empty()
            );
            document
                .events()
                .iter()
                .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                .count()
        })
        .unwrap();
    // A later question arrives after natural completion and must bind as auxiliary QA only.
    // 后续问题在自然完成后到达，必须只绑定为辅助答疑。
    let dispatch = dispatch.retool(ASK_QUESTION_TOOL_ID).await;
    let late = dispatch
        .invoke(AgentCollaborationCommand::AskQuestion(AskQuestionArgs {
            target_session_id: dispatch.target.session_id().to_string(),
            question: "Late question".into(),
        }))
        .await
        .unwrap();
    assert_eq!(late["status"], "ok");
    let late_delivery =
        CollaborationDeliveryId::try_new(late["delivery"]["delivery_id"].as_str().unwrap())
            .unwrap();
    dispatch
        .target
        .read(|document| {
            assert!(document.active_turn().is_none_or(|turn| turn.closed));
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::NaturallyIdle
            );
            let inbound = &document.collaboration_state().inbound_deliveries[&late_delivery];
            assert_eq!(
                inbound.execution_purpose,
                CollaborationExecutionPurpose::NaturallyIdleQuestion
            );
            assert!(inbound.bound_turn_id.is_none());
            assert_eq!(inbound.state, CollaborationDeliveryState::Accepted);
            assert_eq!(
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                    .count(),
                user_inputs,
                "a late question must not revive the completed business task"
            );
            assert!(
                document
                    .collaboration_state()
                    .inbound_task_messages
                    .is_empty()
            );
        })
        .unwrap();
}
