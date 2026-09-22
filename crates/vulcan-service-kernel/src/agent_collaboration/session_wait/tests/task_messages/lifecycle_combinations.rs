//! Stop, MainOnly child-to-parent, unanswered-obligation and late-message combinations.
//! 停止、MainOnly 子到主、未回复义务与迟到消息的组合。

use super::*;
use crate::session_v1_lifecycle::{SessionV1LifecycleError, publish_guarded_edit_generation};
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts;
use vulcan_agent_runtime::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, ASK_QUESTION_TOOL_ID, AskParentArgs, AskQuestionArgs,
    SEND_NOTIFICATION_TOOL_ID, SendNotificationArgs, WAIT_QUESTION_TOOL_ID, WaitQuestionArgs,
};
use vulcan_session_core::v1::collaboration::{
    ActivateChildTaskInput, CollaborationChildTaskTerminalOutcome,
    CollaborationDeliveryFailureReason, CollaborationExecutionId, CollaborationNotificationId,
    CollaborationNotificationState, ReserveChildTaskInput, SetRunControlInput,
    TerminalChildTaskInput,
};
use vulcan_session_core::v1::document::{
    ClientInputSubmission, EditGenerationInput, RootInputReplacement, SessionDocument, TurnOrigin,
};
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

/// Pairs one real embedded child with its parent and returns both handles.
/// 为一个真实嵌入子会话与其父会话建立配对，并返回双方句柄。
///
/// # Parameters
/// 参数
/// `invocation` is the parent's validated native collaboration invocation.
/// `invocation` 是父会话已验证的原生协作调用。
///
/// # Returns
/// 返回值
/// Returns the child id and its registered handle, with the parent's own production child reservation.
/// 返回子会话标识及其已注册句柄，并在父会话上写入其自身生产归约器产生的子任务预留。
async fn embedded_child_of_parent(
    invocation: &ValidatedInvocation,
    parent: &Arc<SessionHandle>,
    label: &str,
) -> (Uuid, Arc<SessionHandle>) {
    // The production creator publishes one real embedded child and its usage ownership.
    // 生产创建器发布一个真实嵌入子任务及其用量归属。
    let child_id = Uuid::new_v4();
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: parent.session_id(),
            parent_session_id: parent.session_id(),
            task_id: child_id,
            execution_id: label.to_owned(),
            prompt: format!("Child work for {label}"),
            display_title: label.to_owned(),
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
                    operation_id: CollaborationOperationId::try_new(label).unwrap(),
                    execution_id: CollaborationExecutionId::try_new(label).unwrap(),
                    task_id: child_identity.task_id.clone().unwrap(),
                    parent_turn_id: parent_turn,
                    child: child_identity,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    (child_id, child)
}

/// A root history edit must refuse while any embedded child still owes collaboration facts.
/// 只要任一嵌入子会话仍欠有协作事实，根历史编辑就必须拒绝。
///
/// # Notes
/// 说明
/// The edit resets `child_tasks`, and startup recovery discovers embedded children through that
/// table, so admitting the edit would discard the child's only recovery entry. The rejection must
/// name the child, not the root: the root itself is settled, which is exactly why checking only the
/// root let this state through before.
/// 编辑会重置 `child_tasks`，而启动恢复正是通过该表发现嵌入子会话，因此放行编辑会丢掉子会话唯一的
/// 恢复入口。拒绝必须指向子会话而不是根：根本身已结清，这正是"只检查根"原先放行该状态的原因。
#[tokio::test]
async fn history_edit_refuses_an_unsettled_embedded_child_and_names_it() {
    // The fixture supplies the isolated host state and registry; the root itself is created by the
    // production creation entry so it starts with no collaboration records at all.
    // 夹具提供隔离的宿主状态与注册表；根会话本身由生产创建入口创建，因此起始没有任何协作记录。
    let (_root, mut invocation, fixture_target, _context) = message_fixture(false).await;
    let project_id = fixture_target
        .storage_locator()
        .unwrap()
        .project_id()
        .to_owned();
    let parent = crate::session_v1_lifecycle::create_and_register_root_session_v1(
        &invocation.controller_runtime,
        &invocation.registry,
        crate::session_v1_lifecycle::CreateRootSessionV1Request {
            session_id: Uuid::new_v4(),
            project_id,
            title: String::from("edit-admission-root"),
            root_submission: ClientInputSubmission::new(
                ClientActionId::try_new("edit-admission-root-input").unwrap(),
                "Root whose history is replaced",
                vec![],
            ),
            now_ms: 3,
        },
    )
    .await
    .unwrap();
    let parent_session_id = parent.session_id();
    // The root starts settled, which is what makes the child the only possible rejection source.
    // 根会话起始即已结清，这正是"只有子会话可能成为拒绝来源"的前提。
    assert!(
        !parent
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap(),
        "a freshly created root must start settled"
    );
    // The production creator publishes a real embedded child under that root.
    // 生产创建器在该根会话下发布真实嵌入子会话。
    let child_id = Uuid::new_v4();
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: parent_session_id,
            parent_session_id,
            task_id: child_id,
            execution_id: String::from("edit-admission-child"),
            prompt: String::from("Child work before the parent history edit"),
            display_title: String::from("edit-admission-child"),
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
    let parent_turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    let child_identity =
        participants::participant_for_handle(&child_invocation, child_id, &child, None)
            .await
            .unwrap();
    assert!(
        child_identity.task_id.is_some(),
        "the embedded child must carry its own task identity"
    );
    // The parent reservation is the production reducer's own record.
    // 父预留是生产归约器自身的记录。
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    source_tool_name: String::from("spawn_agent"),
                    operation_id: CollaborationOperationId::try_new("edit-admission-child")
                        .unwrap(),
                    execution_id: CollaborationExecutionId::try_new("edit-admission-child")
                        .unwrap(),
                    task_id: child_identity.task_id.clone().unwrap(),
                    parent_turn_id: parent_turn,
                    child: child_identity.clone(),
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    // A terminal child task keeps the root settled while the child itself still owes a record.
    // 子任务终结使根会话保持已结清，而子会话自身仍欠有记录。
    // Activation then terminal completion are the domain's own ordered transitions, and they leave
    // the root settled so only the child's own record can reject the edit.
    // 激活与终态完成都是领域自身的顺序转换，它们使根会话保持已结清，因此只有子会话自身的记录才能
    // 拒绝该编辑。
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.activate_child_task(ActivateChildTaskInput {
                    execution_id: CollaborationExecutionId::try_new("edit-admission-child")
                        .unwrap(),
                    operation_id: CollaborationOperationId::try_new("edit-admission-child-active")
                        .unwrap(),
                    task_id: child_identity.task_id.clone().unwrap(),
                    active_at_ms: 6,
                })
            },
        )
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.terminal_child_task(TerminalChildTaskInput {
                    execution_id: CollaborationExecutionId::try_new("edit-admission-child")
                        .unwrap(),
                    operation_id: CollaborationOperationId::try_new(
                        "edit-admission-child-terminal",
                    )
                    .unwrap(),
                    task_id: child_identity.task_id.clone().unwrap(),
                    outcome: CollaborationChildTaskTerminalOutcome::Completed,
                    terminal_at_ms: 7,
                })
            },
        )
        .unwrap();
    // The child asks its parent, which the production port admits as a real embedded-child question.
    // 子会话向其父会话提问，生产端口将其作为真实嵌入子问题接纳。
    let context = admit_real_tool_call(
        &child_invocation,
        ASK_PARENT_TOOL_ID,
        "edit-admission-ask-parent-stream",
        20,
    )
    .await;
    session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        crate::ServiceEventEmitter::new(vec![]),
    )
    .invoke(
        context,
        AgentCollaborationCommand::AskParent(AskParentArgs {
            question: "May the parent history be edited?".into(),
        }),
    )
    .await
    .unwrap();
    // The root's own child-task obligation is gone: activation and terminal completion are the domain's
    // ordered transitions, so no reserved or active child task remains on the root itself.
    // 根本身已没有子任务义务：激活与终态完成是领域规定的顺序转换，因此根上不再留有预留或活动子任务。
    let root_open_child_tasks = parent
        .read(|document| {
            document
                .collaboration_state()
                .child_tasks
                .values()
                .filter(|task| {
                    matches!(
                        task.state,
                        vulcan_session_core::v1::collaboration::CollaborationChildTaskState::Reserved
                            | vulcan_session_core::v1::collaboration::CollaborationChildTaskState::Active
                    )
                })
                .count()
        })
        .unwrap();
    assert_eq!(
        root_open_child_tasks, 0,
        "the root must own no open child task, so only the child's own obligation can reject the edit"
    );
    // The child is the member whose obligation the edit would discard.
    // 子会话是该编辑会丢弃其义务的成员。
    assert!(
        child
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap(),
        "the embedded child must retain its own unsettled obligation"
    );
    // The rejection must name the embedded child rather than the root: before the tree-scoped check the
    // root was the only inspected member, so a child-only obligation could not reject the edit at all.
    // 拒绝必须指向嵌入子会话而不是根：在改为整树校验之前只检查根，因此仅存在于子会话的义务根本无法
    // 拒绝该编辑。
    let generation = parent.read(SessionDocument::segment_generation).unwrap();
    let durable_revision = parent.read(|document| document.durable_revision()).unwrap();
    let result = publish_guarded_edit_generation(
        &invocation.controller_runtime,
        &invocation.registry,
        &parent,
        generation,
        EditGenerationInput {
            target_seq: parent
                .read(|document| document.events().first().unwrap().seq)
                .unwrap(),
            replacement: RootInputReplacement::new("Edited parent", vec![]),
            now_ms: 9,
        },
    )
    .await;
    // The rejection must name a member that genuinely owes collaboration facts, and the embedded child
    // must be one of them: before the tree-scoped check the root was the only inspected member, so an
    // obligation that exists only on the child could not reject the edit at all.
    // 拒绝必须指向真正欠有协作事实的成员，且嵌入子会话必须是其中之一：在改为整树校验之前只检查根，
    // 因此仅存在于子会话的义务根本无法拒绝该编辑。
    let named = match &result {
        Err(SessionV1LifecycleError::UnsettledCollaboration(id)) => *id,
        other => panic!("The history edit discarded an embedded child's obligation: {other:?}"),
    };
    assert!(
        named == child_id
            || parent
                .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
                .unwrap(),
        "the rejection named {named}, which owes no collaboration fact"
    );
    assert_eq!(
        parent.read(SessionDocument::segment_generation).unwrap(),
        generation
    );
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        durable_revision
    );
    // The child remains physically owned by the root, so its recovery entry is intact.
    // 子会话仍由根物理归属，因此其恢复入口完好。
    assert!(
        parent
            .storage_locator()
            .unwrap()
            .embedded_task_tree_locators()
            .unwrap()
            .iter()
            .any(|locator| locator.session_id() == child_id.to_string()),
        "the rejected edit must not detach the embedded child from the root tree"
    );
}

/// Starts the question-execution API on `handle` and prepares its reply ToolCall.
/// 在 `handle` 上启动答疑执行 API 并准备其回答工具调用。
/// Returns the exact context the reply command is validated against.
/// 返回回答命令进行校验时使用的精确上下文。
fn start_question_execution_tool_call_for_parent(
    handle: &Arc<SessionHandle>,
    now_ms: i64,
    stream_label: &str,
) -> VulcanToolInvocationContext {
    let input = handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::QuestionExecution,
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
    let api_call_id = handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    prepare_reply_tool_call_on_active_api(handle, api_call_id, now_ms, stream_label)
}

/// Prepares one reply ToolCall on the handle's already admitted physical API.
/// 在句柄已准入的物理 API 上准备一条回答工具调用。
/// Returns the exact context the reply command is validated against.
/// 返回回答命令进行校验时使用的精确上下文。
fn prepare_reply_tool_call_on_active_api(
    handle: &Arc<SessionHandle>,
    api_call_id: vulcan_session_core::v1::ApiCallId,
    now_ms: i64,
    stream_label: &str,
) -> VulcanToolInvocationContext {
    let request_stream_id = RequestStreamId::try_new(format!("{stream_label}-stream")).unwrap();
    let tool_call_id = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    REPLY_QUESTION_TOOL_ID,
                    ToolPayload::empty(),
                    None,
                    now_ms,
                )
            },
        )
        .unwrap()
        .value;
    let tool_seq = handle
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
        session_id: handle.session_id().to_string(),
        request_stream_id,
        api_call_id,
        tool_call_id,
        tool_seq,
        controller_tool_call_id: format!("{stream_label}-tool"),
        provider_tool_call_id: None,
    }
}

/// Admits one real `wait_question` ToolCall on an invocation's own open turn.
/// 在调用的自身开放回合上准入一条真实的 `wait_question` 工具调用。
/// Returns the exact context the native wait command validates against.
/// 返回原生命令在等待校验时比对的精确上下文。
fn admit_wait_question_tool_call(
    invocation: &ValidatedInvocation,
    stream_label: &str,
    now_ms: i64,
) -> VulcanToolInvocationContext {
    // Reuse the turn's already admitted API when one exists, as the other test helpers do; starting a
    // second physical API while one is active is a domain error, not a test setup detail.
    // 存在已准入 API 时复用它（与其他测试辅助一致）；在已有活动 API 时再启动一个物理 API 属于
    // 领域错误，而不是夹具细节。
    let existing = invocation
        .source_handle
        .read(|document| document.active_api().map(|api| api.api_call_id))
        .unwrap();
    let api_call_id = match existing {
        Some(api_call_id) => api_call_id,
        None => {
            let input = invocation
                .source_handle
                .read(|document| {
                    let through_seq = document.events().last().unwrap().seq;
                    PrepareAndStartApiInput {
                        purpose: ApiPurposeKind::MainGeneration,
                        request: ApiRequestSnapshot::new("openai", "model"),
                        request_budget_evidence:
                            crate::session_runtime_registry_v1::test_provider_request_budget_evidence(
                            ),
                        projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                            segment_generation: document.segment_generation(),
                            document_revision: document.memory_revision(),
                            retained_from_seq: vulcan_session_core::v1::ids::Seq::try_new(1).unwrap(),
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
            invocation
                .source_handle
                .prepare_and_start_api_durable(input)
                .unwrap()
                .value
                .api_call_id
        }
    };
    let request_stream_id = RequestStreamId::try_new(format!("{stream_label}-stream")).unwrap();
    let tool_call_id = invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    WAIT_QUESTION_TOOL_ID,
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
        controller_tool_call_id: format!("{stream_label}-tool"),
        provider_tool_call_id: None,
    }
}

/// A child-to-parent question must round-trip: the parent answers and the child observes that answer.
/// 子到主的问题必须完成往返：父代理作答，子代理观察到该答案。
///
/// # Notes
/// 说明
/// A07 requires genuine bidirectional asking. The existing evidence proves a child can *send* a
/// parent question and that the parent receives it, but never that the answer returns to the asking
/// child, which is what makes the direction usable rather than one-way.
/// A07 要求真正的双向提问。既有证据证明了子代理**能发送**父问题、父代理也能收到，但从未证明
/// 答案会回到提问的子代理——而这正是该方向可用而非单向的原因。
#[tokio::test]
async fn child_question_to_parent_round_trips_the_answer_back_to_the_child() {
    use vulcan_agent_runtime::tooling::collaboration::{REPLY_QUESTION_TOOL_ID, ReplyQuestionArgs};

    let (_root, invocation, parent, _context) = message_fixture(false).await;
    // The production creator publishes one real embedded child owned by this parent.
    // 生产创建器发布一个由该父代理持有真实所有权的嵌入子任务。
    let child_id = Uuid::new_v4();
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: parent.session_id(),
            parent_session_id: parent.session_id(),
            task_id: child_id,
            execution_id: String::from("round-trip-child"),
            prompt: String::from("Child work that must ask its parent"),
            display_title: String::from("round-trip-child"),
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
                    operation_id: CollaborationOperationId::try_new("reserve-round-trip-child")
                        .unwrap(),
                    execution_id: CollaborationExecutionId::try_new("round-trip-child").unwrap(),
                    task_id: child_identity.task_id.clone().unwrap(),
                    parent_turn_id: parent_turn,
                    child: child_identity,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    // The child asks its verified direct parent through the real native port.
    // 子代理通过真实原生端口向已校验的直接父代理提问。
    let runtime = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        crate::ServiceEventEmitter::new(vec![]),
    );
    let ask_context =
        admit_real_tool_call(&child_invocation, ASK_PARENT_TOOL_ID, "round-trip-ask", 20).await;
    let asked = runtime
        .invoke(
            ask_context,
            AgentCollaborationCommand::AskParent(AskParentArgs {
                question: "Which direction should I take?".into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(asked["status"], "ok");
    let question_id =
        CollaborationQuestionId::try_new(asked["question_id"].as_str().unwrap()).unwrap();
    let delivery_id =
        CollaborationDeliveryId::try_new(asked["delivery"]["delivery_id"].as_str().unwrap())
            .unwrap();
    // The parent owns the accepted inbound delivery; the question record itself stays with its asker.
    // 父代理持有已接纳的入站投递；问题记录本身留在提问方。
    let question = child
        .read(|document| document.question_snapshot(&question_id).cloned())
        .unwrap()
        .expect("the child must retain its own question record");
    assert_eq!(question.state, CollaborationQuestionState::Pending);
    parent
        .read(|document| {
            assert_eq!(
                document.collaboration_state().inbound_deliveries[&delivery_id].state,
                CollaborationDeliveryState::Accepted
            );
        })
        .unwrap();
    // The parent's own question execution must be prepared before it may answer. The preparation takes
    // the authoritative **source** snapshot (documented on the domain method), while it is applied to
    // the target document, so the child's record is passed in here exactly as production does.
    // 父代理必须先准备自身的答疑执行才能作答。该准备接收权威**来源**快照（领域方法已注明），
    // 却作用于目标文档，因此这里与生产一致地传入子代理的记录。
    let parent_question = child
        .read(|document| document.question_snapshot(&question_id).cloned())
        .unwrap()
        .expect("the child owns the authoritative question snapshot");
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.prepare_question_execution_before_api(parent_question.clone(), 21),
        )
        .unwrap_or_else(|error| {
            panic!(
                "parent question preparation failed: {error}; active_api={}",
                parent
                    .read(|document| document.active_api().is_some())
                    .unwrap()
            )
        });
    let reply_context =
        start_question_execution_tool_call_for_parent(&parent, 22, "round-trip-reply");
    let answer_body = "Take the archived settlement path.";
    let replied = runtime
        .invoke(
            reply_context,
            AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
                question_id: question_id.as_str().to_owned(),
                answer: answer_body.into(),
                unable_to_answer: false,
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("the parent answer was rejected: {error}"));
    assert_eq!(replied["status"], "ok");
    assert_eq!(
        replied["question_state"].as_str(),
        Some("answered"),
        "the parent answer must settle the child question: {replied}"
    );
    // The answer must be durably bound to the child's own question, not merely echoed.
    // 答案必须可靠绑定到子代理自身的问题，而不仅仅是回显。
    let settled = child
        .read(|document| {
            (
                document.question_snapshot(&question_id).cloned(),
                document
                    .collaboration_state()
                    .outbound_deliveries
                    .get(&delivery_id)
                    .cloned(),
            )
        })
        .unwrap();
    let settled_question = settled
        .0
        .expect("the child must retain its own question record");
    assert_eq!(settled_question.state, CollaborationQuestionState::Answered);
    assert_eq!(
        settled_question
            .answer
            .as_ref()
            .map(|answer| answer.body.as_str()),
        Some(answer_body),
        "the child must hold the exact answer body produced by its parent"
    );
    assert_eq!(
        settled.1.map(|delivery| delivery.settlement_ack.is_some()),
        Some(true),
        "the child must durably acknowledge the settlement of its own question"
    );
    // The child observes the answer through the real wait surface, the usable end of the loop.
    // 子代理通过真实等待入口观察到该答案，这正是该闭环的可用终点。
    let wait_context = admit_wait_question_tool_call(&child_invocation, "round-trip-wait", 23);
    let observed = runtime
        .invoke(
            wait_context,
            AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
                question_id: question_id.as_str().to_owned(),
                after_revision: None,
                timeout_ms: 30_000,
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("the child could not observe its answer: {error}"));
    assert_eq!(
        observed["status"], "ok",
        "an answered child question must be observed at once: {observed}"
    );
    assert_eq!(observed["question_state"].as_str(), Some("answered"));
    assert_eq!(observed["answer"]["body"].as_str(), Some(answer_body));
    // The round trip must not silently complete the child task or close the parent's business turn.
    // 该往返不得静默完成子任务或关闭父代理的业务回合。
    assert!(
        child
            .read(|document| document
                .collaboration_state()
                .outbound_task_messages
                .is_empty())
            .unwrap(),
        "asking the parent must not fabricate a child task intent"
    );
    parent
        .read(|document| {
            assert_eq!(
                document.active_turn().unwrap().turn_id,
                parent_turn,
                "answering a child question must retain the parent turn"
            );
            assert!(
                !document.active_turn().unwrap().closed,
                "answering a child question must not close the parent turn"
            );
        })
        .unwrap();
}
