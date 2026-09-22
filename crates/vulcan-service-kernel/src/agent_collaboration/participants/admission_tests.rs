//! Atomic target admission across MainOnly and full-stop transitions.
//! MainOnly 与完整停止转换中的原子目标准入。

use super::*;
use vulcan_session_core::v1::collaboration::SetRunControlInput;
use vulcan_session_core::v1::document::SessionDocument;
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission};

/// Creates a parent with a durable direct-child reservation and current participant identities.
/// 创建具有可靠直接子任务预留及当前参与者身份的父文档。
/// Returns the parent, its child source, and the retained parent target.
/// 返回父文档、子来源以及保留的父目标。
fn parent_fixture() -> (
    SessionDocument,
    CollaborationParticipant,
    CollaborationParticipant,
) {
    // The parent turn is allocated by the real root-input reducer.
    // 父回合由真实根输入归约器分配。
    let mut parent = SessionDocument::new("parent", "project", "Parent", 1);
    let turn = parent
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("parent-root").unwrap(),
                "Parent work",
                vec![],
            ),
            2,
        )
        .unwrap()
        .into_value();
    // Capture the child without a turn, exactly as reservation precedes child activation.
    // 捕获尚无回合的子身份，与预留先于子激活的顺序一致。
    let child = CollaborationParticipant::new(
        "child",
        "parent",
        "project",
        1,
        None,
        Some(SubagentTaskId::try_new("child").unwrap()),
    )
    .unwrap();
    parent
        .reserve_child_task(ReserveChildTaskInput {
            source_tool_name: String::from("spawn_agent"),
            execution_id:
                vulcan_session_core::v1::collaboration::CollaborationExecutionId::try_new(
                    "child-execution",
                )
                .unwrap(),
            operation_id: CollaborationOperationId::try_new("reserve-child").unwrap(),
            task_id: child.task_id.clone().unwrap(),
            parent_turn_id: turn,
            child: child.clone(),
            reserved_at_ms: 3,
        })
        .unwrap();
    let target = CollaborationParticipant::new(
        "parent",
        "parent",
        "project",
        parent.segment_generation(),
        Some(turn),
        None,
    )
    .unwrap();
    (parent, child, target)
}

/// Writes a real control transition used to race a previously accepted admission snapshot.
/// 写入真实控制转换，用于模拟此前准入快照之后的竞争。
/// Mutates `document` to `control`; each test uses a unique operation label.
/// 将 `document` 改为 `control`；每项测试使用唯一操作标签。
fn set_control(document: &mut SessionDocument, control: CollaborationRunControl, label: &str) {
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(label).unwrap(),
            state: control,
            changed_at_ms: 4,
        })
        .unwrap();
}

/// A Running preflight cannot authorize an unrelated sender after the target becomes MainOnly.
/// Running 预检查不能授权无关发送者在目标变为 MainOnly 后继续写入。
#[test]
fn current_main_only_requires_exact_direct_child() {
    // A separate root is initially allowed while the parent is running.
    // 父会话运行时，独立根会话最初可以发送。
    let (mut parent, child, target) = parent_fixture();
    let stranger =
        CollaborationParticipant::new("stranger", "stranger", "project", 1, None, None).unwrap();
    set_control(&mut parent, CollaborationRunControl::Running, "running");
    require_document_message_admission(&parent, &stranger, &target).unwrap();
    set_control(&mut parent, CollaborationRunControl::MainOnly, "main-only");
    let before = (
        parent.metadata.clone(),
        parent.events().to_vec(),
        parent.memory_revision(),
    );
    assert!(require_document_message_admission(&parent, &stranger, &target).is_err());
    require_document_message_admission(&parent, &child, &target).unwrap();
    // The activated child's turn may differ from the reservation's absent turn.
    // 已激活子代理的回合可以不同于预留时的空回合。
    let mut active_child = child.clone();
    active_child.turn_id = Some(TurnId::try_new(100).unwrap());
    require_document_message_admission(&parent, &active_child, &target).unwrap();
    let mut stale_child = child.clone();
    stale_child.segment_generation += 1;
    assert!(require_document_message_admission(&parent, &stale_child, &target).is_err());
    let mut foreign_root = child.clone();
    foreign_root.root_session_id = String::from("another-root");
    assert!(require_document_message_admission(&parent, &foreign_root, &target).is_err());
    let mut stale_target = target.clone();
    stale_target.turn_id = Some(TurnId::try_new(101).unwrap());
    assert!(require_document_message_admission(&parent, &child, &stale_target).is_err());
    stale_target = target.clone();
    stale_target.segment_generation += 1;
    assert!(require_document_message_admission(&parent, &child, &stale_target).is_err());
    assert_eq!(
        (
            parent.metadata.clone(),
            parent.events().to_vec(),
            parent.memory_revision()
        ),
        before
    );
}

/// Full-stop states reject even a verified child and never mutate the retained parent.
/// 完整停止状态拒绝已验证子代理，且不修改保留的父文档。
#[test]
fn full_stop_rejects_direct_child() {
    for control in [
        CollaborationRunControl::Stopped,
        CollaborationRunControl::MainAndSubagents,
    ] {
        // Each stop starts from an independent parent with a genuine reservation.
        // 每种停止都从具有真实预留的独立父文档开始。
        let (mut parent, child, target) = parent_fixture();
        set_control(&mut parent, control, "stop");
        let before = (
            parent.metadata.clone(),
            parent.events().to_vec(),
            parent.memory_revision(),
        );
        assert!(require_document_message_admission(&parent, &child, &target).is_err());
        assert_eq!(
            (
                parent.metadata.clone(),
                parent.events().to_vec(),
                parent.memory_revision()
            ),
            before
        );
    }
}
