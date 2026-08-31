//! Tool preparation, progress, waiting, completion, and replay regression tests.
//! 工具准备、进度、等待、完成及重放回归测试。

use super::*;

/// 1,000 status updates of the same tool never allocate a new seq.
/// 同一个工具的 1,000 次状态更新绝不分配新 seq。
#[test]
fn one_thousand_tool_updates_keep_one_seq() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("do it", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let tool = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let seq = doc
        .tool_seq_by_id
        .get(&tool)
        .copied()
        .expect("tool must be indexed");
    for index in 0..1_000u64 {
        doc.update_running_tool_output(
            api,
            tool,
            Some(ToolPayload::try_new(serde_json::json!({"progress": index})).unwrap()),
            T0 + index as i64,
        )
        .unwrap();
    }
    assert_eq!(
        doc.tool_seq_by_id.get(&tool).copied(),
        Some(seq),
        "tool seq must never change"
    );
    let tool_event_count = doc
        .events()
        .iter()
        .filter(|event| matches!(&event.event, EventBody::ToolCall(t) if t.tool_call_id == tool))
        .count();
    assert_eq!(tool_event_count, 1, "tool must exist as exactly one event");
}

/// Parallel tool completions never reorder cards because each completion
/// updates the tool's original event in place.
/// 并行工具完成绝不重排卡片，因为每次完成都原位更新工具原事件。
#[test]
fn parallel_tool_finishes_keep_card_order() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tools", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let first = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({"n": 1})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let second = doc
        .start_tool(
            api,
            "file_read",
            ToolPayload::try_new(serde_json::json!({"n": 2})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    let third = doc
        .start_tool(
            api,
            "web",
            ToolPayload::try_new(serde_json::json!({"n": 3})).unwrap(),
            None,
            T0,
        )
        .unwrap();
    doc.finish_tool(
        api,
        third,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    doc.finish_tool(
        api,
        first,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 2,
        ),
    )
    .unwrap();
    doc.finish_tool(
        api,
        second,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )
    .unwrap();
    let tool_ids_in_stream: Vec<u64> = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) => Some(tool.tool_call_id.value()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_ids_in_stream,
        vec![first.value(), second.value(), third.value()]
    );
}

/// Tools after the api stats are rejected at write time.
/// 在 api stats 之后写工具会被写时拒绝。
#[test]
fn tool_after_stats_is_rejected() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    complete_ok(&mut doc, api, None, T0 + 1).unwrap();
    let result = doc.start_tool(
        api,
        "shell",
        ToolPayload::try_new(serde_json::json!({})).unwrap(),
        None,
        T0 + 2,
    );
    // A closed group has no active api; the late tool is rejected at write
    // time as a cross-group update.
    // 已闭合的组没有活动 api；迟到工具在写时作为跨组更新被拒绝。
    assert!(matches!(
        result,
        Err(DomainError::CrossApiGroupUpdate { .. })
    ));
}

/// Non-object tool payloads are rejected at construction, keeping raw
/// unvalidated JSON out of the main event stream (audit P0#9).
/// 非对象工具载荷在构造时被拒绝，使未经验证的原始 JSON 无法进入主事件流
/// (审计 P0#9)。
#[test]
fn tool_payload_rejects_non_object_containers() {
    assert!(ToolPayload::try_new(serde_json::json!(1)).is_err());
    assert!(ToolPayload::try_new(serde_json::json!("x")).is_err());
    assert!(ToolPayload::try_new(serde_json::json!([1, 2])).is_err());
    assert!(ToolPayload::try_new(serde_json::json!(null)).is_err());
    assert!(ToolPayload::try_new(serde_json::json!({"ok": true})).is_ok());
}

/// Batch preparation preserves Provider order, while an exact repeated start is idempotent.
/// 批量准备保留 Provider 原始顺序，而精确重复启动是幂等操作。
#[test]
fn prepared_tool_batch_preserves_order_and_idempotent_start() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let ids = doc
        .prepare_tool_batch(
            api,
            vec![
                ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("stream-first").unwrap(),
                    tool_name: "first".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"order": 1}))?,
                    provider_tool_call_id: Some("provider-1".to_string()),
                },
                ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("stream-second").unwrap(),
                    tool_name: "second".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"order": 2}))?,
                    provider_tool_call_id: Some("provider-2".to_string()),
                },
                ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("stream-third").unwrap(),
                    tool_name: "third".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"order": 3}))?,
                    provider_tool_call_id: Some("provider-3".to_string()),
                },
            ],
            T0 + 1,
        )?
        .into_value();
    let ids = ids
        .tools
        .iter()
        .map(|receipt| receipt.tool_call_id)
        .collect::<Vec<_>>();
    let prepared = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) => Some((event.seq(), tool.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(prepared.len(), 3);
    assert_eq!(
        prepared
            .iter()
            .map(|(_, tool)| tool.tool_name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "third"]
    );
    assert!(prepared.windows(2).all(|pair| {
        pair[1].0.value() == pair[0].0.value() + 1
            && pair[0].1.status == ToolCallStatus::Prepared
            && pair[0].1.started_at_ms.is_none()
    }));
    assert_eq!(prepared[2].1.status, ToolCallStatus::Prepared);
    let second_seq = prepared[1].0;

    assert!(doc.start_prepared_tool(api, ids[1], T0 + 2)?);
    let started = doc
        .events()
        .iter()
        .find(|event| event.seq() == second_seq)
        .expect("prepared tool seq must remain present");
    assert!(matches!(
        &started.event,
        EventBody::ToolCall(tool)
            if tool.status == ToolCallStatus::Running
                && tool.started_at_ms == Some(T0 + 2)
    ));
    let revision_before_replay = doc.memory_revision();
    assert!(!doc.start_prepared_tool(api, ids[1], T0 + 3)?);
    assert_eq!(doc.memory_revision(), revision_before_replay);
    assert!(matches!(
        doc.events()
            .iter()
            .find(|event| event.seq() == second_seq)
            .map(|event| &event.event),
        Some(EventBody::ToolCall(tool)) if tool.started_at_ms == Some(T0 + 2)
    ));
    let unknown = ToolCallId::try_new(9_000_000)?;
    assert!(matches!(
        doc.start_prepared_tool(api, unknown, T0 + 3),
        Err(DomainError::ReferenceViolation {
            kind: "tool_call_id",
            ..
        })
    ));
    Ok(())
}

/// Tool progress transitions commit once and exact replays do not create revisions.
/// 工具进度转换仅提交一次，精确重放不会制造修订号。
#[test]
fn tool_progress_transitions_are_idempotent() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-progress-idempotent")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 2,
    )?;

    assert!(doc.start_prepared_tool(api, tool, T0 + 3)?);
    let started_revision = doc.memory_revision();
    assert!(!doc.start_prepared_tool(api, tool, T0 + 30)?);
    assert_eq!(doc.memory_revision(), started_revision);

    let progress = Some(ToolPayload::try_new(serde_json::json!({"progress": 50}))?);
    assert!(doc.update_running_tool_output(api, tool, progress.clone(), T0 + 4)?);
    let progress_revision = doc.memory_revision();
    assert!(!doc.update_running_tool_output(api, tool, progress, T0 + 40)?);
    assert_eq!(doc.memory_revision(), progress_revision);

    assert!(doc.mark_tool_waiting_user(api, tool, T0 + 5)?);
    let waiting_revision = doc.memory_revision();
    assert!(!doc.mark_tool_waiting_user(api, tool, T0 + 50)?);
    assert_eq!(doc.memory_revision(), waiting_revision);
    Ok(())
}

/// Every same-seq tool lifecycle failure leaves the complete document unchanged.
/// 任一同 seq 工具生命周期失败都会保持完整文档不变。
#[test]
fn tool_lifecycle_revision_failures_are_atomic() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-lifecycle-overflow")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 2,
    )?;
    let tool_seq = doc.tool_seq_by_id[&tool];
    let index =
        usize::try_from(tool_seq.value() - doc.resident_start_seq).expect("resident fixture index");
    doc.events[index].event_revision = JS_SAFE_INTEGER_MAX;
    let before_start = doc.clone();
    assert!(matches!(
        doc.start_prepared_tool(api, tool, T0 + 3),
        Err(DomainError::InvalidId {
            entity: "event_revision",
            ..
        })
    ));
    assert_eq!(doc, before_start);

    doc.events[index].event_revision = 1;
    assert!(doc.start_prepared_tool(api, tool, T0 + 3)?);
    assert!(doc.mark_tool_waiting_user(api, tool, T0 + 4)?);
    doc.events[index].event_revision = JS_SAFE_INTEGER_MAX;
    let before_answer = doc.clone();
    assert!(matches!(
        doc.answer_waiting_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?),
                ToolCallStatus::Completed,
                T0 + 5,
            ),
        ),
        Err(DomainError::InvalidId {
            entity: "event_revision",
            ..
        })
    ));
    assert_eq!(doc, before_answer);
    Ok(())
}

/// A crash interrupts a durably Prepared tool because no exactly-once tool
/// process recovery scheduler exists; the same seq receives an explicit error
/// output that the next Provider request can replay.
/// 崩溃会中断已可靠 Prepared 的工具，因为不存在 exactly-once 工具进程恢复调度器；
/// 同一 seq 会得到明确错误输出，供下一次 Provider 请求重放。
#[test]
fn crash_interrupts_prepared_tool_with_replayable_error_output() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-provider-tool")?,
        "shell",
        ToolPayload::try_new(serde_json::json!({"command": "echo ready"}))?,
        Some("provider-tool".to_string()),
        T0 + 1,
    )?;
    let tool_seq = doc.tool_seq_by_id[&tool];
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        doc.events().to_vec(),
    )?;

    assert_eq!(restored.project_persisted_interruptions()?, 2);
    assert!(restored.active_api().is_none());
    assert_eq!(
        restored.active_turn().map(|turn| turn.turn_id),
        Some(turn_id)
    );
    assert!(matches!(
        restored
            .events()
            .iter()
            .find(|event| event.seq() == tool_seq),
        Some(ConversationEvent {
            event: EventBody::ToolCall(ToolCallEvent {
                status: ToolCallStatus::Interrupted,
                started_at_ms: None,
                finished_at_ms: Some(_),
                output: Some(_),
                ..
            }),
            ..
        })
    ));
    let interrupted_output = restored
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ToolCall(tool_event) if tool_event.tool_call_id == tool => {
                tool_event.output.clone()
            }
            _ => None,
        })
        .expect("interrupted tool must persist an explicit output");
    assert_eq!(
        serde_json::to_value(&interrupted_output).unwrap(),
        serde_json::json!({
            "error": {
                "kind": "interrupted",
                "message": "Tool execution was interrupted before completion."
            },
            "ok": false
        })
    );
    let items = crate::v1::projection::project_provider_items(restored.events(), None, &[])?;
    assert!(items.iter().any(|item| matches!(
        item,
        crate::v1::projection::ProjectedItem::ToolResult { output, .. }
            if output == &interrupted_output
    )));
    Ok(())
}

/// Tool completion commits once, survives reload, and treats an exact replay as a no-op.
/// 工具终态仅提交一次、可跨重载保留，并将精确重放视为无操作。
#[test]
fn tool_completion_is_exactly_idempotent_across_reload() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-idempotent-tool")?,
        "shell",
        ToolPayload::try_new(serde_json::json!({"command": "echo ok"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 3)?;
    let output = ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?;
    let before_finish_revision = doc.memory_revision();
    let committed = doc.finish_tool(
        api,
        tool,
        tool_completion(Some(output.clone()), ToolCallStatus::Completed, T0 + 4),
    )?;
    let receipt = match committed {
        IdempotentMutation::Committed(receipt) => receipt,
        IdempotentMutation::Idempotent(_) => panic!("first completion must commit"),
    };
    assert_eq!(receipt.api_call_id, api);
    assert_eq!(receipt.tool_call_id, tool);
    assert_eq!(receipt.status, ToolCallStatus::Completed);
    assert_eq!(receipt.finished_at_ms, T0 + 4);
    assert_eq!(doc.memory_revision(), before_finish_revision + 1);

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        Vec::new(),
        doc.events().to_vec(),
    )?;
    let revision_before_replay = restored.memory_revision();
    let replayed = restored.finish_tool(
        api,
        tool,
        tool_completion(Some(output), ToolCallStatus::Completed, T0 + 99),
    )?;
    assert_eq!(replayed, IdempotentMutation::Idempotent(receipt.clone()));
    assert_eq!(restored.memory_revision(), revision_before_replay);

    let before_drift = restored.clone();
    assert!(matches!(
        restored.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"exit_code": 1}))?),
                ToolCallStatus::Completed,
                T0 + 100,
            ),
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "output",
        })
    ));
    assert_eq!(restored, before_drift);

    let before_changeset_drift = restored.clone();
    let drift_changeset: PersistentChangeSet = serde_json::from_value(serde_json::json!({
        "mode": "applied",
        "summary": null,
        "files": [],
        "diagnostics": []
    }))
    .expect("empty canonical changeset fixture");
    assert!(matches!(
        restored.finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
                changeset: Some(drift_changeset),
                status: ToolCallStatus::Completed,
                finished_at_ms: T0 + 101,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "changeset",
        })
    ));
    assert_eq!(restored, before_changeset_drift);

    let before_ref_drift = restored.clone();
    assert!(matches!(
        restored.finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
                changeset: None,
                status: ToolCallStatus::Completed,
                finished_at_ms: T0 + 102,
                external_refs: vec![ExternalResourceRef {
                    kind: ExternalResourceKind::Terminal,
                    id: "terminal-1".to_string(),
                }],
                generated_resources: Vec::new(),
            },
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "external_refs",
        })
    ));
    assert_eq!(restored, before_ref_drift);
    Ok(())
}

/// WaitingUser rejects direct completion, while a durable answer or cancel can be confirmed once.
/// WaitingUser 拒绝直接终结，而已持久化回答或取消可被精确确认一次。
#[test]
fn waiting_tool_requires_durable_answer_or_cancel_before_finish_confirmation()
-> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("ask", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let ask = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-waiting-answer")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, ask, T0 + 3)?;
    doc.mark_tool_waiting_user(api, ask, T0 + 4)?;
    let answer = ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?;
    let before_direct_finish = doc.clone();
    assert!(matches!(
        doc.finish_tool(
            api,
            ask,
            tool_completion(Some(answer.clone()), ToolCallStatus::Completed, T0 + 5,),
        ),
        Err(DomainError::ToolStatusMismatch {
            expected: "running_or_terminal",
            actual: "waiting_user",
            ..
        })
    ));
    assert_eq!(doc, before_direct_finish);

    let committed_answer = doc.answer_waiting_tool(
        api,
        ask,
        tool_completion(Some(answer.clone()), ToolCallStatus::Completed, T0 + 6),
    )?;
    assert!(matches!(committed_answer, IdempotentMutation::Committed(_)));
    let answer_revision = doc.memory_revision();
    let confirmed = doc.answer_waiting_tool(
        api,
        ask,
        tool_completion(Some(answer), ToolCallStatus::Completed, T0 + 60),
    )?;
    assert!(matches!(confirmed, IdempotentMutation::Idempotent(_)));
    assert_eq!(confirmed.into_value().finished_at_ms, T0 + 6);
    assert_eq!(doc.memory_revision(), answer_revision);

    let ask_cancel = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-waiting-cancel")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "cancel?"}))?,
        None,
        T0 + 7,
    )?;
    doc.start_prepared_tool(api, ask_cancel, T0 + 8)?;
    doc.mark_tool_waiting_user(api, ask_cancel, T0 + 9)?;
    let committed_cancel = doc.cancel_waiting_tool(api, ask_cancel, None, T0 + 10)?;
    assert!(matches!(committed_cancel, IdempotentMutation::Committed(_)));
    let cancel_revision = doc.memory_revision();
    let confirmed_cancel = doc.cancel_waiting_tool(api, ask_cancel, None, T0 + 100)?;
    assert!(matches!(
        confirmed_cancel,
        IdempotentMutation::Idempotent(_)
    ));
    assert_eq!(confirmed_cancel.into_value().finished_at_ms, T0 + 10);
    assert_eq!(doc.memory_revision(), cancel_revision);
    Ok(())
}

/// A same-seq revision validation failure leaves the entire document unchanged.
/// 同 seq 修订号校验失败时整个文档保持不变。
#[test]
fn tool_completion_revision_failure_is_atomic() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-revision-overflow")?,
        "shell",
        ToolPayload::try_new(serde_json::json!({"command": "echo ok"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 3)?;
    let tool_seq = doc.tool_seq_by_id[&tool];
    let index =
        usize::try_from(tool_seq.value() - doc.resident_start_seq).expect("resident fixture index");
    doc.events[index].event_revision = JS_SAFE_INTEGER_MAX;
    let before = doc.clone();
    assert!(matches!(
        doc.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
                ToolCallStatus::Completed,
                T0 + 4,
            ),
        ),
        Err(DomainError::InvalidId {
            entity: "event_revision",
            ..
        })
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// Tool completion rejects an in-memory malformed change-set before any terminal mutation.
/// 工具完成会在任何终态修改前拒绝内存中构造的非法 change-set。
#[test]
fn tool_completion_validates_changeset_before_mutation() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    let tool = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-invalid-changeset")?,
        "file_edit",
        ToolPayload::try_new(serde_json::json!({"path": "a.txt"}))?,
        None,
        T0 + 2,
    )?;
    doc.start_prepared_tool(api, tool, T0 + 3)?;
    let invalid = PersistentChangeSet {
        mode: ChangeSetMode::Applied,
        summary: None,
        files: vec![PersistentChangeSetFile {
            resource_id: None,
            change: ChangeSetFileChangeKind::Create,
            path: Some("a.txt".to_string()),
            old_path: None,
            new_path: None,
            hunks: Vec::new(),
            lifecycle_content: None,
            before_text_format: None,
            after_text_format: None,
        }],
        diagnostics: Vec::new(),
    };
    let before = doc.clone();
    assert!(matches!(
        doc.finish_tool(
            api,
            tool,
            ToolCompletion {
                output: Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
                changeset: Some(invalid),
                status: ToolCallStatus::Completed,
                finished_at_ms: T0 + 4,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        ),
        Err(DomainError::InvalidPersistentChangeSet { .. })
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// WaitingUser Ask survives a raw reload while its API remains open; answers
/// and cancellation stay on the original tool seq, and Continue never
/// manufactures another user input.
/// WaitingUser Ask 在 API 仍开放的原始重载中保持等待；回答与取消都留在原工具
/// seq，Continue 绝不制造另一条用户输入。
#[test]
fn waiting_ask_answer_cancel_and_continue_keep_one_turn() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("ask", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let ask = doc.prepare_tool(
        api,
        RequestStreamId::try_new("stream-ask-1")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "continue?"}))?,
        None,
        T0 + 1,
    )?;
    doc.start_prepared_tool(api, ask, T0 + 1)?;
    doc.mark_tool_waiting_user(api, ask, T0 + 2)?;
    let ask_seq = doc.tool_seq_by_id[&ask];
    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        doc.events().to_vec(),
    )?;
    restored.answer_waiting_tool(
        api,
        ask,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?),
            ToolCallStatus::Completed,
            T0 + 3,
        ),
    )?;
    complete_user_interrupted(&mut restored, api, T0 + 4)?;
    restored.continue_turn_fixture(T0 + 4)?;
    assert!(matches!(
        restored.events().iter().find(|event| event.seq() == ask_seq),
        Some(ConversationEvent {
            turn_id: Some(value),
            event: EventBody::ToolCall(ToolCallEvent {
                status: ToolCallStatus::Completed,
                ..
            }),
            ..
        }) if *value == turn_id
    ));

    let api2 = restored.start_api(
        ApiPurposeKind::Continue,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 5,
    )?;
    let ask2 = restored.prepare_tool(
        api2,
        RequestStreamId::try_new("stream-ask-2")?,
        "ask",
        ToolPayload::try_new(serde_json::json!({"question": "cancel?"}))?,
        None,
        T0 + 6,
    )?;
    restored.start_prepared_tool(api2, ask2, T0 + 6)?;
    restored.mark_tool_waiting_user(api2, ask2, T0 + 6)?;
    let ask2_seq = restored.tool_seq_by_id[&ask2];
    restored.cancel_waiting_tool(api2, ask2, None, T0 + 8)?;
    complete_ok(&mut restored, api2, None, T0 + 9)?;
    assert!(matches!(
        restored.events().iter().find(|event| event.seq() == ask2_seq),
        Some(ConversationEvent {
            event: EventBody::ToolCall(ToolCallEvent {
                status: ToolCallStatus::Cancelled,
                finished_at_ms: Some(value),
                ..
            }),
            ..
        }) if *value == T0 + 8
    ));
    assert_eq!(
        restored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::UserInput(_)))
            .count(),
        1
    );
    assert_eq!(
        restored.active_turn().map(|turn| turn.turn_id),
        Some(turn_id)
    );
    Ok(())
}

/// Tool batch receipts preserve local execution identity across retry and reload.
/// 工具批次 receipt 在重试与重载后保持本地执行身份。
#[test]
fn prepared_tool_batch_receipt_is_exact_and_restart_idempotent() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("tools", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, replay_request(), T0 + 1)?;
    let preparations = vec![
        prepared_tool("ask", "provider-tool-1"),
        prepared_tool("shell", "provider-tool-2"),
    ];
    let source = replay_source("message-1");
    let first = doc.prepare_tool_batch_with_provider_source(
        api,
        preparations.clone(),
        source.clone(),
        T0 + 2,
    )?;
    assert!(first.changed());
    let receipt = first.into_value();
    assert_eq!(receipt.api_call_id, api);
    assert_eq!(receipt.tools.len(), preparations.len());
    for (mapped, preparation) in receipt.tools.iter().zip(&preparations) {
        assert_eq!(mapped.api_call_id, api);
        assert_eq!(mapped.request_stream_id, preparation.request_stream_id);
        assert!(matches!(
            doc.events().iter().find(|event| event.seq == mapped.tool_seq),
            Some(ConversationEvent {
                event: EventBody::ToolCall(tool),
                ..
            }) if tool.tool_call_id == mapped.tool_call_id
                && tool.request_stream_id == mapped.request_stream_id
        ));
    }

    let revision_before_retry = doc.memory_revision();
    let repeated = doc.prepare_tool_batch_with_provider_source(
        api,
        preparations.clone(),
        source.clone(),
        T0 + 3,
    )?;
    assert!(!repeated.changed());
    assert_eq!(repeated.into_value(), receipt);
    assert_eq!(doc.memory_revision(), revision_before_retry);

    let mut restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        doc.events().to_vec(),
    )?;
    let restored_revision = restored.memory_revision();
    let repeated_after_reload = restored.prepare_tool_batch_with_provider_source(
        api,
        preparations.clone(),
        source,
        T0 + 4,
    )?;
    assert!(!repeated_after_reload.changed());
    assert_eq!(repeated_after_reload.into_value(), receipt);
    assert_eq!(restored.memory_revision(), restored_revision);

    let mut drifted = preparations;
    drifted[0].input = ToolPayload::try_new(serde_json::json!({"changed": true}))?;
    let before_drift = restored.clone();
    assert!(matches!(
        restored.prepare_tool_batch(api, drifted, T0 + 5),
        Err(DomainError::InvalidToolIdentity {
            field: "prepared_batch_semantic_drift"
        })
    ));
    assert_eq!(restored, before_drift);
    Ok(())
}
