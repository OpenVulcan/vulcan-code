//! Owned predecessor recovery preserves distinct results and immutable successor coverage.
//! 所属前驱恢复保留不同结果与不可变后继覆盖范围。

use super::*;

/// Completes a later child turn; returns its real source document and exact return request.
/// 完成一个后续子回合；返回真实来源文档及精确回流请求。
fn later_child_result() -> Result<(SessionDocument, SubagentReturnBuildRequest), DomainError> {
    // Reuse the same original execution as the owned-return fixture before starting its successor.
    // 启动后继之前，复用所属回流夹具中的同一原始执行。
    let (mut child, _, _) = durable_child_return_fixture()?;
    // A new root action distinguishes the successor from the original reservation.
    // 新的根动作区分后继与原预留。
    let turn = child.submit_root_input_fixture("queued follow-up", vec![], T0 + 10)?;
    // Produce an actual successful business API rather than manufacturing return evidence.
    // 产生实际成功业务 API，而非制造回流凭据。
    let api = child.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 11,
    )?;
    // This marker proves recovery does not substitute the successor's content for the original answer.
    // 此标记证明恢复不会用后继正文替换原始回答。
    let text = child
        .append_assistant_text(
            api,
            "queued-result-marker",
            AssistantSemanticStatus::Completed,
            T0 + 12,
        )?
        .into_value();
    complete_with_usage(&mut child, api, usage(2, 3), T0 + 13)?;
    child.close_turn(TurnOutcome::Completed, 4, T0 + 14)?;
    advance_fixture_durable(&mut child)?;
    Ok((
        child,
        SubagentReturnBuildRequest {
            final_turn_id: turn,
            child_task_id: SubagentTaskId::try_new("owned-return-task")?,
            source_assistant_text_seq: text,
        },
    ))
}

/// An original owned result can recover after a later result claimed its prefix, including cold replay.
/// 后续结果占用前缀后，原始所属结果仍可恢复，包括冷重放。
/// Returns domain errors; verifies exact content, atomic completion and both results' idempotence.
/// 返回领域错误；验证精确正文、原子完成及两个结果的幂等性。
#[test]
fn owned_return_recovers_overlapping_successor_prefix_without_rewriting_history()
-> Result<(), DomainError> {
    // The original reservation remains active while the queued result is published first.
    // 排队结果先发布时，原预留保持活动状态。
    let (mut parent, turn, execution, original) = owned_return_fixture()?;
    // Build the historical broad range through the same production builder used by live delivery.
    // 通过实时投递所用的同一生产构造器建立历史宽范围。
    let (child, request) = later_child_result()?;
    // No previous parent return exists, so the successor claims the entire unreturned prefix.
    // 父侧尚无回流，因此后继占用整个未返回前缀。
    let successor = child.build_subagent_return_input_since(request.clone(), None)?;
    parent.append_subagent_return(successor.clone(), T0 + 20)?;
    // Existing events must remain byte-for-byte equivalent after recovery.
    // 恢复之后，既有事件必须保持逐字节等价。
    let history = parent.events.to_vec();
    assert!(matches!(
        parent.append_subagent_return(original.clone(), T0 + 21),
        Err(DomainError::InvalidStatistics {
            field: "subagent_return.overlapping_source_range"
        })
    ));
    assert_eq!(parent.events.to_vec(), history);
    parent.complete_child_task_with_return(execution.clone(), original.clone(), T0 + 22)?;
    assert_eq!(&parent.events[..history.len()], history.as_slice());
    assert!(
        parent
            .completion_obligations(turn)
            .open_child_task_ids
            .is_empty()
    );
    // Original content and execution identity must both survive, independent of successor coverage.
    // 原始正文及执行身份必须同时保留，不受后继覆盖影响。
    let returned = parent
        .events
        .iter()
        .find_map(|event| match &event.event {
            EventBody::SubagentReturn(returned)
                if returned.child_root_action_id.as_str() == execution.as_str() =>
            {
                Some(returned)
            }
            _ => None,
        })
        .expect("the recovered original result must exist");
    assert_eq!(returned.content, original.content);
    assert_ne!(returned.content, successor.content);
    advance_fixture_durable(&mut parent)?;
    // Reconstruct through the normal strict loader before replaying both deliveries.
    // 在重放两次投递之前，经普通严格加载器重建。
    let mut restored = SessionDocument::from_parts(
        parent.metadata,
        parent.runtime_lock,
        parent.compactions,
        parent.segment_catalog,
        parent.events.into_vec(),
    )?;
    // The later predecessor must not shift the successor's original immutable frontier.
    // 后补前驱不能移动后继原始不可变前沿。
    let frontier = restored.subagent_return_frontier_before(
        "child-session",
        child.segment_generation(),
        request.source_assistant_text_seq,
    );
    assert_eq!(
        child.build_subagent_return_input_since(request, frontier)?,
        successor
    );
    assert!(
        !restored
            .append_subagent_return(successor, T0 + 23)?
            .changed()
    );
    assert!(
        !restored
            .complete_child_task_with_return(execution.clone(), original.clone(), T0 + 24)?
            .changed()
    );
    // A recovered identity remains immutable even when replay is permitted through historical overlap.
    // 即使允许经过历史重叠重放，恢复后的身份仍然不可变。
    let mut drifted = original;
    drifted.content.push_str(" drift");
    // Preserve the complete state while rejecting a changed source body.
    // 拒绝变更来源正文时保留完整状态。
    let before = restored.clone();
    assert!(matches!(
        restored.complete_child_task_with_return(execution, drifted, T0 + 25),
        Err(DomainError::ImmutableFieldMutation {
            entity: "subagent_return",
            field: "durable_facts"
        })
    ));
    assert_eq!(restored, before);
    Ok(())
}

/// Recovery never accepts a different task, reused root action, or crossing source.
/// 恢复绝不接受不同任务、复用根动作或交叉来源。
/// Returns domain errors; every rejection must leave the reservation and complete document unchanged.
/// 返回领域错误；每次拒绝都必须保持预留及完整文档不变。
#[test]
fn owned_return_overlap_recovery_keeps_identity_and_source_guards() -> Result<(), DomainError> {
    // Each mutation below represents corrupted historical successor evidence.
    // 下方每次修改均表示损坏的历史后继凭据。
    for fault in ["task", "root", "crossing"] {
        // Each case begins with a real owned execution and a distinct completed successor.
        // 每种情况从真实所属执行及不同已完成后继开始。
        let (mut parent, _, execution, original) = owned_return_fixture()?;
        // Obtain the historical broad prefix with no earlier delivery.
        // 在没有更早投递时取得历史宽前缀。
        let (child, request) = later_child_result()?;
        parent.append_subagent_return(
            child.build_subagent_return_input_since(request, None)?,
            T0 + 20,
        )?;
        // Alter only the selected evidence field; the original opaque input remains verified.
        // 仅修改选定凭据字段；原不透明输入仍经过验证。
        let returned = parent
            .events
            .iter_mut()
            .find_map(|event| match &mut event.event {
                EventBody::SubagentReturn(returned) => Some(returned),
                _ => None,
            })
            .unwrap();
        match fault {
            "task" => returned.child_task_id = SubagentTaskId::try_new("foreign-task")?,
            "root" => returned.child_root_action_id = original.child_root_action_id.clone(),
            "crossing" => returned.source_assistant_text_seq = original.child_turn_stats_seq,
            _ => unreachable!(),
        }
        // Failed recovery must not append content or accidentally release the original obligation.
        // 恢复失败不得追加正文，也不得意外解除原义务。
        let before = parent.clone();
        assert!(matches!(
            parent.complete_child_task_with_return(execution, original, T0 + 21),
            Err(DomainError::InvalidStatistics {
                field: "subagent_return.overlapping_source_range"
            })
        ));
        assert_eq!(parent, before);
    }
    Ok(())
}
