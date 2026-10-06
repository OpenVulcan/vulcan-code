//! Document-wide sequence, identity, metadata, and failure-atomicity regression tests.
//! 文档级序号、身份、元数据及失败原子性回归测试。

use super::*;

/// 20,000 mixed events keep a fully contiguous seq stream.
/// 20,000 条混合事件保持完全连续的 seq 流。
#[test]
fn ten_thousand_mixed_events_stay_contiguous() {
    let mut doc = new_document();
    for index in 0..2_500u64 {
        doc.submit_root_input_fixture(format!("prompt {index}"), Vec::new(), T0 + index as i64)
            .unwrap();
        let api = doc
            .start_api(
                ApiPurposeKind::MainGeneration,
                crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                0,
            )
            .unwrap();
        doc.append_reasoning(
            api,
            format!("think {index}"),
            AssistantSemanticStatus::Completed,
            T0 + index as i64,
        )
        .unwrap()
        .into_value();
        doc.append_assistant_text(
            api,
            format!("answer {index}"),
            AssistantSemanticStatus::Completed,
            T0 + index as i64,
        )
        .unwrap()
        .into_value();
        let tool = doc
            .start_tool(
                api,
                "file_edit",
                ToolPayload::try_new(serde_json::json!({"a": 1})).unwrap(),
                Some("pt1".to_string()),
                T0 + index as i64,
            )
            .unwrap();
        doc.finish_tool(
            api,
            tool,
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
                ToolCallStatus::Completed,
                T0 + index as i64,
            ),
        )
        .unwrap()
        .into_value();
        complete_ok(&mut doc, api, None, T0 + index as i64).unwrap();
        doc.close_turn(TurnOutcome::Completed, 5, T0 + index as i64)
            .unwrap();
    }
    assert_eq!(doc.events().len(), 2_500 * 8);
    let mut expected = 1u64;
    for event in doc.events() {
        assert_eq!(event.seq.value(), expected, "seq must stay contiguous");
        expected += 1;
    }
    assert_eq!(doc.id_frontier().next_seq, expected);
}

/// The allocator rejects ids beyond the JavaScript-safe maximum.
/// 分配器拒绝超过 JavaScript 安全上限的 id。
#[test]
fn js_safe_overflow_is_rejected() {
    let mut allocator = IdAllocator::from_frontier(2, JS_SAFE_INTEGER_MAX);
    // The frontier value itself is still allocatable; the next call must fail.
    // frontier 值本身仍然可分配；下一次调用必须失败。
    assert!(allocator.next_seq().is_ok());
    assert!(allocator.next_seq().is_err());
    let mut local = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX, 1);
    assert!(local.next_local_id().is_ok());
    assert!(local.next_local_id().is_err());
}

/// `advance_beyond` rejects overflow instead of panicking.
/// `advance_beyond` 拒绝溢出而不是 panic。
#[test]
fn advance_beyond_rejects_overflow() {
    let mut allocator = IdAllocator::new();
    let result = allocator.advance_beyond(u64::MAX, 1);
    assert!(result.is_err());
    let result = allocator.advance_beyond(1, JS_SAFE_INTEGER_MAX);
    assert!(result.is_err());
    assert!(allocator.advance_beyond(10, 20).is_ok());
    assert_eq!(allocator.frontier(), (21, 11));
}

/// Lifecycle metadata mutations are idempotent, monotonic, and metadata-only.
/// 生命周期 metadata 修改保持幂等、时间单调且只标记 metadata。
#[test]
fn lifecycle_metadata_mutations_are_idempotent_and_monotonic() -> Result<(), DomainError> {
    let mut doc = new_document();
    assert!(doc.rename_session("renamed", T0 + 10)?);
    assert_eq!(doc.metadata.title, "renamed");
    assert_eq!(doc.metadata.updated_at_ms, T0 + 10);
    assert_eq!(
        doc.dirty_files().collect::<BTreeSet<_>>(),
        BTreeSet::from([DirtyFile::Metadata])
    );
    let revision = doc.memory_revision();
    assert!(!doc.rename_session("renamed", T0 + 11)?);
    assert_eq!(doc.memory_revision(), revision);

    assert!(doc.set_archived_at(Some(T0 + 5), T0 + 5)?);
    assert_eq!(doc.metadata.archived_at_ms, Some(T0 + 5));
    assert_eq!(doc.metadata.updated_at_ms, T0 + 10);
    let before_invalid = doc.clone();
    assert!(doc.rename_session("   ", T0 + 12).is_err());
    assert_eq!(doc, before_invalid);
    assert!(doc.set_archived_at(Some(T0 - 1), T0 + 12).is_err());
    assert_eq!(doc, before_invalid);
    Ok(())
}

/// Allocation and seq failures leave every public input/API transition byte-for-byte unchanged.
/// 分配与 seq 失败会让每个公开输入/API 转换保持完整文档逐字段不变。
#[test]
fn input_and_api_allocation_failures_are_document_atomic() -> Result<(), DomainError> {
    let mut root = new_document();
    root.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX, 1);
    let before_root = root.clone();
    assert!(root.submit_root_input_fixture("x", Vec::new(), T0).is_err());
    assert_eq!(root, before_root);

    let mut enqueue = new_document();
    let (_, enqueue_seq) = enqueue.id_allocator.frontier();
    enqueue.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, enqueue_seq);
    let before_enqueue = enqueue.clone();
    assert!(
        enqueue
            .enqueue_input_fixture("queued", Vec::new(), T0)
            .is_err()
    );
    assert_eq!(enqueue, before_enqueue);

    let mut pending = new_document();
    pending.submit_root_input_fixture("root", Vec::new(), T0)?;
    let (_, pending_seq) = pending.id_allocator.frontier();
    pending.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, pending_seq);
    let before_pending = pending.clone();
    assert!(
        pending
            .add_pending_in_turn_input_fixture("more", Vec::new(), T0 + 1)
            .is_err()
    );
    assert_eq!(pending, before_pending);

    let mut api = new_document();
    api.submit_root_input_fixture("root", Vec::new(), T0)?;
    let (_, api_seq) = api.id_allocator.frontier();
    api.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, api_seq);
    let before_api = api.clone();
    assert!(
        api.start_api(
            ApiPurposeKind::MainGeneration,
            ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )
        .is_err()
    );
    assert_eq!(api, before_api);

    let mut continuation = new_document();
    let (_, continuation_seq) = continuation.id_allocator.frontier();
    continuation.id_allocator =
        IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, continuation_seq);
    let before_continuation = continuation.clone();
    assert!(
        continuation
            .start_goal_continuation_turn("continue", T0)
            .is_err()
    );
    assert_eq!(continuation, before_continuation);
    Ok(())
}

/// The common append primitive validates seq consistency before publishing allocator state.
/// 公共追加母层在发布分配器状态前校验 seq 一致性。
#[test]
fn append_event_sequence_failure_does_not_advance_allocator() -> Result<(), DomainError> {
    let mut doc = new_document();
    let (next_local_id, next_seq) = doc.id_allocator.frontier();
    doc.id_allocator = IdAllocator::from_frontier(next_local_id, next_seq + 1);
    let before = doc.clone();
    assert!(matches!(
        doc.append_event(
            None,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: "broken".to_string(),
                operation: SystemContextOperation::Upsert,
                scope: SystemContextScope::Session,
                ttl_turns: None,
                provider_role_hint: None,
                content: "broken".to_string(),
            }),
            T0,
        ),
        Err(DomainError::SequenceViolation { .. })
    ));
    assert_eq!(doc, before);
    Ok(())
}

/// System-context TTL shape is strict, failure-atomic, and identical on reload.
/// 系统上下文 TTL 结构保持严格、失败原子，并在重载时使用相同契约。
#[test]
fn system_context_ttl_shape_is_symmetric_and_atomic() -> Result<(), DomainError> {
    let mut document = new_document();
    let invalid_cases = [
        (
            " turn",
            SystemContextOperation::Upsert,
            SystemContextScope::Turn,
            Some(1),
            "context_key",
        ),
        (
            "turn",
            SystemContextOperation::Upsert,
            SystemContextScope::Turn,
            None,
            "turn_scope_ttl_turns",
        ),
        (
            "session",
            SystemContextOperation::Upsert,
            SystemContextScope::Session,
            Some(0),
            "ttl_turns",
        ),
        (
            "session",
            SystemContextOperation::Remove,
            SystemContextScope::Session,
            Some(1),
            "remove_ttl_turns",
        ),
    ];
    for (context_key, operation, scope, ttl_turns, field) in invalid_cases {
        let before = document.clone();
        let result = document.append_system_context(
            None,
            context_key,
            operation,
            scope,
            ttl_turns,
            None,
            "semantic body",
            T0,
        );
        assert!(
            matches!(
                &result,
                Err(DomainError::ImmutableFieldMutation {
                    entity: "system_context",
                    field: actual,
                }) if *actual == field
            ),
            "unexpected validation result for {field}: {result:?}"
        );
        assert_eq!(document, before);
    }

    document.append_system_context(
        None,
        "session",
        SystemContextOperation::Upsert,
        SystemContextScope::Session,
        Some(2),
        None,
        "semantic body",
        T0,
    )?;
    let mut invalid_events = document.events().to_vec();
    let context = invalid_events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::SystemContext(context) => Some(context),
            _ => None,
        })
        .expect("system context event");
    context.ttl_turns = Some(0);
    assert!(matches!(
        SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            invalid_events,
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "ttl_turns",
        })
    ));
    Ok(())
}

/// Nested resident event frames restore every replaced, truncated and appended event exactly.
/// 嵌套常驻事件帧精确恢复每个被替换、截断及追加的事件。
#[test]
fn resident_event_frames_restore_exact_entry_streams() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("prompt".to_owned(), Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
        0,
    )?;
    doc.append_reasoning(
        api,
        "think".to_owned(),
        AssistantSemanticStatus::Completed,
        T0,
    )?;
    doc.append_assistant_text(
        api,
        "answer".to_owned(),
        AssistantSemanticStatus::Completed,
        T0,
    )?;
    let source = doc.events().to_vec();
    assert!(source.len() >= 4, "fixture must yield at least four events");

    // An outer rollback undoes committed inner truncation and in-place edits as well.
    // 外层回滚同样撤销已提交内层的截断与原位编辑。
    let mut stream = ResidentEvents::new(source[..2].to_vec());
    let entry = stream.to_vec();
    stream.open_frame();
    stream.replace(0, source[2].clone());
    stream.extend([source[3].clone()]);
    stream.open_frame();
    stream.truncate(1);
    stream.get_mut(0).expect("resident event").updated_at_ms += 1;
    stream.extend([source[1].clone(), source[2].clone()]);
    stream.commit_frame();
    stream.rollback_frame();
    assert_eq!(stream.to_vec(), entry);

    // An inner rollback keeps the enclosing frame's edits, which its commit then keeps.
    // 内层回滚保留外层帧的编辑，外层提交随后保留这些编辑。
    stream.open_frame();
    stream.replace(1, source[3].clone());
    let outer_edit = stream.to_vec();
    stream.open_frame();
    stream.truncate(0);
    stream.extend([source[2].clone()]);
    stream.rollback_frame();
    assert_eq!(stream.to_vec(), outer_edit);
    stream.commit_frame();
    assert_eq!(stream.into_vec(), outer_edit);
    Ok(())
}
