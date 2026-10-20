//! Delivery decisions for child results whose parent can no longer receive them.
//! 父会话已无法接收的子结果的回流判定。

use super::*;
use vulcan_session_core::v1::collaboration::CollaborationParticipant;
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission};

/// A result stays deliverable while its parent turn is open and is discarded once the parent
/// settled the execution without success.
/// 父回合开放时结果仍可回流；父侧以非成功结算该执行后结果被丢弃。
/// Parameters: none. Returns: none; verifies the decision and that discarding writes nothing.
/// 参数：无。返回值：无；验证判定结果及丢弃时不写入任何内容。
#[tokio::test]
async fn child_return_is_discarded_after_parent_settles_without_success() {
    let (_root, invocation, parent) = test_support::fixture(false).await;
    parent
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("orphan-parent").unwrap(),
                "root",
                vec![],
            ),
            2,
        )
        .unwrap();
    let parent_turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    let project_id = invocation.source_locator.project_id().to_owned();
    let child_id = Uuid::new_v4().to_string();
    let task_id = SubagentTaskId::try_new(child_id.clone()).unwrap();
    let execution = CollaborationExecutionId::try_new("orphan-execution").unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    operation_id: CollaborationOperationId::try_new("orphan-reserve").unwrap(),
                    source_tool_name: String::from("spawn_agent"),
                    execution_id: execution.clone(),
                    task_id: task_id.clone(),
                    parent_turn_id: parent_turn,
                    child: CollaborationParticipant::new(
                        child_id.clone(),
                        parent.session_id().to_string(),
                        project_id.as_str(),
                        1,
                        None,
                        Some(task_id.clone()),
                    )?,
                    reserved_at_ms: 3,
                })
            },
        )
        .unwrap();

    assert!(!settle_child_return_without_open_parent_turn(&parent, &execution, 4).unwrap());

    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.terminal_child_task(TerminalChildTaskInput {
                    operation_id: CollaborationOperationId::try_new("orphan-failed").unwrap(),
                    execution_id: execution.clone(),
                    task_id: task_id.clone(),
                    outcome: CollaborationChildTaskTerminalOutcome::Failed,
                    terminal_at_ms: 5,
                })
            },
        )
        .unwrap();
    let revision = parent.read(|document| document.durable_revision()).unwrap();

    assert!(settle_child_return_without_open_parent_turn(&parent, &execution, 6).unwrap());
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision
    );
}
