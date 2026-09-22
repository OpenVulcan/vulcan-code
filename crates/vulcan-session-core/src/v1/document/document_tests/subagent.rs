//! Durable child-return identity, accounting, and delivery regression tests.
//! 可靠子任务返回身份、记账及投递回归测试。

use super::*;

mod owned_returns;
mod result_consumption;

/// A final return includes stopped predecessor usage and additive timing exactly once.
/// 最终回流恰好一次包含此前停止回合的用量与可累加计时。
#[test]
fn subagent_return_includes_unreturned_stopped_turn_and_timing() -> Result<(), DomainError> {
    let mut child = new_document();
    let mut final_turn = None;
    let mut final_text = None;
    for index in 0..2 {
        let now = T0 + index * 100;
        let turn = child.submit_root_input_fixture("work", vec![], now)?;
        let api = child.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            now + 1,
        )?;
        final_text = Some(
            child
                .append_assistant_text(api, "result", AssistantSemanticStatus::Completed, now + 4)?
                .into_value(),
        );
        child.complete_api_group(
            api,
            ApiCompletion {
                request: ApiRequestSnapshot::new("openai", "gpt-5"),
                usage: usage(10, 6),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings {
                    dispatched_at_ms: Some(now + 1),
                    first_token_at_ms: Some(now + 3),
                    completed_at_ms: Some(now + 9),
                    ..Default::default()
                },
                provider_source: None,
                completed_at_ms: now + 9,
            },
        )?;
        child.close_turn(
            if index == 0 {
                TurnOutcome::Stopped
            } else {
                TurnOutcome::Completed
            },
            10,
            now + 10,
        )?;
        final_turn = Some(turn);
    }
    advance_fixture_durable(&mut child)?;
    let input = child.build_subagent_return_input_since(
        SubagentReturnBuildRequest {
            final_turn_id: final_turn.unwrap(),
            child_task_id: SubagentTaskId::try_new("worker")?,
            source_assistant_text_seq: final_text.unwrap(),
        },
        None,
    )?;
    let mut parent = SessionDocument::new("parent", "project", "parent", T0);
    parent.submit_root_input_fixture("delegate", vec![], T0)?;
    parent.append_subagent_return(input.clone(), T0 + 200)?;
    assert!(!parent.append_subagent_return(input, T0 + 201)?.changed());
    let buckets = parent.aggregate_turn_buckets(
        parent.active_turn().unwrap().turn_id,
        parent.active_turn().unwrap().start_seq,
    )?;
    assert_eq!(buckets.total.api_call_count, 0);
    assert!(buckets.total.output_timing.is_none());
    let projected = crate::v1::projection::project_provider_items(parent.events(), None, &[])?;
    assert!(projected.iter().any(|item| matches!(item,
        crate::v1::projection::ProjectedItem::SubagentReturnText { text }
            if text.contains("<vulcan_agent_message")
                && text.contains("kind=\"task_result\"")
                && text.contains("source=\"sess-1\"")
                && text.contains("target=\"parent\"")
                && text.contains("task_id=\"worker\"")
                && text.contains(">result</vulcan_agent_message>")
    )));
    Ok(())
}

/// A durable child return round-trips and exact delivery retry is revision-idempotent.
/// 可靠子代理回流可以严格往返，且精确重复投递不会制造新修订。
#[test]
fn subagent_return_round_trips_and_exact_retry_is_idempotent() -> Result<(), DomainError> {
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-final")?,
        source_assistant_text_seq,
    })?;
    assert_eq!(input.child_session_id, "child-session");
    assert_eq!(input.content, "final child result");

    let mut parent = new_document();
    let parent_turn = parent.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    let committed = parent.append_subagent_return(input.clone(), T0 + 1)?;
    assert!(committed.changed());
    let receipt = committed.into_value();
    assert_eq!(receipt.parent_turn_id, parent_turn);
    assert_eq!(receipt.source_assistant_text_seq, source_assistant_text_seq);
    assert_eq!(
        parent.pending_subagent_return_dispatch()?,
        Some(receipt.clone())
    );
    let mut covered = parent.clone();
    covered.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    assert_eq!(covered.pending_subagent_return_dispatch()?, None);
    let revision_after_commit = parent.memory_revision();
    let repeated = parent.append_subagent_return(input.clone(), T0 + 2)?;
    assert!(!repeated.changed());
    assert_eq!(repeated.into_value(), receipt);
    assert_eq!(parent.memory_revision(), revision_after_commit);

    advance_fixture_durable(&mut parent)?;
    let mut restored = SessionDocument::from_parts(
        parent.metadata.clone(),
        parent.runtime_lock.clone(),
        parent.compactions.clone(),
        parent.segment_catalog.clone(),
        parent.events.clone(),
    )?;
    let restored_revision = restored.memory_revision();
    let replayed = restored.append_subagent_return(input, T0 + 3)?;
    assert!(!replayed.changed());
    assert_eq!(replayed.into_value(), receipt);
    assert_eq!(restored.memory_revision(), restored_revision);

    restored.close_turn(TurnOutcome::Stopped, 4, T0 + 4)?;
    let turn_stats = restored
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(value) => Some(value),
            _ => None,
        })
        .expect("turn stats must exist");
    assert_eq!(turn_stats.buckets.subagent.api_call_count, 0);
    assert_eq!(turn_stats.buckets.subagent.unknown_usage_api_call_count, 0);
    assert_eq!(turn_stats.buckets.subagent.tool_call_count, 0);
    assert_eq!(turn_stats.buckets.subagent.subagent_return_count, 0);
    assert_eq!(turn_stats.buckets.total.usage.input_tokens, None);
    assert_eq!(turn_stats.buckets.total.usage.output_tokens, None);
    Ok(())
}

/// A late return opens a non-user successor without altering the closed parent history.
/// 迟到回流开启非用户后继，不改变已关闭的父回合历史。
/// Parameters: none. Returns: domain errors, with persistence and idempotency assertions.
/// 参数：无。返回值：领域错误，并断言持久化与幂等性。
#[test]
fn subagent_return_after_parent_completion_opens_durable_successor() -> Result<(), DomainError> {
    // input is built from the exact durable child final answer, not a synthetic root prompt.
    // input 来源于精确可靠的子任务最终回答，而不是合成根提示词。
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("late-child")?,
        source_assistant_text_seq,
    })?;
    // parent has already finished the user turn before the child's final result arrives.
    // parent 在子任务最终结果到达前已经结束用户回合。
    let mut parent = new_document();
    let original_turn = parent.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    parent.close_turn(TurnOutcome::Completed, 1, T0 + 1)?;
    let original_events = parent.events.clone();
    let receipt = parent
        .append_subagent_return(input.clone(), T0 + 2)?
        .into_value();
    assert_ne!(receipt.parent_turn_id, original_turn);
    assert_eq!(
        &parent.events[..original_events.len()],
        original_events.as_slice()
    );
    assert_eq!(
        parent.metadata.open_turn_start_seq,
        Some(receipt.return_seq)
    );
    assert_eq!(
        parent.pending_subagent_return_dispatch()?,
        Some(receipt.clone())
    );
    advance_fixture_durable(&mut parent)?;
    // restored must infer the successor from the return itself and dispatch without a new user input.
    // restored 必须从回流自身重建后继，并在没有新用户输入时分发。
    let mut restored = SessionDocument::from_parts(
        parent.metadata.clone(),
        parent.runtime_lock.clone(),
        parent.compactions.clone(),
        parent.segment_catalog.clone(),
        parent.events.clone(),
    )?;
    assert_eq!(restored.active_turn(), parent.active_turn());
    // A real next prompt supersedes an undispatched return instead of failing with an open-turn error.
    // 真实下一条提示词取代未分发回流，而不是因开放回合报错。
    let mut user_resume = restored.clone();
    let next_user_turn = user_resume.submit_root_input_fixture(
        "continue with my instruction",
        Vec::new(),
        T0 + 3,
    )?;
    assert_ne!(next_user_turn, receipt.parent_turn_id);
    assert_eq!(
        user_resume.active_turn().unwrap().origin,
        TurnOrigin::UserRoot
    );
    assert_eq!(
        user_resume
            .events
            .iter()
            .filter(|event| matches!(&event.event, EventBody::SubagentReturn(_)))
            .count(),
        1
    );
    advance_fixture_durable(&mut user_resume)?;
    SessionDocument::from_parts(
        user_resume.metadata,
        user_resume.runtime_lock,
        user_resume.compactions,
        user_resume.segment_catalog,
        user_resume.events,
    )?;
    restored.close_turn(TurnOutcome::Completed, 1, T0 + 3)?;
    let after_completion = restored.clone();
    assert!(!restored.append_subagent_return(input, T0 + 4)?.changed());
    assert_eq!(restored, after_completion);
    assert_eq!(
        restored
            .events
            .iter()
            .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
            .count(),
        1
    );
    Ok(())
}

/// Distinct completions from a retained child remain deliverable in the same parent turn.
/// 保持开放的子任务在同一父回合中的不同完成结果均可投递。
/// Parameters: none. Returns: domain errors; asserts exact retry identity and reload validation.
/// 参数：无。返回值：领域错误；断言精确重试身份与重载校验。
#[test]
fn subagent_return_distinguishes_multiple_child_completions() -> Result<(), DomainError> {
    // The first delivery uses the child's initial durable completion.
    // 第一次投递使用子任务最初的可靠完成结果。
    let (mut child, first_turn, first_text) = durable_child_return_fixture()?;
    let first_input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: first_turn,
        child_task_id: SubagentTaskId::try_new("retained-child")?,
        source_assistant_text_seq: first_text,
    })?;
    // A later instruction produces a different completed child turn under the same task identity.
    // 后续指令在相同任务身份下产生不同的已完成子回合。
    let second_turn = child.submit_root_input_fixture("follow up", Vec::new(), T0 + 10)?;
    let second_api = child.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 11,
    )?;
    let second_text = child
        .append_assistant_text(
            second_api,
            "follow-up child result",
            AssistantSemanticStatus::Completed,
            T0 + 12,
        )?
        .into_value();
    complete_with_usage(&mut child, second_api, usage(2, 3), T0 + 13)?;
    child.close_turn(TurnOutcome::Completed, 4, T0 + 14)?;
    advance_fixture_durable(&mut child)?;
    // The child's statistics sequence distinguishes completions without changing the task ID.
    // 子任务统计序号在不改变任务 ID 的情况下区分完成结果。
    let second_input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: second_turn,
        child_task_id: SubagentTaskId::try_new("retained-child")?,
        source_assistant_text_seq: second_text,
    })?;
    assert_ne!(
        first_input.child_turn_stats_seq,
        second_input.child_turn_stats_seq
    );
    // Both deliveries belong to one still-open parent turn and survive exact retries independently.
    // 两次投递属于同一仍开放的父回合，且各自的精确重试均保持幂等。
    let mut parent = new_document();
    parent.submit_root_input_fixture("delegate and follow up", Vec::new(), T0)?;
    let first = parent
        .append_subagent_return(first_input.clone(), T0 + 15)?
        .into_value();
    let second = parent
        .append_subagent_return(second_input.clone(), T0 + 16)?
        .into_value();
    assert_eq!(first.parent_turn_id, second.parent_turn_id);
    assert_ne!(first.return_seq, second.return_seq);
    assert!(
        !parent
            .append_subagent_return(first_input, T0 + 17)?
            .changed()
    );
    assert!(
        !parent
            .append_subagent_return(second_input, T0 + 18)?
            .changed()
    );
    advance_fixture_durable(&mut parent)?;
    SessionDocument::from_parts(
        parent.metadata,
        parent.runtime_lock,
        parent.compactions,
        parent.segment_catalog,
        parent.events,
    )?;
    Ok(())
}

/// Child return selection rejects non-durable state and an imprecise final text seq.
/// 子代理回流选择会拒绝未可靠状态与不精确的最终正文 seq。
#[test]
fn subagent_return_builder_requires_durable_exact_terminal_child() -> Result<(), DomainError> {
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let request = SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-final")?,
        source_assistant_text_seq,
    };

    let mut dirty = child.clone();
    dirty.rename_session("renamed child", T0 + 20)?;
    assert!(matches!(
        dirty.build_subagent_return_input(request.clone()),
        Err(DomainError::SubagentSourceNotDurable { .. })
    ));

    let mut wrong_source = request;
    wrong_source.source_assistant_text_seq = child
        .events()
        .iter()
        .find_map(|event| matches!(&event.event, EventBody::ToolCall(_)).then_some(event.seq))
        .expect("child tool event");
    assert!(matches!(
        child.build_subagent_return_input(wrong_source),
        Err(DomainError::InvalidStatistics {
            field: "source_assistant_text"
        })
    ));
    Ok(())
}

/// Identity drift, broken atomic pairs, and allocator-frontier failure never partially commit.
/// 身份漂移、破损原子事件对与分配器前沿失败绝不会产生部分提交。
#[test]
fn subagent_return_conflict_and_failure_paths_are_strict_and_atomic() -> Result<(), DomainError> {
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-final")?,
        source_assistant_text_seq,
    })?;

    let mut parent = new_document();
    parent.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    parent.append_subagent_return(input.clone(), T0 + 1)?;
    let mut drifted = input.clone();
    drifted.content.push_str(" changed");
    let before_drift = parent.clone();
    assert!(matches!(
        parent.append_subagent_return(drifted, T0 + 2),
        Err(DomainError::ImmutableFieldMutation {
            entity: "subagent_return",
            ..
        })
    ));
    assert_eq!(parent, before_drift);

    let mut broken_events = parent.events.clone();
    // A historical paired return must still reject its missing statistics partner.
    // 历史成对回流仍须拒绝缺失的统计配对。
    if let EventBody::SubagentReturn(returned) = &mut broken_events.last_mut().unwrap().event {
        returned.content_only = false;
    } else {
        panic!("content return expected");
    }
    assert!(matches!(
        SessionDocument::from_parts(
            parent.metadata.clone(),
            parent.runtime_lock.clone(),
            parent.compactions.clone(),
            parent.segment_catalog.clone(),
            broken_events,
        ),
        Err(DomainError::InvalidStatistics {
            field: "subagent_return_stats_pair"
        })
    ));

    let mut wrong_visibility = parent.events.clone();
    let stats_event = wrong_visibility
        .iter_mut()
        .find(|event| matches!(&event.event, EventBody::SubagentReturn(_)))
        .expect("subagent return event");
    stats_event.visibility = EventVisibility::UiOnly;
    assert!(matches!(
        SessionDocument::from_parts(
            parent.metadata.clone(),
            parent.runtime_lock.clone(),
            parent.compactions.clone(),
            parent.segment_catalog.clone(),
            wrong_visibility,
        ),
        Err(DomainError::InvalidStatistics {
            field: "subagent_return.visibility"
        })
    ));

    let mut exhausted = new_document();
    exhausted.submit_root_input_fixture("delegate", Vec::new(), T0)?;
    let (next_local_id, next_seq) = exhausted.id_allocator.frontier();
    exhausted.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before_exhausted = exhausted.clone();
    assert!(matches!(
        exhausted.append_subagent_return(input, T0 + 3),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(exhausted, before_exhausted);
    Ok(())
}
