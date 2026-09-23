//! Real port coverage for the `wait_agent` incoming-question wake path.
//! `wait_agent` 传入问题唤醒路径的真实端口覆盖。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::WAIT_AGENT_TOOL_ID;
use vulcan_session_core::v1::document::ClientInputSubmission;
use vulcan_session_core::v1::ids::{ClientActionId, TurnId};

/// Records one real accepted question from `target` into the waiting source session.
/// 把来自 `target` 的一条真实已接纳问题记入等待中的来源会话。
/// Returns the durable question identity so callers can assert the exact wake snapshot.
/// 返回可靠问题身份，供调用方断言精确唤醒快照。
fn admit_incoming_question(
    dispatch: &NativeDispatch,
    source_turn: u64,
    target_turn: u64,
) -> CollaborationQuestionId {
    let invocation = &dispatch.invocation;
    // The asking participant carries its indexed project's authority, including foreign scopes.
    // 提问参与者携带其索引项目的权威身份，包含跨项目范围。
    let asking_project = dispatch
        .target
        .storage_locator()
        .unwrap()
        .project_id()
        .to_owned();
    let asking = CollaborationParticipant::new(
        dispatch.target.session_id().to_string(),
        dispatch.target.session_id().to_string(),
        asking_project,
        1,
        Some(TurnId::try_new(target_turn).unwrap()),
        None,
    )
    .unwrap();
    let waiting = CollaborationParticipant::new(
        invocation.source_session_id.to_string(),
        invocation.source_session_id.to_string(),
        invocation.source_locator.project_id(),
        1,
        Some(TurnId::try_new(source_turn).unwrap()),
        None,
    )
    .unwrap();
    let question_id =
        CollaborationQuestionId::try_new(format!("cq1/v1:{}:wake", dispatch.target.session_id()))
            .unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("wake-delivery").unwrap();
    dispatch
        .target
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .create_question(CreateQuestionInput {
                        operation_id: CollaborationOperationId::try_new("wake-question").unwrap(),
                        question_id: question_id.clone(),
                        delivery_id: delivery_id.clone(),
                        source: asking.clone(),
                        target: waiting.clone(),
                        body: "Do you have the direction?".into(),
                        execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                        created_at_ms: 3,
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    invocation
        .source_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_delivery(AcceptDeliveryInput {
                        operation_id: CollaborationOperationId::try_new("wake-accept").unwrap(),
                        question_id: question_id.clone(),
                        delivery_id: delivery_id.clone(),
                        source: asking,
                        target: waiting,
                        bound_turn_id: Some(TurnId::try_new(source_turn).unwrap()),
                        execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                        accepted_at_ms: 4,
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    question_id
}

/// The `wait_agent` port must return the exact accepted incoming question without consuming it.
/// `wait_agent` 端口必须返回精确的已接纳传入问题，且不消费它。
#[tokio::test]
async fn wait_agent_returns_incoming_question_without_consuming_it() {
    // The wake context is admitted for the real `wait_agent` tool identity.
    // 唤醒上下文按真实 `wait_agent` 工具身份准入。
    let dispatch = native_dispatch_fixture(false, WAIT_AGENT_TOOL_ID).await;
    let mut dispatch = dispatch;
    // The fixture already owns the source's open business turn; reuse it instead of opening another.
    // 夹具已持有来源的开放业务回合；复用它，而不是再开一个。
    let source_turn = dispatch
        .source()
        .read(|document| document.active_turn().unwrap().turn_id.value())
        .unwrap();
    dispatch.invocation.source_turn_id = source_turn;
    let target_turn = dispatch
        .target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("wait-agent-asker-root").unwrap(),
                "Ask the waiter for direction",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value
        .value();
    let question_id = admit_incoming_question(&dispatch, source_turn, target_turn);
    let source_revision = dispatch
        .source()
        .read(|document| document.memory_revision())
        .unwrap();
    let runtime = session_v1_native_agent_collaboration_runtime(
        dispatch.invocation.state.clone(),
        crate::ServiceEventEmitter::new(vec![]),
    );
    let woken = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.wait_for_incoming_question(dispatch.context()),
    )
    .await
    .expect("the wake port must return an already accepted question")
    .unwrap();
    assert_eq!(woken["status"], "incoming");
    assert_eq!(woken["question_id"], question_id.as_str());
    assert_eq!(woken["question"], "Do you have the direction?");
    assert_eq!(woken["delivery_id"], "wake-delivery");
    // Reading the accepted question must not change the source revision or consume the delivery.
    // 读取已接纳问题不得改变来源修订，也不得消费该投递。
    dispatch
        .source()
        .read(|document| {
            assert_eq!(document.memory_revision(), source_revision);
            assert_eq!(
                document.collaboration_state().inbound_deliveries
                    [&CollaborationDeliveryId::try_new("wake-delivery").unwrap()]
                    .state,
                CollaborationDeliveryState::Accepted
            );
        })
        .unwrap();
}

/// A revoked foreign question must not turn the incoming mailbox scan into an error.
/// 已撤权的跨项目问题不得使传入收件箱扫描失败。
#[tokio::test]
async fn incoming_question_scan_skips_revoked_foreign_question() {
    // Keep the waiting session's real open turn and admit an exact foreign source participant.
    // 保留等待会话的真实开放回合，并接纳精确的跨项目来源参与者。
    let dispatch = native_dispatch_fixture(true, WAIT_AGENT_TOOL_ID).await;
    let source_turn = dispatch
        .source()
        .read(|document| document.active_turn().unwrap().turn_id.value())
        .unwrap();
    let target_turn = dispatch
        .target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("revoked-wake-asker-root").unwrap(),
                "Ask the foreign waiter for direction",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value
        .value();
    // The grant belongs to the question source, which is the fixture's foreign target.
    // 授权属于问题来源，即夹具中的跨项目目标会话。
    let granted = crate::project_conversation_services::service_add_collaboration_grant(
        dispatch.invocation.state.clone(),
        dispatch.target.session_id().to_string(),
        dispatch.invocation.source_session_id.to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Ask].into_iter().collect(),
    )
    .await
    .unwrap();
    let question_id = admit_incoming_question(&dispatch, source_turn, target_turn);
    let visible = incoming_question_snapshot(&dispatch.invocation)
        .await
        .unwrap();
    assert_eq!(visible.unwrap()["question_id"], question_id.as_str());
    // Revocation leaves the accepted inbox fact for durable repair but removes model disclosure.
    // 撤权保留已接纳收件事实供可靠修复，同时移除模型可见性。
    crate::project_conversation_services::service_remove_collaboration_grant(
        dispatch.invocation.state.clone(),
        dispatch.target.session_id().to_string(),
        granted["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    assert!(
        incoming_question_snapshot(&dispatch.invocation)
            .await
            .unwrap()
            .is_none()
    );
}
