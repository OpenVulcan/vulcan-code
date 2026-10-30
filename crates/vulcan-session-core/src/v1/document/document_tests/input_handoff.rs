//! Regression coverage for final-answer insertion batches and deferred ordinary queues.
//! 最终回答插队批次与后置普通队列的回归覆盖。

use super::*;

mod boundaries;

/// Verifies that terminal Goal notices preserve an insertion's exact handoff proof.
/// 验证目标终态提示保留插队的精确交接证明；无参数，返回测试结果。
#[test]
fn goal_stop_notice_preserves_interjection_handoff_reload() -> Result<(), DomainError> {
    // An unknown budget result emits a GoalStatus note before the batch root.
    // 未知预算结果会在批次根之前发出 GoalStatus 提示。
    let mut doc = new_document();
    let previous_turn = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    doc.create_goal("finish", Some(1), "Keep working.", T0 + 1)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    // A real queue promotion makes strict reload prove the cross-Turn ownership change.
    // 真实队列提升使严格重载必须证明跨回合归属变化。
    let action = ClientActionId::try_new("goal-insertion")?;
    let input_id = doc
        .enqueue_input(
            ClientInputSubmission::new(action.clone(), "insert", Vec::new()),
            T0 + 3,
        )?
        .into_value()
        .input_id;
    doc.promote_queued_input_to_pending_turn(
        QueuedInputIdentity {
            client_action_id: action,
            input_id,
        },
        T0 + 4,
    )?;
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0 + 5)?;
    complete_ok(&mut doc, api, None, T0 + 6)?;
    let transition = doc.complete_turn_and_select_successor(previous_turn, T0 + 7)?;
    assert!(
        matches!(transition.successor, TurnSuccessor::QueuedUser { input_id: actual, .. }
        if actual == input_id)
    );
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Blocked);
    SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    Ok(())
}

/// Verifies ordered batch handoff, FIFO priority, original identities, reload and failure atomicity.
/// 验证批次有序交接、队列优先级、原身份、重载和失败原子性；无参数，返回测试结果。
#[test]
fn final_answer_hands_off_all_insertions_before_ordinary_queue() -> Result<(), DomainError> {
    // Keep an ordinary queue item older than the insertions to prove explicit batch priority.
    // 让普通队列项早于插队，验证明确的批次优先级。
    let mut doc = new_document();
    // The predecessor owns one successful tool-free physical request.
    // 前驱持有一次成功且无工具的物理请求。
    let previous_turn = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    // Preserve the original FIFO queue independently of insertion arrival order.
    // 独立于插队到达顺序保留原先进先出队列。
    let mut ordinary_ids = Vec::new();
    for (position, text) in ["ordinary-first", "ordinary-second"]
        .into_iter()
        .enumerate()
    {
        ordinary_ids.push(
            doc.enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new(format!("ordinary-{position}"))?,
                    text,
                    Vec::new(),
                ),
                T0 + 2 + position as i64,
            )?
            .into_value()
            .input_id,
        );
    }
    // The last captured request choice becomes the batch choice without changing ordinary items.
    // 最后捕获的请求选择成为批次选择，不改变普通队列项。
    let batch_config = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        model: Some(String::from("batch-model")),
        connection_id: None,
        thinking_level: None,
        temperature: None,
    };
    // Each insertion keeps separate text, resource preparation and the original queue identity.
    // 每条插队保留独立正文、资源准备与原队列身份。
    let mut insertion_ids = Vec::new();
    for (position, text) in ["insert-first", "insert-second", "insert-third"]
        .into_iter()
        .enumerate()
    {
        // Distinct resources expose accidental merging or dropping of attachment owners.
        // 不同资源用于暴露附件所有者被意外合并或丢弃。
        let resource = ResourceId::try_new(100 + position as u64)?;
        let action = ClientActionId::try_new(format!("insert-{position}"))?;
        let id = doc
            .enqueue_input(
                ClientInputSubmission::new(action.clone(), text, vec![resource])
                    .with_bound_request_config(batch_config.clone()),
                T0 + 4 + position as i64,
            )?
            .into_value()
            .input_id;
        doc.promote_queued_input_to_pending_turn(
            QueuedInputIdentity {
                client_action_id: action,
                input_id: id,
            },
            T0 + 7 + position as i64,
        )?;
        insertion_ids.push(id);
    }
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0 + 10)?;
    complete_ok(&mut doc, api, None, T0 + 11)?;
    assert_eq!(
        doc.metadata.input_state.pending_in_turn_inputs.len(),
        insertion_ids.len()
    );
    assert!(doc.events().iter().all(|event| !matches!(&event.event,
        EventBody::UserInput(input) if insertion_ids.contains(&input.input_id))));

    // Exhausted allocation must leave both queues and the old open Turn exactly unchanged.
    // 分配耗尽必须使两类队列及旧开放回合完全保持原状。
    let (_, next_seq) = doc.id_allocator.frontier();
    let mut exhausted = doc.clone();
    exhausted.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_failure = exhausted.clone();
    assert!(
        exhausted
            .complete_turn_and_select_successor(previous_turn, T0 + 12)
            .is_err()
    );
    assert_eq!(exhausted, before_failure);

    // Closure and every new input commit together, with only one successor Turn.
    // 关闭与全部新输入共同提交，且只有一个后继回合。
    let transition = doc.complete_turn_and_select_successor(previous_turn, T0 + 12)?;
    let TurnSuccessor::QueuedUser {
        turn_id,
        input_seq,
        input_id,
        ..
    } = transition.successor
    else {
        panic!("the complete insertion batch must own one queued successor");
    };
    assert_ne!(turn_id, previous_turn);
    assert_eq!(Some(&input_id), insertion_ids.first());
    assert_eq!(doc.metadata.config.active_request, batch_config);
    assert!(doc.metadata.input_state.pending_in_turn_inputs.is_empty());
    assert_eq!(
        doc.metadata
            .input_state
            .queued_inputs
            .iter()
            .map(|input| input.input_id)
            .collect::<Vec<_>>(),
        ordinary_ids
    );
    // Read the real persisted batch boundary rather than an assumed event index.
    // 读取真实持久批次边界，不假设事件下标。
    let batch = doc.initial_queued_input_batch(turn_id, input_seq)?;
    assert_eq!(
        batch
            .iter()
            .map(|(_, input)| input.input_id)
            .collect::<Vec<_>>(),
        insertion_ids
    );
    assert_eq!(
        batch
            .iter()
            .map(|(_, input)| input.text.as_str())
            .collect::<Vec<_>>(),
        ["insert-first", "insert-second", "insert-third"]
    );
    assert_eq!(
        batch
            .iter()
            .flat_map(|(_, input)| &input.resource_ids)
            .copied()
            .collect::<Vec<_>>(),
        [
            ResourceId::try_new(100)?,
            ResourceId::try_new(101)?,
            ResourceId::try_new(102)?
        ]
    );
    assert!(batch.iter().all(|(event, _)| event.turn_id == Some(turn_id)
        && event.seq > transition.completed_turn_final_seq));

    // Strict reconstruction must accept the old queue receipts and the new batch ownership together.
    // 严格重建必须同时接纳旧队列回执与新批次归属。
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let before_repeat = restored.clone();
    assert!(
        restored
            .complete_turn_and_select_successor(previous_turn, T0 + 13)
            .is_err()
    );
    assert_eq!(restored, before_repeat);
    // A forged root source cannot turn an arbitrary cross-Turn input into an accepted handoff.
    // 伪造根来源不能把任意跨回合输入变成可接纳的交接。
    let mut invalid_events = doc.events().to_vec();
    let root = invalid_events
        .iter_mut()
        .find(|event| event.seq == input_seq)
        .expect("the successor root was just committed");
    root.mutate_in_place(T0 + 13, |body| {
        if let EventBody::UserInput(input) = body {
            input.input_source = UserInputSource::Root;
        }
    })?;
    assert!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog().to_vec(),
            invalid_events
        )
        .is_err()
    );

    // The ordinary FIFO head is selected only after the insertion batch receives its own answer.
    // 只有插队批次收到自身回答后，才选择普通先进先出队首。
    let request = ApiRequestSnapshot::new("openai", "batch-model");
    let next_api = restored.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0 + 14)?;
    restored.append_assistant_text(
        next_api,
        "batch done",
        AssistantSemanticStatus::Completed,
        T0 + 15,
    )?;
    complete_ok_for_request(&mut restored, next_api, request, None, T0 + 16)?;
    let after_batch = restored.complete_turn_and_select_successor(turn_id, T0 + 17)?;
    assert!(
        matches!(after_batch.successor, TurnSuccessor::QueuedUser { input_id, .. }
        if Some(&input_id) == ordinary_ids.first())
    );
    assert_eq!(
        restored
            .metadata
            .input_state
            .queued_inputs
            .iter()
            .map(|input| input.input_id)
            .collect::<Vec<_>>(),
        ordinary_ids.into_iter().skip(1).collect::<Vec<_>>()
    );
    Ok(())
}

/// Verifies that a single direct insertion remains idempotent after its new-Turn handoff.
/// 验证单条直接插队在新回合交接后仍幂等；无参数，返回测试结果。
#[test]
fn direct_insertion_handoff_preserves_submission_idempotency() -> Result<(), DomainError> {
    // One canonical direct insertion must never become a second accepted submission on retry.
    // 一条规范直接插队在重试时绝不能变为第二次接纳。
    let mut doc = new_document();
    let previous_turn = doc.submit_root_input_fixture("root", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("direct-handoff")?,
        "direct",
        Vec::new(),
    );
    let input_id = doc
        .add_pending_in_turn_input(submission.clone(), T0 + 2)?
        .into_value();
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0 + 3)?;
    complete_ok(&mut doc, api, None, T0 + 4)?;
    doc.complete_turn_and_select_successor(previous_turn, T0 + 5)?;
    // Repeating the original action acknowledges its stable input instead of appending work.
    // 重复原动作只确认其稳定输入，不追加工作。
    let before_retry = doc.clone();
    assert_eq!(
        doc.add_pending_in_turn_input(submission, T0 + 6)?,
        IdempotentMutation::Idempotent(input_id)
    );
    assert_eq!(doc, before_retry);
    SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    Ok(())
}
