//! Regression tests for insertion batch boundaries and historical forks.
//! 插队批次边界与历史分叉的回归测试。

use super::*;

/// Creates two queue-origin insertions and returns their predecessor completion receipt.
/// 创建两条来自队列的插队并返回前驱完成回执；无参数，失败返回领域错误。
fn handed_off_document() -> Result<(SessionDocument, TurnCompletionTransition), DomainError> {
    // Queue resolution records must be present to exercise strict cross-Turn ownership.
    // 必须存在队列处置记录，才能覆盖严格的跨回合归属。
    let mut document = new_document();
    // The predecessor owns the original physical request.
    // 前驱持有原始物理请求。
    let turn_id = document.submit_root_input_fixture("original", vec![], T0)?;
    // This tool-free request ends while the two insertions are still unconsumed.
    // 此无工具请求在两条插队仍未消费时结束。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    for (position, text) in ["first insertion", "second insertion"]
        .into_iter()
        .enumerate()
    {
        // Keep independent action and attachment identities for both batch members.
        // 为两条批次成员保留独立的动作与附件身份。
        let action = ClientActionId::try_new(format!("handoff-{position}"))?;
        // Each pending input retains its original queue identity after handoff.
        // 每条待处理输入在交接后保留原始队列身份。
        let input_id = document
            .enqueue_input(
                ClientInputSubmission::new(
                    action.clone(),
                    text,
                    vec![ResourceId::try_new(100 + position as u64)?],
                ),
                T0 + 2 + position as i64,
            )?
            .into_value()
            .input_id;
        document.promote_queued_input_to_pending_turn(
            QueuedInputIdentity {
                client_action_id: action,
                input_id,
            },
            T0 + 4 + position as i64,
        )?;
    }
    document.append_assistant_text(
        api,
        "original answer",
        AssistantSemanticStatus::Completed,
        T0 + 6,
    )?;
    complete_ok(&mut document, api, None, T0 + 7)?;
    // The receipt identifies both the closed predecessor and the single new batch Turn.
    // 回执同时标识已关闭的前驱与唯一的新批次回合。
    let completion = document.complete_turn_and_select_successor(turn_id, T0 + 8)?;
    Ok((document, completion))
}

/// Publishes every pending revision of `document`; returns an error when publication fails.
/// 发布 `document` 的全部待处理修订；发布失败时返回领域错误。
fn make_handoff_durable(document: &mut SessionDocument) -> Result<(), DomainError> {
    if let Some(candidate) = document.prepare_flush_candidates()? {
        document.advance_durable(candidate.revision)?;
    }
    Ok(())
}

/// Rebuilds `document` from its exact stored parts and returns the validated document.
/// 从 `document` 的精确存储部分重建并返回已校验文档，失败返回领域错误。
fn reload_handoff(document: &SessionDocument) -> Result<SessionDocument, DomainError> {
    SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
}

/// Forks `document` at `boundary` under `child_id` and returns a strictly reconstructed child.
/// 在 `boundary` 处以 `child_id` 分叉 `document`，返回严格重建的子会话或领域错误。
fn fork_handoff(
    document: &SessionDocument,
    boundary: Seq,
    child_id: &str,
) -> Result<SessionDocument, DomainError> {
    // Fork construction must exclude future work without rewriting the source document.
    // 分叉构建必须排除未来工作，同时不能改写来源文档。
    let output = document.build_fork_generation(ForkGenerationInput {
        child_session_id: child_id.to_owned(),
        title: "fork".to_owned(),
        source_turn_end_seq: boundary,
        now_ms: T0 + 30,
    })?;
    SessionDocument::from_parts(
        output.metadata,
        output.runtime_lock,
        output.compactions,
        vec![],
        output.events,
    )
}

/// Verifies that legitimate adjacent guidance ends the initial batch and survives reload.
/// 验证合法相邻引导会结束初始批次且能够重载；无参数，失败返回领域错误。
#[test]
fn adjacent_agent_guidance_preserves_batch_identity_and_reload() -> Result<(), DomainError> {
    // Freeze the exact canonical batch before any later request-boundary guidance.
    // 在任何后续请求边界引导之前冻结精确规范批次。
    let (mut document, completion) = handed_off_document()?;
    // The queued successor receipt owns the initial batch lookup.
    // 排队后继回执持有初始批次查找身份。
    let TurnSuccessor::QueuedUser {
        turn_id, input_seq, ..
    } = completion.successor
    else {
        panic!("insertions must open one queued successor");
    };
    // Clone the original inputs so identities, resources and preparation are compared together.
    // 克隆原输入，以同时比较身份、资源与准备事实。
    let original_inputs = document
        .initial_queued_input_batch(turn_id, input_seq)?
        .iter()
        .map(|(_, input)| (*input).clone())
        .collect::<Vec<_>>();
    // Collaboration envelopes and task steering both use this public request-boundary operation.
    // 协作信封与任务方向引导均使用这个公开请求边界操作。
    let guidance_seq = document
        .append_in_turn_guidance_before_api(
            ClientActionId::try_new("adjacent-guidance")?,
            "guidance".to_owned(),
            T0 + 9,
        )?
        .into_value();
    // An additional ordinary pending input after guidance must not be absorbed into the initial batch.
    // 引导之后额外的普通待处理输入不能被吸入初始批次。
    let late_id = document
        .add_pending_in_turn_input(
            ClientInputSubmission::new(ClientActionId::try_new("later-input")?, "later", vec![]),
            T0 + 10,
        )?
        .into_value();
    document.promote_pending_in_turn_inputs_after_api(T0 + 11)?;
    // Reload uses the same validation as persisted sessions.
    // 重载使用与持久会话相同的校验。
    let restored = reload_handoff(&document)?;
    // Only the original members belong to the scheduled batch, although later inputs remain in history.
    // 只有原成员属于调度批次，但后续输入仍保留在历史中。
    let actual_inputs = restored
        .initial_queued_input_batch(turn_id, input_seq)?
        .iter()
        .map(|(_, input)| (*input).clone())
        .collect::<Vec<_>>();
    assert_eq!(actual_inputs, original_inputs);
    assert!(restored.events().iter().any(|event| event.seq == guidance_seq
        && matches!(&event.event, EventBody::UserInput(input) if input.input_source == UserInputSource::AgentMessage)));
    assert!(restored.events().iter().any(
        |event| matches!(&event.event, EventBody::UserInput(input) if input.input_id == late_id)
    ));
    // Ending at another source cannot bless a forged original member that was changed into guidance.
    // 遇到其他来源结束不能放行被伪装为引导的原批次成员。
    let member_id = original_inputs
        .last()
        .expect("fixture has insertion members")
        .input_id;
    let mut forged_events = restored.events().to_vec();
    let forged = forged_events.iter_mut().find(|event|
        matches!(&event.event, EventBody::UserInput(input) if input.input_id == member_id))
        .expect("the original member is retained");
    forged.mutate_in_place(T0 + 12, |body| {
        if let EventBody::UserInput(input) = body {
            input.input_source = UserInputSource::AgentMessage;
        }
    })?;
    assert!(
        SessionDocument::from_parts(
            restored.metadata,
            restored.runtime_lock,
            restored.compactions,
            vec![],
            forged_events
        )
        .is_err()
    );
    Ok(())
}

/// Verifies predecessor forks retire only excluded handoffs and remain forkable again.
/// 验证前驱分叉仅退休被排除的交接且仍可再次分叉；无参数，失败返回领域错误。
#[test]
fn predecessor_fork_retires_outside_batch_without_changing_source() -> Result<(), DomainError> {
    // The source contains queue receipts before the fork boundary and their inputs after it.
    // 来源在分叉边界前包含队列回执，在边界后包含其输入。
    let (mut document, completion) = handed_off_document()?;
    make_handoff_durable(&mut document)?;
    // Source identity and content must survive this pure generation operation.
    // 此纯代次操作必须保留来源身份与内容。
    let original = document.clone();
    let child = fork_handoff(&document, completion.completed_turn_final_seq, "first-fork")?;
    assert_eq!(document, original);
    assert!(child.active_turn().is_none());
    assert!(child.metadata.input_state.queued_inputs.is_empty());
    assert!(child.metadata.input_state.pending_in_turn_inputs.is_empty());
    for event in original
        .events()
        .iter()
        .filter(|event| event.seq <= completion.completed_turn_final_seq)
    {
        // Compare by stable sequence identity instead of relying on evolving positions.
        // 按稳定序列身份比较，不依赖会演进的位置。
        let copied = child
            .events()
            .iter()
            .find(|copied| copied.seq == event.seq)
            .expect("fork retains every prefix identity");
        if let EventBody::QueuedInputResolution(resolution) = &event.event {
            // Only the disposition and owner change when this branch excludes the entire batch.
            // 本分支排除整个批次时，仅处置结果与所有者发生变化。
            let EventBody::QueuedInputResolution(retired) = &copied.event else {
                panic!("resolution audit must remain");
            };
            let mut expected = resolution.clone();
            expected.resolution = QueuedInputResolutionKind::Removed;
            assert_eq!(retired, &expected);
            assert_eq!(copied.turn_id, None);
            assert_eq!(copied.created_at_ms, event.created_at_ms);
            assert_eq!(copied.event_revision, event.event_revision + 1);
        } else {
            assert_eq!(copied, event);
        }
    }
    // A second fork cannot depend on the original parent's now-inaccessible future events.
    // 二次分叉不能依赖此时不可访问的原父会话未来事件。
    let second = fork_handoff(&child, completion.completed_turn_final_seq, "second-fork")?;
    assert!(second.active_turn().is_none());
    assert_eq!(child.events(), second.events());
    Ok(())
}

/// Verifies forks through the answered batch preserve its original handoff audit unchanged.
/// 验证分叉包含已回答批次时原交接审计完全保留；无参数，失败返回领域错误。
#[test]
fn fork_including_batch_keeps_resolution_and_message_owners() -> Result<(), DomainError> {
    // Answer the new batch so the selected fork includes every referenced message.
    // 回答新批次，使所选分叉包含每条被引用的消息。
    let (mut document, completion) = handed_off_document()?;
    let TurnSuccessor::QueuedUser { turn_id, .. } = completion.successor else {
        panic!("queued successor expected");
    };
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 9,
    )?;
    document.append_assistant_text(
        api,
        "batch answer",
        AssistantSemanticStatus::Completed,
        T0 + 10,
    )?;
    complete_ok(&mut document, api, None, T0 + 11)?;
    // This boundary follows the batch rather than preceding it.
    // 此边界位于批次之后而非之前。
    let boundary = document
        .complete_turn_and_select_successor(turn_id, T0 + 12)?
        .completed_turn_final_seq;
    make_handoff_durable(&mut document)?;
    let child = fork_handoff(&document, boundary, "whole-batch-fork")?;
    assert_eq!(child.events(), document.events());
    assert!(child.active_turn().is_none());
    Ok(())
}

/// Verifies that fork retirement cannot conceal invalid source identity or overflow an audit revision.
/// 验证分叉退休不能掩盖来源身份无效或使审计修订溢出；无参数，失败返回领域错误。
#[test]
fn fork_retirement_rejects_invalid_source_and_revision_exhaustion_atomically()
-> Result<(), DomainError> {
    // Both failure cases start from the exact same legitimate source handoff.
    // 两个失败场景从同一个合法来源交接开始。
    let (mut document, completion) = handed_off_document()?;
    make_handoff_durable(&mut document)?;
    // Corrupt a future member that the child would omit; it must still be validated before retirement.
    // 损坏子分支将省略的未来成员；退休之前仍必须校验它。
    let mut invalid = document.clone();
    let omitted = invalid
        .events
        .iter_mut()
        .find(|event| {
            event.seq > completion.completed_turn_final_seq
                && matches!(event.event, EventBody::UserInput(_))
        })
        .expect("handoff produced a future member");
    if let EventBody::UserInput(input) = &mut omitted.event {
        input.text = "forged".to_owned();
    }
    let invalid_before = invalid.clone();
    assert!(matches!(
        fork_handoff(
            &invalid,
            completion.completed_turn_final_seq,
            "invalid-fork"
        ),
        Err(DomainError::InvalidQueuedInputOrder {
            field: "promoted_resolution_stream_conflict"
        })
    ));
    assert_eq!(invalid, invalid_before);
    // Revision exhaustion must fail the pure builder before any caller can publish a partial fork.
    // 修订耗尽必须使纯构建器失败，避免调用方发布不完整分叉。
    let mut exhausted = document;
    let audit = exhausted
        .events
        .iter_mut()
        .find(|event| matches!(event.event, EventBody::QueuedInputResolution(_)))
        .expect("handoff contains a queue resolution");
    audit.event_revision = JS_SAFE_INTEGER_MAX;
    let exhausted_before = exhausted.clone();
    assert!(
        fork_handoff(
            &exhausted,
            completion.completed_turn_final_seq,
            "exhausted-fork"
        )
        .is_err()
    );
    assert_eq!(exhausted, exhausted_before);
    Ok(())
}
