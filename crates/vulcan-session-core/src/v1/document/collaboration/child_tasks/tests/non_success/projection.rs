//! Complete non-success result projection and business-only consumption regressions.
//! 完整非成功结果投影与仅业务消费回归。

use super::*;
use crate::v1::event::{ApiPurposeKind, ApiRequestSnapshot};

/// Publishes the current complete fixture revision through the actual Core flush protocol.
/// 通过实际 Core 刷写协议发布当前完整构造修订。
/// `document` is mutated only by normal durability advancement; returns no value.
/// `document` 仅通过正常持久化推进修改；无返回值。
fn publish(document: &mut SessionDocument) {
    // The flush candidate owns the revision; tests never guess or assign its number.
    // 刷写候选拥有修订；测试绝不猜测或赋值修订号。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
}

/// Builds a durable parent report containing long Unicode and escaped diagnostic text.
/// 构造包含长 Unicode 及转义诊断文本的可靠父报告。
/// Returns the parent, child execution and immutable report for projection tests.
/// 返回父文档、子执行及不可变报告供投影测试使用。
fn parent_report() -> (
    SessionDocument,
    CollaborationExecutionId,
    CollaborationChildTaskNonSuccessResult,
) {
    // The open business turn and original reservation define the result's exact recipient.
    // 开放业务回合与原预留定义结果的精确接收者。
    let (mut parent, turn) = document_with_open_turn();
    parent
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: operation("projection-running"),
            state: CollaborationRunControl::Running,
            changed_at_ms: T0,
        })
        .unwrap();
    // Keep a stable execution across the real child root and parent reservation.
    // 在真实子根输入和父预留之间保持稳定执行。
    let reservation = reserve_input(
        operation("project-result-reserve"),
        task("project-child"),
        turn,
    );
    parent.reserve_child_task(reservation.clone()).unwrap();
    // Real source storage is represented by a canonical child whose failure is durably advanced.
    // 真实来源存储由失败已推进持久化的规范子文档表示。
    let mut child = SessionDocument::new("child-session", "project-1", "child", T0);
    // Exact root identity comes from the original execution, never a later selected task.
    // 精确根身份来自原执行，绝非后来选中的任务。
    let child_turn = child
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new(reservation.execution_id.as_str()).unwrap(),
                "work",
                vec![],
            ),
            T0,
        )
        .unwrap()
        .into_value();
    // This exceeds one envelope and exercises multibyte boundaries plus shared XML escaping.
    // 此文本超过一个信封，并验证多字节边界及共享 XML 转义。
    let diagnostic = "失败🧪 <tag> & \"quoted\"\n".repeat(6000);
    child
        .fail_execution_boundary_and_close_turn("long_failure", Some(&diagnostic), T0 + 3)
        .unwrap();
    publish(&mut child);
    // The opaque input guarantees authentic durable source coordinates.
    // 不透明输入保证真实可靠来源坐标。
    let input = child
        .build_child_task_non_success_input(
            reservation.execution_id.clone(),
            child_turn,
            CollaborationChildTaskTerminalOutcome::Failed,
        )
        .unwrap();
    // The original report is published before any model disclosure is authorized.
    // 在授权任何模型披露前发布原报告。
    let result = parent
        .record_child_task_non_success(input, T0 + 4)
        .unwrap()
        .into_value();
    publish(&mut parent);
    (parent, reservation.execution_id, result)
}

/// Complete input batches preserve bytes, prefix history, replay receipts and business-only consumption.
/// 完整输入批次保留字节、历史前缀、重放回执及仅业务消费语义。
/// Returns no value; actual canonical APIs and serialized recovery prove each boundary.
/// 不返回值；实际规范 API 与序列化恢复证明各个边界。
#[test]
fn child_non_success_projection_preserves_all_chunks_and_consumption() {
    // Start from an actually published report containing multiple envelope bodies.
    // 从实际已发布且包含多个信封正文的报告开始。
    let (mut parent, execution, result) = parent_report();
    // The entire prefix must survive model projection unchanged.
    // 模型投影后整个前缀必须保持不变。
    let prefix = parent.events().to_vec();
    // One revision owns the complete input batch and its receipt.
    // 一份修订拥有完整输入批次及其回执。
    let revision = parent.memory_revision();
    // Freeze the complete projection receipt before testing replay and recovery.
    // 在验证重放与恢复前冻结完整投影回执。
    let projected = parent
        .project_child_task_non_success_before_api(execution.clone(), T0 + 5)
        .unwrap()
        .into_value();
    assert_eq!(parent.memory_revision(), revision + 1);
    assert_eq!(&parent.events()[..prefix.len()], prefix.as_slice());
    assert!(projected.inputs.len() > 1);
    // Decode every original model body to prove byte-complete reconstruction.
    // 解码每份原模型正文，证明字节完整重建。
    let task = &parent.collaboration_state().child_tasks[&execution];
    // Pure envelope generation uses the same immutable authority as first insertion.
    // 纯信封生成使用与首次插入相同的不可变权威。
    let envelopes = result.model_envelopes(task, "parent-session").unwrap();
    // Accumulate exact original JSON fragments without newline or encoding normalization.
    // 累积精确原 JSON 片段，不归一化换行或编码。
    let mut recovered_body = String::new();
    for (index, envelope) in envelopes.iter().enumerate() {
        assert!(
            envelope.body.len() <= vulcan_ai_contract::host_message::HOST_MESSAGE_MAX_BODY_BYTES
        );
        // Header and fragment are separated only at the first newline.
        // 仅在第一个换行处拆分头部与片段。
        let (header, fragment) = envelope.body.split_once('\n').unwrap();
        // Structured byte coordinates are independent of the escaped display representation.
        // 结构化字节坐标独立于转义后的显示表示。
        let header: serde_json::Value = serde_json::from_str(header).unwrap();
        assert_eq!(header["chunk_index"], index);
        assert_eq!(header["chunk_count"], envelopes.len());
        assert_eq!(header["byte_start"], recovered_body.len());
        recovered_body.push_str(fragment);
        assert_eq!(header["byte_end"], recovered_body.len());
        assert_eq!(envelope.source, "child-session");
        assert_eq!(envelope.target, "parent-session");
    }
    assert_eq!(
        serde_json::from_str::<CollaborationChildTaskNonSuccessProof>(&recovered_body).unwrap(),
        result.proof
    );
    // Replaying projection returns its original receipt and does not append another User input.
    // 投影重放返回原回执，不追加另一份 User 输入。
    let before = parent.clone();
    assert_eq!(
        parent
            .project_child_task_non_success_before_api(execution.clone(), T0 + 6)
            .unwrap()
            .into_value(),
        projected
    );
    assert_eq!(parent, before);
    publish(&mut parent);
    parent = reload(&parent).unwrap();
    assert!(parent.pending_collaboration_continuation().is_some());
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        parent
            .start_api(
                purpose,
                ApiRequestSnapshot::new("provider", "model"),
                T0 + 7,
            )
            .unwrap();
        assert!(parent.pending_collaboration_continuation().is_some());
        parent.interrupt_active_api(T0 + 8).unwrap();
    }
    parent
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("provider", "model"),
            T0 + 9,
        )
        .unwrap();
    assert!(parent.pending_collaboration_continuation().is_none());
    assert!(
        reload(&parent)
            .unwrap()
            .pending_collaboration_continuation()
            .is_none()
    );
}

/// A conflict in a later input chunk leaves no earlier chunk or projection receipt behind.
/// 后续输入分块冲突不会留下较早分块或投影回执。
/// Returns no value; the complete parent document is compared after rejection.
/// 不返回值；拒绝后比较完整父文档。
#[test]
fn child_non_success_projection_rolls_back_partial_batch() {
    // At least two chunks are required to expose a failure after the first staged insertion.
    // 至少需要两块，才能暴露首块暂存插入后的失败。
    let (mut parent, execution, result) = parent_report();
    parent
        .append_in_turn_guidance_before_api(
            ClientActionId::try_new(format!(
                "agent-message:child-result:{}:1",
                result.event_seq.value()
            ))
            .unwrap(),
            String::from("conflicting input"),
            T0 + 5,
        )
        .unwrap();
    // Preserve all canonical state, including the pre-existing conflicting input.
    // 保留全部规范状态，包括已存在的冲突输入。
    let before = parent.clone();
    assert!(
        parent
            .project_child_task_non_success_before_api(execution, T0 + 6)
            .is_err()
    );
    assert_eq!(parent, before);
}

/// Active API insertion and tampered resident input are rejected by their authoritative boundaries.
/// 活动 API 插入和被篡改驻留输入均由权威边界拒绝。
/// Returns no value; malformed input cannot be accepted as consumed after cold reconstruction.
/// 不返回值；畸形输入不能在冷重建后被接纳为已消费。
#[test]
fn child_non_success_projection_checks_api_and_recovery_proofs() {
    // The original report has been published, but no model input exists yet.
    // 原报告已发布，但尚无模型输入。
    let (mut parent, execution, _) = parent_report();
    parent
        .start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("provider", "model"),
            T0 + 5,
        )
        .unwrap();
    // A business request before projection cannot consume data absent from its context.
    // 投影前的业务请求不能消费其上下文中不存在的数据。
    let before = parent.clone();
    assert!(parent.pending_collaboration_continuation().is_some());
    assert!(
        parent
            .project_child_task_non_success_before_api(execution.clone(), T0 + 6)
            .is_err()
    );
    assert_eq!(parent, before);
    parent.interrupt_active_api(T0 + 7).unwrap();
    // Project through the same normal API boundary used by production.
    // 通过生产所用同一正常 API 边界投影。
    let projection = parent
        .project_child_task_non_success_before_api(execution, T0 + 8)
        .unwrap()
        .into_value();
    for event in parent.events.iter_mut() {
        if event.seq == projection.inputs[0].input_seq
            && let EventBody::UserInput(input) = &mut event.event
        {
            input.text.push_str("tampered");
        }
    }
    assert!(reload(&parent).is_err());
}
