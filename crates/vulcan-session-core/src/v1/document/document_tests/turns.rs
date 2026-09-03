//! Logical turn continuity, statistics, and successor-selection regression tests.
//! 逻辑回合连续性、统计及后继选择回归测试。

use super::*;

/// Abort then continue keeps the same turn id and never creates a fake user
/// prompt.
/// 中止后继续保持同一 turn id，且绝不创建假的用户提示词。
#[test]
fn abort_continue_keeps_turn_id() {
    let mut doc = new_document();
    let turn_id = doc
        .submit_root_input_fixture("first", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "partial", AssistantSemanticStatus::Interrupted, T0)
        .unwrap();
    complete_user_interrupted(&mut doc, api, T0 + 1).unwrap();
    doc.continue_turn_fixture(T0 + 2).unwrap();
    let api2 = doc
        .start_api(
            ApiPurposeKind::Continue,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api2, "resumed", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap();
    complete_ok(&mut doc, api2, None, T0 + 3).unwrap();
    doc.close_turn(TurnOutcome::Completed, 10, T0 + 3).unwrap();
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(turn_id));
    let user_inputs: Vec<&str> = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::UserInput(input) => Some(input.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        user_inputs,
        vec!["first"],
        "continue must not fake a prompt"
    );
    let resume_controls = doc
        .events()
        .iter()
        .filter(|event| {
            matches!(
                &event.event,
                EventBody::ExecutionControl(control)
                    if control.control_kind == ExecutionControlKind::TurnResumed
            )
        })
        .count();
    assert_eq!(resume_controls, 1);
}

/// Repeating stream-failure settlement must preserve an already interrupted API's Continue boundary.
/// 重复结算流失败必须保留已中断 API 的 Continue 边界。
#[test]
fn repeated_failure_keeps_interrupted_api_continuable() -> Result<(), DomainError> {
    let mut document = new_document();
    document.create_goal("work", None, "work", T0)?;
    document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    document.interrupt_active_api_for_process(T0 + 2)?;
    let before = document.clone();
    assert!(!document.fail_turn_without_active_api(
        "stream_finished_without_v1_api_terminal",
        None,
        T0 + 3
    )?);
    assert_eq!(document, before);
    assert!(!document.active_turn().unwrap().closed);
    Ok(())
}

/// Turn statistics aggregate direct APIs and final subagent returns.
/// 回合统计聚合 direct API 与最终子代理回流。
#[test]
fn turn_stats_aggregate_direct_and_subagent() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("multi", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    )?;
    doc.append_assistant_text(api, "answer", AssistantSemanticStatus::Completed, T0)?;
    complete_ok(&mut doc, api, None, T0 + 1)?;
    let (child, child_turn_id, source_assistant_text_seq) = durable_child_return_fixture()?;
    let input = child.build_subagent_return_input(SubagentReturnBuildRequest {
        final_turn_id: child_turn_id,
        child_task_id: SubagentTaskId::try_new("task-1")?,
        source_assistant_text_seq,
    })?;
    assert!(matches!(
        doc.append_subagent_return_with_stats(input, T0 + 2)?,
        IdempotentMutation::Committed(_)
    ));
    doc.close_turn(TurnOutcome::Completed, 5, T0 + 3)?;
    let (stats_turn_id, stats) = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(stats) => Some((event.turn_id, stats)),
            _ => None,
        })
        .expect("turn stats must exist");
    assert_eq!(stats.buckets.direct.api_call_count, 1);
    assert_eq!(stats.buckets.subagent.api_call_count, 2);
    assert_eq!(stats.buckets.total.api_call_count, 3);
    assert_eq!(stats.buckets.subagent.unknown_usage_api_call_count, 1);
    assert_eq!(stats.buckets.total.unknown_usage_api_call_count, 1);
    assert_eq!(stats.buckets.subagent.tool_call_count, 1);
    assert_eq!(stats.buckets.total.tool_call_count, 1);
    assert_eq!(stats.buckets.total.usage.input_tokens, None);
    assert_eq!(stats.buckets.total.usage.output_tokens, None);
    assert_eq!(stats.buckets.total.subagent_return_count, 1);
    assert_eq!(stats_turn_id, Some(turn_id));
    Ok(())
}

/// Turn statistics count Provider and tool work physically owned by a child TaskSession.
/// 回合统计会计入子 TaskSession 物理持有的 Provider 与工具工作。
#[test]
fn child_turn_stats_include_local_subagent_purpose_work() -> Result<(), DomainError> {
    let mut child = SessionDocument::new("child-local-session", "project-1", "child", T0);
    child.submit_root_input_fixture("run locally", Vec::new(), T0)?;
    let tool_api = child.start_api(
        ApiPurposeKind::Subagent,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool_call_id = child.start_tool(
        tool_api,
        "file",
        ToolPayload::try_new(serde_json::json!({"path": "child.txt"}))?,
        Some("provider-child-local-tool".to_string()),
        T0 + 2,
    )?;
    child.finish_tool(
        tool_api,
        tool_call_id,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )?;
    complete_ok(&mut child, tool_api, None, T0 + 4)?;
    let final_api = child.start_api(
        ApiPurposeKind::Subagent,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 5,
    )?;
    child.append_assistant_text(
        final_api,
        "done",
        AssistantSemanticStatus::Completed,
        T0 + 6,
    )?;
    complete_ok(&mut child, final_api, None, T0 + 7)?;
    child.close_turn(TurnOutcome::Completed, 8, T0 + 8)?;

    let stats = child
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(stats) => Some(stats),
            _ => None,
        })
        .expect("child turn stats must exist");
    assert_eq!(stats.buckets.direct.api_call_count, 2);
    assert_eq!(stats.buckets.direct.tool_call_count, 1);
    assert_eq!(stats.buckets.total.api_call_count, 2);
    assert_eq!(stats.buckets.total.tool_call_count, 1);
    assert_eq!(stats.buckets.total.usage.input_tokens, Some(20));
    assert_eq!(stats.buckets.total.usage.output_tokens, Some(40));
    Ok(())
}

/// Completing a turn and creating its active Goal successor is atomic and
/// receipt-free exactly-once by the completed turn id.
/// 完成回合并创建其活动 Goal 后继回合，以已完成 turn id 实现无 receipt 的原子
/// exactly-once。
#[test]
fn goal_continuation_exactly_once() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.metadata.mode.goal.enabled = true;
    doc.metadata.mode.goal.state = GoalState::Active;
    doc.metadata.mode.goal.objective = "objective".to_string();
    let completed_turn = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0)?;
    complete_ok(&mut doc, api, None, T0 + 1)?;

    let transition = doc.complete_turn_and_select_successor(completed_turn, T0 + 2)?;
    let TurnSuccessor::GoalContinuation {
        turn_id: goal_turn, ..
    } = transition.successor
    else {
        panic!("active Goal must open one successor turn");
    };
    assert_eq!(
        doc.active_turn()
            .map(|turn| (turn.turn_id, turn.origin, turn.closed)),
        Some((goal_turn, TurnOrigin::GoalContinuation, false))
    );
    assert_eq!(
        doc.metadata.open_turn_start_seq,
        doc.active_turn().map(|turn| turn.start_seq)
    );
    assert!(matches!(
        doc.events()
            .iter()
            .find(|event| event.seq() == transition.completed_turn_final_seq)
            .map(|event| &event.event),
        Some(EventBody::TurnStats(_))
    ));

    let event_count = doc.events().len();
    let duplicate = doc.complete_turn_and_select_successor(completed_turn, T0 + 3);
    assert!(matches!(
        duplicate,
        Err(DomainError::TurnIdentityMismatch { expected, actual })
            if expected == completed_turn.value() && actual == goal_turn.value()
    ));
    assert_eq!(doc.events().len(), event_count);
    assert_eq!(doc.active_turn().map(|turn| turn.turn_id), Some(goal_turn));
    Ok(())
}

/// Completing a turn with Goal inactive closes it without creating a successor.
/// Goal 未活动时完成回合，只闭合该回合而不创建后继回合。
#[test]
fn complete_turn_without_active_goal_stays_closed() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(&mut doc, api, None, T0 + 3)?;
    let transition = doc.complete_turn_and_select_successor(turn_id, T0 + 4)?;
    assert_eq!(transition.successor, TurnSuccessor::None);
    assert_eq!(doc.active_turn().map(|turn| turn.closed), Some(true));
    assert!(doc.metadata.open_turn_start_seq.is_none());
    Ok(())
}

/// The terminal turn statistic covers its own seq as the end.
/// 终态回合统计以自身 seq 作为覆盖结束。
#[test]
fn turn_stats_end_seq_includes_itself() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "x", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    complete_ok(&mut doc, api, None, T0 + 1).unwrap();
    let stats_seq = doc.close_turn(TurnOutcome::Completed, 1, T0 + 2).unwrap();
    let stats = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::TurnStats(stats) => Some(stats),
            _ => None,
        })
        .expect("turn stats must exist");
    assert_eq!(
        stats.end_seq.value(),
        stats_seq.value(),
        "turn stats must cover its own seq"
    );
}

/// Turn completion selects queued users before Goal continuation and preserves FIFO on restart.
/// 回合完成在 Goal 续跑前选择排队用户，并在重启后保持 FIFO。
#[test]
fn turn_completion_selects_one_queued_successor_before_goal() -> Result<(), DomainError> {
    let mut doc = new_document();
    let first_turn = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    doc.create_goal("finish", None, "Keep working.", T0 + 1)?;
    let queued_first_action = ClientActionId::try_new("queued-first")?;
    let queued_second_action = ClientActionId::try_new("queued-second")?;
    let queued_first_request = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: None,
        model: Some(String::from("gpt-5.1")),
        thinking_level: Some(String::from("high")),
        temperature: None,
    };
    let queued_second_request = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: None,
        model: Some(String::from("gpt-5.2")),
        thinking_level: Some(String::from("low")),
        temperature: None,
    };
    let queued_first = doc
        .enqueue_input(
            ClientInputSubmission::new(queued_first_action.clone(), "first queued", Vec::new())
                .with_bound_request_config(queued_first_request.clone()),
            T0 + 2,
        )?
        .into_value()
        .input_id;
    let queued_second = doc
        .enqueue_input(
            ClientInputSubmission::new(queued_second_action.clone(), "second queued", Vec::new())
                .with_bound_request_config(queued_second_request.clone()),
            T0 + 3,
        )?
        .into_value()
        .input_id;
    assert_eq!(
        doc.metadata.config.active_request,
        ActiveRequestConfig::default()
    );
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 4,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 5)?;
    complete_ok(&mut doc, api, None, T0 + 6)?;
    let transition = doc.complete_turn_and_select_successor(first_turn, T0 + 7)?;
    let TurnSuccessor::QueuedUser {
        turn_id: first_queued_turn,
        input_id,
        client_action_id,
        input_seq,
    } = transition.successor
    else {
        panic!("queued user must outrank active Goal");
    };
    assert_eq!(input_id, queued_first);
    assert_eq!(client_action_id, queued_first_action);
    assert_eq!(doc.metadata.config.active_request, queued_first_request);
    assert_eq!(doc.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        doc.metadata.input_state.queued_inputs[0].input_id,
        queued_second
    );
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Active);
    assert!(matches!(
        doc.events().iter().find(|event| event.seq == input_seq),
        Some(ConversationEvent {
            turn_id: Some(value),
            event: EventBody::UserInput(UserInputEvent {
                input_source: UserInputSource::QueuedScheduled,
                ..
            }),
            ..
        }) if *value == first_queued_turn
    ));

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    assert_eq!(
        restored.active_turn().map(|turn| turn.turn_id),
        Some(first_queued_turn)
    );
    assert_eq!(restored.metadata.input_state.queued_inputs.len(), 1);
    let mut first_queued_request_snapshot = ApiRequestSnapshot::new("openai", "gpt-5.1");
    first_queued_request_snapshot.thinking_level = Some(String::from("high"));
    let api = restored.start_api(
        ApiPurposeKind::MainGeneration,
        first_queued_request_snapshot.clone(),
        T0 + 8,
    )?;
    restored.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 9)?;
    complete_ok_for_request(
        &mut restored,
        api,
        first_queued_request_snapshot,
        None,
        T0 + 10,
    )?;
    let second = restored.complete_turn_and_select_successor(first_queued_turn, T0 + 11)?;
    let TurnSuccessor::QueuedUser {
        turn_id: second_queued_turn,
        input_id,
        client_action_id,
        ..
    } = second.successor
    else {
        panic!("second FIFO item must become the next queued successor");
    };
    assert_eq!(input_id, queued_second);
    assert_eq!(client_action_id, queued_second_action);
    assert_eq!(
        restored.metadata.config.active_request,
        queued_second_request
    );
    assert!(restored.metadata.input_state.queued_inputs.is_empty());

    let mut second_queued_request_snapshot = ApiRequestSnapshot::new("openai", "gpt-5.2");
    second_queued_request_snapshot.thinking_level = Some(String::from("low"));
    let api = restored.start_api(
        ApiPurposeKind::MainGeneration,
        second_queued_request_snapshot.clone(),
        T0 + 12,
    )?;
    restored.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 13)?;
    complete_ok_for_request(
        &mut restored,
        api,
        second_queued_request_snapshot,
        None,
        T0 + 14,
    )?;
    let after_queue = restored.complete_turn_and_select_successor(second_queued_turn, T0 + 15)?;
    assert!(matches!(
        after_queue.successor,
        TurnSuccessor::GoalContinuation { .. }
    ));
    Ok(())
}

/// Queued successor completion is failure-atomic without an active Goal.
/// 不存在活动 Goal 时，排队后继完成仍保持失败原子性。
#[test]
fn queued_successor_completion_is_atomic_without_goal() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    let queued_id = doc
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queued-no-goal")?,
                "queued",
                Vec::new(),
            ),
            T0 + 1,
        )?
        .into_value()
        .input_id;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    doc.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 3)?;
    complete_ok(&mut doc, api, None, T0 + 4)?;

    let (_, next_seq) = doc.id_allocator.frontier();
    let mut local_exhausted = doc.clone();
    local_exhausted.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_local_failure = local_exhausted.clone();
    assert!(
        local_exhausted
            .complete_turn_and_select_successor(turn_id, T0 + 5)
            .is_err()
    );
    assert_eq!(local_exhausted, before_local_failure);

    let (next_local_id, _) = doc.id_allocator.frontier();
    let mut seq_exhausted = doc.clone();
    seq_exhausted.id_allocator = IdAllocator::from_frontier(next_local_id, JS_SAFE_INTEGER_MAX + 1);
    let before_seq_failure = seq_exhausted.clone();
    assert!(
        seq_exhausted
            .complete_turn_and_select_successor(turn_id, T0 + 5)
            .is_err()
    );
    assert_eq!(seq_exhausted, before_seq_failure);

    let completed = doc.complete_turn_and_select_successor(turn_id, T0 + 5)?;
    assert!(matches!(
        completed.successor,
        TurnSuccessor::QueuedUser { input_id, .. } if input_id == queued_id
    ));
    Ok(())
}

/// Goal budget exhaustion is computed from persisted statistics and recovery
/// cannot manufacture a successor or replay the completed transition.
/// Goal 预算耗尽只从持久化统计计算，恢复后既不能制造后继回合，也不能重放已完成转换。
#[test]
fn goal_budget_exhaustion_and_successor_recovery_are_exactly_once() -> Result<(), DomainError> {
    let mut exhausted = new_document();
    exhausted.metadata.mode.goal.enabled = true;
    exhausted.metadata.mode.goal.state = GoalState::Active;
    exhausted.metadata.mode.goal.objective = "finish".to_string();
    exhausted.metadata.mode.goal.budget_tokens = Some(30);
    let turn = exhausted.submit_root_input_fixture("work", Vec::new(), T0)?;
    exhausted.metadata.mode.goal.origin_turn_id = Some(turn);
    exhausted.metadata.mode.goal.origin_seq = exhausted.active_turn().map(|value| value.start_seq);
    let api = exhausted.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    exhausted.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(&mut exhausted, api, None, T0 + 3)?;
    let transition = exhausted.complete_turn_and_select_successor(turn, T0 + 4)?;
    assert_eq!(transition.successor, TurnSuccessor::None);
    assert_eq!(
        exhausted.metadata.mode.goal.state,
        GoalState::BudgetExhausted
    );
    let cache = exhausted
        .metadata
        .mode
        .goal
        .budget_cache
        .as_ref()
        .expect("known persisted usage must build a cache");
    assert_eq!(cache.total_tokens, 30);
    assert_eq!(cache.covered_until_seq, transition.completed_turn_final_seq);

    let restored = SessionDocument::from_parts(
        exhausted.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        exhausted.events().to_vec(),
    )?;
    assert!(restored.active_turn().is_none());
    assert!(!restored.events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::SystemContext(context) if context.context_key == "goal_continuation"
        )
    }));

    let mut continuing = new_document();
    continuing.metadata.mode.goal.enabled = true;
    continuing.metadata.mode.goal.state = GoalState::Active;
    continuing.metadata.mode.goal.objective = "continue".to_string();
    let first = continuing.submit_root_input_fixture("work", Vec::new(), T0)?;
    continuing.metadata.mode.goal.origin_turn_id = Some(first);
    continuing.metadata.mode.goal.origin_seq =
        continuing.active_turn().map(|value| value.start_seq);
    let api = continuing.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    continuing.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(&mut continuing, api, None, T0 + 3)?;
    let completed = continuing.complete_turn_and_select_successor(first, T0 + 4)?;
    let TurnSuccessor::GoalContinuation {
        turn_id: successor, ..
    } = completed.successor
    else {
        panic!("active unbudgeted Goal must continue");
    };
    let mut recovered = SessionDocument::from_parts(
        continuing.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        continuing.events().to_vec(),
    )?;
    assert_eq!(
        recovered.active_turn().map(|value| value.turn_id),
        Some(successor)
    );
    assert!(matches!(
        recovered.complete_turn_and_select_successor(first, T0 + 5),
        Err(DomainError::TurnIdentityMismatch { expected, actual })
            if expected == first.value() && actual == successor.value()
    ));
    Ok(())
}
