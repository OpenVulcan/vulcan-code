//! Provider replay identity, event placement, and provenance regression tests.
//! Provider 重放身份、事件位置及来源回归测试。

use super::*;

/// A completed provider-sourced group carries exactly one provider_source and
/// never allows a second one.
/// 已完成的 Provider 来源组恰好携带一个 provider_source，绝不允许第二个。
#[test]
fn provider_source_exactly_once() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("go", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let _text_seq = doc
        .append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0)
        .unwrap();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: Some("m1".to_string()),
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 1).unwrap();
    let placed = doc
        .events()
        .iter()
        .filter(|event| matches!(&event.event, EventBody::AssistantText(t) if t.provider_source.is_some()))
        .count();
    assert_eq!(placed, 1);
    // A completed group closes; the next API group starts on fresh facts.
    // 已完成的组合闭；下一个 API 组在全新事实上启动。
    assert!(
        doc.start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0
        )
        .is_ok()
    );
}

/// Provider source on a group without Provider-sourced events is rejected.
/// 无 Provider 来源事件的组被拒绝放置 provider_source。
#[test]
fn provider_source_requires_provider_events() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("fail", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: None,
        response_id: None,
        replay: None,
    };
    let result = doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::ProviderError),
            error: Some(terminal_error(ApiEndReason::ProviderError)),
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: Some(source),
            completed_at_ms: T0,
        },
    );
    assert!(matches!(
        result,
        Err(DomainError::ReferenceViolation { .. })
    ));
}

/// Provider tool events and replay source become durable together before any tool can wait.
/// Provider 工具事件与重放来源在任何工具进入等待前一起可靠形成。
#[test]
fn prepared_tool_source_survives_open_api_reload_and_closes_once() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tools", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, replay_request(), T0 + 1)?;

    let event_count_before_failure = doc.events().len();
    let revision_before_failure = doc.memory_revision();
    let frontier_before_failure = doc.id_frontier();
    let mut mismatched_source = replay_source("message-1");
    mismatched_source.model = String::from("wrong-model");
    let mismatch = doc.prepare_tool_batch_with_provider_source(
        api,
        vec![prepared_tool("ask", "provider-tool-1")],
        mismatched_source,
        T0 + 2,
    );
    assert!(matches!(
        mismatch,
        Err(DomainError::ApiRequestMismatch { .. })
    ));
    assert_eq!(doc.events().len(), event_count_before_failure);
    assert_eq!(doc.memory_revision(), revision_before_failure);
    assert_eq!(doc.id_frontier(), frontier_before_failure);

    let source = replay_source("message-1");
    let tools = doc
        .prepare_tool_batch_with_provider_source(
            api,
            vec![
                prepared_tool("ask", "provider-tool-1"),
                prepared_tool("shell_session", "provider-tool-2"),
            ],
            source.clone(),
            T0 + 3,
        )?
        .into_value();
    assert_eq!(tools.tools.len(), 2);
    let tool_ids = tools
        .tools
        .iter()
        .map(|receipt| receipt.tool_call_id)
        .collect::<Vec<_>>();
    let provider_tools = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.api_call_id == api => Some(tool),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(provider_tools.len(), 2);
    assert!(provider_tools[0].provider_source.is_none());
    assert_eq!(
        provider_tools[1]
            .provider_source
            .as_ref()
            .and_then(|placed| placed.message_id.as_deref()),
        Some("message-1")
    );
    assert!(
        doc.events()
            .iter()
            .all(|event| !matches!(event.event, EventBody::ApiStats(_)))
    );

    let event_count_before_duplicate = doc.events().len();
    assert!(matches!(
        doc.prepare_tool_batch_with_provider_source(
            api,
            vec![prepared_tool("file", "provider-tool-3")],
            source.clone(),
            T0 + 4,
        ),
        Err(DomainError::ProviderSourceAlreadySet { .. })
    ));
    assert_eq!(doc.events().len(), event_count_before_duplicate);

    doc.start_prepared_tool(api, tool_ids[0], T0 + 5)?;
    doc.mark_tool_waiting_user(api, tool_ids[0], T0 + 6)?;

    let persisted_metadata = serde_json::from_str(
        &serde_json::to_string(&doc.metadata).expect("metadata must serialize"),
    )
    .expect("metadata must strictly reload");
    let persisted_events =
        serde_json::from_str(&serde_json::to_string(doc.events()).expect("events must serialize"))
            .expect("events must strictly reload");
    let mut restored = SessionDocument::from_parts(
        persisted_metadata,
        doc.runtime_lock.clone(),
        doc.compactions.clone(),
        doc.segment_catalog().to_vec(),
        persisted_events,
    )?;
    assert_eq!(
        restored
            .active_api()
            .map(|active| active.provider_source_placed),
        Some(true)
    );
    assert!(
        restored
            .events()
            .iter()
            .all(|event| !matches!(event.event, EventBody::ApiStats(_)))
    );

    restored.answer_waiting_tool(
        api,
        tool_ids[0],
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"answer": "yes"}))?),
            ToolCallStatus::Completed,
            T0 + 7,
        ),
    )?;
    restored.start_prepared_tool(api, tool_ids[1], T0 + 8)?;
    restored.finish_tool(
        api,
        tool_ids[1],
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"exit_code": 0}))?),
            ToolCallStatus::Completed,
            T0 + 9,
        ),
    )?;
    let event_count_before_duplicate_finish = restored.events().len();
    assert!(matches!(
        restored.finish_tool(
            api,
            tool_ids[1],
            tool_completion(
                Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
                ToolCallStatus::Completed,
                T0 + 10,
            ),
        ),
        Err(DomainError::ImmutableFieldMutation {
            entity: "tool_call_terminal",
            field: "output",
        })
    ));
    assert_eq!(restored.events().len(), event_count_before_duplicate_finish);

    let stats_seq = restored
        .complete_api_group(
            api,
            ApiCompletion {
                request: replay_request(),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(source.clone()),
                completed_at_ms: T0 + 11,
            },
        )?
        .stats_seq;
    assert!(matches!(
        restored
            .events()
            .iter()
            .find(|event| event.seq == stats_seq)
            .map(|event| &event.event),
        Some(EventBody::ApiStats(_))
    ));
    assert_eq!(
        restored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::ApiStats(_)))
            .count(),
        1
    );
    assert!(matches!(
        restored.complete_api_group(
            api,
            ApiCompletion {
                request: replay_request(),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(source),
                completed_at_ms: T0 + 12,
            },
        ),
        Err(DomainError::CrossApiGroupUpdate { .. })
    ));
    assert_eq!(
        restored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::ApiStats(_)))
            .count(),
        1
    );
    Ok(())
}

/// A changed terminal replay identity cannot close a group whose source is already durable.
/// 变更后的终态重放身份不能闭合来源已经可靠落盘的 API 组。
#[test]
fn durable_tool_source_rejects_terminal_identity_drift() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run tool", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, replay_request(), T0 + 1)?;
    let source = replay_source("message-1");
    let tools = doc
        .prepare_tool_batch_with_provider_source(
            api,
            vec![prepared_tool("file", "provider-tool-1")],
            source,
            T0 + 2,
        )?
        .into_value();
    let tool_id = tools.tools[0].tool_call_id;
    doc.start_prepared_tool(api, tool_id, T0 + 3)?;
    doc.finish_tool(
        api,
        tool_id,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true}))?),
            ToolCallStatus::Completed,
            T0 + 4,
        ),
    )?;
    let mut changed_source = replay_source("message-2");
    changed_source.response_id = Some(String::from("response-2"));
    let event_count = doc.events().len();
    assert!(matches!(
        doc.complete_api_group(
            api,
            ApiCompletion {
                request: replay_request(),
                usage: usage(10, 20),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(changed_source),
                completed_at_ms: T0 + 5,
            },
        ),
        Err(DomainError::ApiRequestMismatch { .. })
    ));
    assert_eq!(doc.events().len(), event_count);
    assert!(doc.active_api().is_some());
    Ok(())
}

/// A tools-only group (no reasoning/text) carries exactly one provider_source
/// placed on its last tool event, per plan 5.5.5.
/// 仅工具组(无推理/正文)按计划 5.5.5 在最后一个工具事件上恰好携带一个
/// provider_source。
#[test]
fn provider_source_on_tools_only_group() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("run", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let second_tool = doc
        .start_tool(
            api,
            "shell",
            ToolPayload::try_new(serde_json::json!({"cmd": "ls"})).unwrap(),
            Some("pt-1".to_string()),
            T0,
        )
        .unwrap();
    let last_tool = doc
        .start_tool(
            api,
            "file_read",
            ToolPayload::try_new(serde_json::json!({"path": "a"})).unwrap(),
            Some("pt-2".to_string()),
            T0,
        )
        .unwrap();
    doc.finish_tool(
        api,
        second_tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    doc.finish_tool(
        api,
        last_tool,
        tool_completion(
            Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
            ToolCallStatus::Completed,
            T0 + 1,
        ),
    )
    .unwrap();
    let last_tool_seq = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.tool_call_id == last_tool => Some(event.seq()),
            _ => None,
        })
        .unwrap();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: Some("m1".to_string()),
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 2).unwrap();
    let placed: Vec<(u64, &ToolCallEvent)> = doc
        .events()
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::ToolCall(tool) if tool.provider_source.is_some() => {
                Some((tool.tool_call_id.value(), tool))
            }
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 1, "exactly one provider_source in the group");
    assert_eq!(
        placed[0].0,
        last_tool.value(),
        "must sit on the last tool event"
    );
    assert_eq!(
        placed[0]
            .1
            .provider_source
            .as_ref()
            .map(|source| source.covered_until_seq),
        Some(last_tool_seq)
    );
}

/// The document derives provider-source coverage and fingerprint internally.
/// 文档在内部推导 provider-source 覆盖位置与指纹。
#[test]
fn document_derives_provider_source_coverage_and_fingerprint() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("go", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let text_seq = doc
        .append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0)
        .unwrap()
        .into_value();
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: None,
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 1).unwrap();
    let placed = doc.events().iter().find_map(|event| match &event.event {
        EventBody::AssistantText(text) => text.provider_source.as_ref(),
        _ => None,
    });
    let placed = placed.expect("provider source must be attached");
    assert_eq!(placed.covered_until_seq, text_seq);
    assert_eq!(
        placed.fingerprint_sha256,
        crate::v1::projection::provider_visible_fingerprint(doc.events(), text_seq).unwrap()
    );
    assert_eq!(placed.fingerprint_sha256.len(), 64);
}

/// Typed replay state round-trips on the final Provider event, while a replay
/// envelope from another route is rejected before any completion mutation.
/// 强类型 replay 状态在最后一条 Provider 事件上往返保持；来自另一执行路由的封套会在
/// 任何完成态修改前被拒绝。
#[test]
fn typed_provider_replay_round_trips_and_rejects_route_drift() -> Result<(), DomainError> {
    let mut request = ApiRequestSnapshot::new("openai", "gpt-5");
    request.profile = Some("responses.v1".to_string());
    request.connection_id = Some("connection-1".to_string());
    let replay = ProviderReplayEnvelope::new(
        "openai",
        "connection-1",
        "https://example.test/v1/responses",
        "responses.v1",
        "gpt-5",
        Vec::new(),
        vec![serde_json::json!({"type": "reasoning", "id": "r1"})],
    );
    let mut doc = new_document();
    doc.submit_root_input_fixture("replay", Vec::new(), T0)?;
    let api = doc.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0)?;
    let text_seq = doc
        .append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    doc.complete_api_group(
        api,
        ApiCompletion {
            request: request.clone(),
            usage: usage(1, 2),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: Some(ProviderSourceInput {
                provider: "openai".to_string(),
                profile: Some("responses.v1".to_string()),
                model: "gpt-5".to_string(),
                connection_id: Some("connection-1".to_string()),
                message_id: Some("message-1".to_string()),
                response_id: Some("response-1".to_string()),
                replay: Some(replay.clone()),
            }),
            completed_at_ms: T0 + 2,
        },
    )?;
    let serialized =
        serde_json::to_string(doc.events()).expect("typed replay event stream must serialize");
    let events: Vec<ConversationEvent> =
        serde_json::from_str(&serialized).expect("typed replay event stream must deserialize");
    let restored = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        events,
    )?;
    let placed = restored
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::AssistantText(text) if event.seq() == text_seq => {
                text.provider_source.as_ref()
            }
            _ => None,
        });
    assert_eq!(
        placed.and_then(|source| source.replay.as_ref()),
        Some(&replay)
    );

    let mut rejected = new_document();
    rejected.submit_root_input_fixture("replay", Vec::new(), T0)?;
    let rejected_api = rejected.start_api(ApiPurposeKind::MainGeneration, request.clone(), T0)?;
    rejected.append_assistant_text(
        rejected_api,
        "done",
        AssistantSemanticStatus::Completed,
        T0 + 1,
    )?;
    let event_count = rejected.events().len();
    let wrong_replay = ProviderReplayEnvelope::new(
        "openai",
        "connection-1",
        "https://example.test/v1/responses",
        "responses.v1",
        "different-model",
        Vec::new(),
        Vec::new(),
    );
    let result = rejected.complete_api_group(
        rejected_api,
        ApiCompletion {
            request,
            usage: usage(1, 2),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: Some(ProviderSourceInput {
                provider: "openai".to_string(),
                profile: Some("responses.v1".to_string()),
                model: "gpt-5".to_string(),
                connection_id: Some("connection-1".to_string()),
                message_id: None,
                response_id: None,
                replay: Some(wrong_replay),
            }),
            completed_at_ms: T0 + 2,
        },
    );
    assert!(matches!(
        result,
        Err(DomainError::ApiRequestMismatch {
            field: "provider_source.replay.model_id",
            ..
        })
    ));
    assert_eq!(rejected.events().len(), event_count);
    assert_eq!(
        rejected.active_api().map(|value| value.api_call_id),
        Some(rejected_api)
    );
    Ok(())
}

/// Rebuild rejects a provider source moved off the final Provider event even
/// when its coverage and fingerprint were recomputed to look locally valid.
/// 即使重新计算覆盖位置与指纹使其局部看似有效，重建仍拒绝被移出最后 Provider 事件的来源。
#[test]
fn rebuild_rejects_provider_source_before_final_provider_event() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("source", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let reasoning_seq = doc
        .append_reasoning(api, "reasoning", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    doc.append_assistant_text(api, "final", AssistantSemanticStatus::Completed, T0 + 2)?;
    complete_ok(
        &mut doc,
        api,
        Some(ProviderSourceInput {
            provider: "openai".to_string(),
            profile: None,
            model: "gpt-5".to_string(),
            connection_id: None,
            message_id: Some("m1".to_string()),
            response_id: None,
            replay: None,
        }),
        T0 + 3,
    )?;
    let mut events = doc.events().to_vec();
    let mut source = events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::AssistantText(text) => text.provider_source.take(),
            _ => None,
        })
        .expect("valid group must place a source");
    source.covered_until_seq = reasoning_seq;
    source.fingerprint_sha256 =
        crate::v1::projection::provider_visible_fingerprint(&events, reasoning_seq)?;
    let reasoning = events
        .iter_mut()
        .find_map(|event| {
            if event.seq() != reasoning_seq {
                return None;
            }
            match &mut event.event {
                EventBody::AssistantReasoning(reasoning) => Some(reasoning),
                _ => None,
            }
        })
        .expect("reasoning event must exist");
    reasoning.provider_source = Some(source);
    let result = SessionDocument::from_parts(
        doc.metadata.clone(),
        initialized_runtime_lock(),
        Vec::new(),
        Vec::new(),
        events,
    );
    assert!(matches!(
        result,
        Err(DomainError::ProviderSourcePlacement {
            api_call_id,
            source_seq,
            api_closed: true,
            ..
        }) if api_call_id == api.value() && source_seq == reasoning_seq.value()
    ));
    Ok(())
}

/// A checkpoint covers events only from its segment start, so any window holding that start verifies it.
/// 检查点只覆盖其分段起点之后的事件，因此任何包含该起点的窗口都能校验它。
#[test]
fn checkpoint_fingerprint_covers_from_its_segment_start() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("go", Vec::new(), T0)?;
    let rollover = doc
        .events()
        .last()
        .map(|event| event.seq.value() + 1)
        .unwrap();
    let segment = |segment_number: u64, start: u64| crate::v1::config::SegmentInfo {
        segment_number,
        file_name: format!("s{segment_number:07}.jsonl"),
        start_seq: Seq::try_new(start).unwrap(),
        end_seq: None,
        event_count: 0,
        byte_len: 0,
        next_event_local_id: 1,
    };
    doc.segment_catalog = vec![segment(1, 1), segment(2, rollover)];
    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.append_assistant_text(api, "t", AssistantSemanticStatus::Completed, T0)?;
    let source = ProviderSourceInput {
        provider: "openai".to_string(),
        profile: None,
        model: "gpt-5".to_string(),
        connection_id: None,
        message_id: Some("m1".to_string()),
        response_id: None,
        replay: None,
    };
    complete_ok(&mut doc, api, Some(source), T0 + 1)?;
    let (coverage, digest) = doc
        .events()
        .iter()
        .find_map(|event| match &event.event {
            EventBody::AssistantText(text) => text
                .provider_source
                .as_ref()
                .map(|source| (source.covered_until_seq, source.fingerprint_sha256.clone())),
            _ => None,
        })
        .unwrap();
    let from = doc
        .events()
        .iter()
        .position(|event| event.seq.value() == rollover)
        .unwrap();
    assert_eq!(
        digest,
        crate::v1::projection::provider_visible_fingerprint(&doc.events()[from..], coverage)?
    );
    assert_ne!(
        digest,
        crate::v1::projection::provider_visible_fingerprint(doc.events(), coverage)?
    );
    let load = |catalog: Vec<crate::v1::config::SegmentInfo>| {
        SessionDocument::from_stored_parts(
            doc.metadata.clone(),
            crate::v1::context::RuntimeLock {
                initialization_state: crate::v1::context::RuntimeInitializationState::Initialized,
                ..crate::v1::context::RuntimeLock::default()
            },
            vec![],
            catalog,
            doc.events().to_vec(),
        )
    };
    // Full history verifies through the later segment start; without that start the digest cannot be reproduced.
    // 完整历史经由较晚的分段起点校验通过；缺少该起点时摘要无法复现。
    assert!(load(doc.segment_catalog.clone())?.1.is_empty());
    assert_eq!(load(Vec::new())?.1, vec![coverage.value()]);
    Ok(())
}
