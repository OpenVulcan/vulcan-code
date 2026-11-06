//! Business-only consumption of durable child results.
//! 仅允许业务请求消费可靠子任务结果。

use super::*;

/// Constructs a real closed child with completed text for negative success-return checks.
/// 构造具有已完成文本的真实闭合子文档，用于成功回流的拒绝检查。
/// `purpose` selects the actual API and `outcome` selects the actual turn closure.
/// `purpose` 选择实际 API，`outcome` 选择实际回合闭合结果。
/// Returns the durable document and source request, propagating fixture domain errors.
/// 返回可靠文档及来源请求，传播构造领域错误。
fn closed_child(
    purpose: ApiPurposeKind,
    outcome: TurnOutcome,
) -> Result<(SessionDocument, SubagentReturnBuildRequest), DomainError> {
    // This child keeps authentic API and turn terminal facts for the selected scenario.
    // 此子文档保留所选场景的真实 API 和回合终态事实。
    let mut child = new_document();
    // The root owns the exact closed business turn.
    // 根输入拥有精确闭合业务回合。
    let turn = child.submit_root_input_fixture("work", vec![], T0)?;
    // The API purpose remains distinguishable in the source proof.
    // API 用途在来源凭据中保持可区分。
    let api = child.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 1)?;
    // Completed text can survive a later stop, so its status alone cannot authorize success.
    // 已完成文本可以在后续停止后保留，因此单凭文本状态不能授权成功。
    let text = child
        .append_assistant_text(
            api,
            "recorded text",
            AssistantSemanticStatus::Completed,
            T0 + 2,
        )?
        .into_value();
    complete_with_usage(&mut child, api, UsageSnapshot::default(), T0 + 3)?;
    child.close_turn(outcome, 4, T0 + 4)?;
    advance_fixture_durable(&mut child)?;
    Ok((
        child,
        SubagentReturnBuildRequest {
            final_turn_id: turn,
            child_task_id: SubagentTaskId::try_new("terminal-proof-child")?,
            source_assistant_text_seq: text,
        },
    ))
}

/// Stopped and abandoned child tasks cannot publish surviving text as successful results.
/// 停止及放弃的子任务不能把留存文本作为成功结果发布。
/// Returns fixture errors; both non-success outcomes must be explicitly rejected.
/// 返回构造错误；两种非成功结果必须被明确拒绝。
#[test]
fn subagent_return_rejects_non_success_business_outcomes() -> Result<(), DomainError> {
    for outcome in [TurnOutcome::Stopped, TurnOutcome::Abandoned] {
        // Both fixtures contain a completed API and text before their non-success closure.
        // 两种场景在非成功闭合前均包含已完成 API 和文本。
        let (child, request) = closed_child(ApiPurposeKind::MainGeneration, outcome)?;
        assert!(matches!(
            child.build_subagent_return_input(request),
            Err(DomainError::InvalidStatistics {
                field: "child_final_outcome"
            })
        ));
    }
    Ok(())
}

/// Auxiliary APIs and boundaries cannot substitute for a final business response.
/// 辅助 API 与边界不能替代最终业务回复。
/// Returns fixture errors; success requires both a normal request and business closure.
/// 返回构造错误；成功同时要求正常请求和业务闭合。
#[test]
fn subagent_return_rejects_auxiliary_final_proofs() -> Result<(), DomainError> {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // An auxiliary response stays auxiliary even when its enclosing turn is completed, so the
        // turn has no business API to return.
        // 即使外层回合已完成，辅助回复仍然属于辅助工作，因此该回合没有可回流的业务 API。
        let (child, request) = closed_child(purpose, TurnOutcome::Completed)?;
        assert!(matches!(
            child.build_subagent_return_input(request),
            Err(DomainError::InvalidStatistics {
                field: "child_final_api_stats"
            })
        ));
    }
    // An auxiliary boundary is independently rejected even with a normal completed API.
    // 即使 API 正常完成，辅助边界也被独立拒绝。
    let (mut child, request) =
        closed_child(ApiPurposeKind::MainGeneration, TurnOutcome::Completed)?;
    for event in child.events.iter_mut() {
        if let EventBody::TurnBoundary(boundary) = &mut event.event {
            boundary.purpose = crate::v1::event::TurnBoundaryPurpose::QuestionExecution;
        }
    }
    assert!(matches!(
        child.build_subagent_return_input(request),
        Err(DomainError::InvalidStatistics {
            field: "child_final_business_boundary"
        })
    ));
    Ok(())
}

/// Auxiliary APIs cannot consume a child result, including after durable document reconstruction.
/// 辅助 API 不能消费子结果，包括可靠文档重建后的情况。
/// Returns a domain error on fixture failure; all lifecycle and receipt checks are assertions.
/// 构造失败时返回领域错误；生命周期及回执检查均使用断言。
#[test]
fn subagent_return_consumption_excludes_compaction_and_question_apis() -> Result<(), DomainError> {
    // Build the returned body from a completed canonical child rather than inventing its proof.
    // 从已完成的规范子文档构造返回正文，不虚构凭据。
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    // The input is immutable and may be delivered independently into each parent fixture.
    // 输入不可变，可独立投递到各父文档测试场景。
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("result-consumption-child")?,
        source_assistant_text_seq,
    })?;
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // Each scenario has a distinct canonical parent document and open business turn.
        // 每个场景拥有独立规范父文档和开放业务回合。
        let mut parent = new_document();
        parent.submit_root_input_fixture("delegate", Vec::new(), T0)?;
        // Keep the original receipt to prove auxiliary work cannot select or replace a result.
        // 保留原回执，证明辅助工作不能消费或替换结果。
        let receipt = parent
            .append_subagent_return(input.clone(), T0 + 1)?
            .into_value();
        parent.start_api(purpose, ApiRequestSnapshot::new("openai", "gpt-5"), T0 + 2)?;
        assert_eq!(
            parent.pending_subagent_return_dispatch()?,
            Some(receipt.clone())
        );
        parent.interrupt_active_api(T0 + 3)?;
        advance_fixture_durable(&mut parent)?;
        // Restore actual serialized authoritative state, with the closed auxiliary API retained.
        // 恢复实际序列化权威状态，保留已闭合辅助 API。
        let mut restored = SessionDocument::from_parts(
            serde_json::from_str(&serde_json::to_string(&parent.metadata).unwrap()).unwrap(),
            parent.runtime_lock.clone(),
            parent.compactions.clone(),
            parent.segment_catalog.clone(),
            serde_json::from_str(&serde_json::to_string(parent.events()).unwrap()).unwrap(),
        )?;
        assert_eq!(restored.pending_subagent_return_dispatch()?, Some(receipt));
        restored.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 4,
        )?;
        assert_eq!(restored.pending_subagent_return_dispatch()?, None);
    }
    Ok(())
}
