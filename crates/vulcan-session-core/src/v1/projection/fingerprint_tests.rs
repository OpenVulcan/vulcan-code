//! Replay fingerprint contract and interrupted-history regression coverage.
//! 重放指纹契约与中断历史回归覆盖。

use super::*;
use crate::v1::context::{RuntimeInitializationState, RuntimeLock};
use crate::v1::document::{ApiCompletion, SessionDocument};
use crate::v1::ids::ClientActionId;
use crate::v1::stats::UsageSnapshot;

/// Builds interrupted reasoning/text, optional pending tools, guidance, and a completed sourced reply.
/// 构造中断推理、正文、可选挂起工具、插队指令及带来源的完整回复。
/// `with_tool` selects the terminal pairing case; returns a closed document without external I/O.
/// `with_tool` 选择终态配对场景；返回已闭合文档，不执行外部 IO。
fn interrupted_history(with_tool: bool) -> SessionDocument {
    let mut document = SessionDocument::new("fingerprint", "project", "title", 1);
    document
        .submit_root_input_fixture("inspect", vec![], 1)
        .unwrap();
    let request = ApiRequestSnapshot::new("deepseek", "deepseek-flash");
    let interrupted = document
        .start_api(ApiPurposeKind::MainGeneration, request.clone(), 2)
        .unwrap();
    document
        .append_reasoning(
            interrupted,
            "partial reasoning",
            AssistantSemanticStatus::Completed,
            3,
        )
        .unwrap();
    document
        .append_assistant_text(
            interrupted,
            "partial answer",
            AssistantSemanticStatus::Streaming,
            4,
        )
        .unwrap();
    if with_tool {
        document
            .start_tool(
                interrupted,
                "read",
                ToolPayload::try_new(serde_json::json!({"path":"a"})).unwrap(),
                Some("call-1".into()),
                4,
            )
            .unwrap();
    }
    document.interrupt_active_api(5).unwrap();
    document
        .append_in_turn_guidance_before_api(
            ClientActionId::try_new("stop").unwrap(),
            "stop".into(),
            6,
        )
        .unwrap();
    let completed = document
        .start_api(ApiPurposeKind::MainGeneration, request.clone(), 7)
        .unwrap();
    document
        .append_assistant_text(completed, "ack", AssistantSemanticStatus::Completed, 8)
        .unwrap();
    document
        .complete_api_group(
            completed,
            ApiCompletion {
                request,
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(ProviderSourceInput {
                    provider: "deepseek".into(),
                    profile: None,
                    model: "deepseek-flash".into(),
                    connection_id: None,
                    message_id: None,
                    response_id: None,
                    replay: None,
                }),
                completed_at_ms: 9,
            },
        )
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 10).unwrap();
    document
}

/// Finds the sole sourced acknowledgement, returning its mutable checkpoint for compatibility fixtures.
/// 找到唯一带来源的确认回复，返回可变检查点以构造兼容夹具。
fn source_mut(events: &mut [ConversationEvent]) -> &mut ProviderSource {
    events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::AssistantText(text) => text.provider_source.as_mut(),
            _ => None,
        })
        .expect("ack checkpoint")
}

/// Loads exact events through the production domain constructor with an initialized runtime.
/// 使用已初始化运行时，通过生产领域构造器加载精确事件。
fn reload(
    document: &SessionDocument,
    events: Vec<ConversationEvent>,
) -> Result<SessionDocument, DomainError> {
    SessionDocument::from_parts(
        document.metadata.clone(),
        RuntimeLock {
            initialization_state: RuntimeInitializationState::Initialized,
            ..RuntimeLock::default()
        },
        vec![],
        vec![],
        events,
    )
}

/// Pins the pre-filter wire bytes independently of the projection implementation.
/// 独立于投影实现固定过滤改动之前的线协议字节。
#[test]
fn fingerprint_preserves_original_wire_contract() {
    let document = interrupted_history(false);
    let mut events = document.events().to_vec();
    let source = source_mut(&mut events);
    let original_bytes = br#"[{"kind":"user_input","text":"inspect","resource_ids":[]},{"kind":"assistant_text","text":"partial answer","reasoning":"partial reasoning","tool_calls":[]},{"kind":"user_input","text":"stop","resource_ids":[]},{"kind":"assistant_text","text":"ack","reasoning":null,"tool_calls":[]}]"#;
    assert_eq!(
        source.fingerprint_sha256,
        hex_encode(&Sha256::digest(original_bytes))
    );
    assert_eq!(
        source.fingerprint_version,
        ProviderFingerprintVersion::ImmutableProjectionV1
    );
    let wire = serde_json::to_value(&source).unwrap();
    assert_eq!(wire["fingerprint_version"], "immutable_projection_v1");
    let mut unversioned_wire = wire.clone();
    unversioned_wire
        .as_object_mut()
        .unwrap()
        .remove("fingerprint_version");
    assert!(serde_json::from_value::<ProviderSource>(unversioned_wire).is_err());
    let mut future_wire = wire;
    future_wire["fingerprint_version"] = "unknown_future_version".into();
    assert!(serde_json::from_value::<ProviderSource>(future_wire).is_err());
}

/// Host terminal status cannot change a stable digest, but partial Provider facts remain protected.
/// 宿主终态不能改变稳定摘要，但部分 Provider 事实仍受保护。
#[test]
fn fingerprint_ignores_host_status_but_detects_partial_text_changes() {
    let document = interrupted_history(true);
    let mut events = document.events().to_vec();
    let source = source_mut(&mut events).clone();
    for status in [
        ApiCallStatus::Completed,
        ApiCallStatus::Failed,
        ApiCallStatus::Cancelled,
        ApiCallStatus::Interrupted,
    ] {
        let mut changed = events.clone();
        let stats = changed
            .iter_mut()
            .find_map(|event| match &mut event.event {
                EventBody::ApiStats(stats) => Some(stats),
                _ => None,
            })
            .unwrap();
        stats.status = status;
        assert_eq!(
            provider_visible_fingerprint(&changed, source.covered_until_seq).unwrap(),
            source.fingerprint_sha256
        );
    }
    let partial = events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::AssistantText(text) if text.text == "partial answer" => Some(text),
            _ => None,
        })
        .unwrap();
    partial.text.push_str(" changed");
    assert!(matches!(
        validate_provider_fingerprint(&events, &[], &source),
        Err(DomainError::ProviderSourceFingerprintMismatch { .. })
    ));
}

/// Hashes the request-policy projection, which drops partial assistant facts, in the fingerprint byte form.
/// 以指纹字节形态哈希会丢弃部分 assistant 事实的请求策略投影。
fn omitted_partial_digest(events: &[ConversationEvent], coverage: Seq) -> String {
    let covered: Vec<ConversationEvent> = events
        .iter()
        .filter(|event| event.seq() <= coverage)
        .cloned()
        .collect();
    let items: Vec<ProjectedItem> =
        project_items_with_policy(&covered, None, &[], PartialAssistantPolicy::Omit, None)
            .unwrap()
            .into_iter()
            .map(|sourced| match sourced.item {
                ProjectedItem::AssistantText {
                    text,
                    reasoning,
                    tool_calls,
                    ..
                } => ProjectedItem::AssistantText {
                    text,
                    reasoning,
                    tool_calls,
                    provider_source: None,
                },
                item => item,
            })
            .filter(|item| !matches!(item, ProjectedItem::ToolResult { .. }))
            .collect();
    hex_encode(&Sha256::digest(serde_json::to_vec(&items).unwrap()))
}

/// Checkpoints never accept a digest computed with partial assistant facts filtered out.
/// 检查点绝不接受过滤部分 assistant 事实后计算的摘要。
#[test]
fn fingerprint_rejects_filtered_digest() {
    let document = interrupted_history(false);
    let mut events = document.events().to_vec();
    let coverage = source_mut(&mut events).covered_until_seq;
    let filtered = omitted_partial_digest(&events, coverage);
    assert_ne!(filtered, source_mut(&mut events).fingerprint_sha256);
    source_mut(&mut events).fingerprint_sha256 = filtered;
    assert!(
        matches!(reload(&document, events), Err(DomainError::ProviderSourceFingerprintMismatch { seq, .. }) if seq == coverage.value())
    );
}

/// Fingerprint validation rejects modified user or completed assistant content.
/// 指纹校验拒绝被修改的用户输入或完整 assistant 内容。
#[test]
fn fingerprint_rejects_tampered_content() {
    for tamper_user in [false, true] {
        let document = interrupted_history(true);
        let mut events = document.events().to_vec();
        let coverage = source_mut(&mut events).covered_until_seq;
        for event in &mut events {
            match &mut event.event {
                EventBody::UserInput(input) if tamper_user => {
                    input.text.push_str(" changed");
                    break;
                }
                EventBody::AssistantText(text)
                    if !tamper_user && text.provider_source.is_some() =>
                {
                    text.text.push_str(" changed");
                    break;
                }
                _ => {}
            }
        }
        let error = reload(&document, events).unwrap_err();
        assert!(matches!(
            error,
            DomainError::ProviderSourceFingerprintMismatch { .. }
        ));
        assert!(
            error
                .to_string()
                .contains(&format!("seq {}", coverage.value()))
        );
    }
}

/// Builds one resource-only root input followed by a completed sourced reply.
/// 构造一条仅资源根输入及随后带来源的完整回复。
fn resource_only_history() -> SessionDocument {
    let mut document = SessionDocument::new("resource-only", "project", "title", 1);
    document
        .submit_root_input_fixture("", vec![ResourceId::try_new(1).unwrap()], 1)
        .unwrap();
    let request = ApiRequestSnapshot::new("deepseek", "deepseek-flash");
    let api = document
        .start_api(ApiPurposeKind::MainGeneration, request.clone(), 2)
        .unwrap();
    document
        .append_assistant_text(api, "ack", AssistantSemanticStatus::Completed, 3)
        .unwrap();
    document
        .complete_api_group(
            api,
            ApiCompletion {
                request,
                usage: UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: Some(ProviderSourceInput {
                    provider: "deepseek".into(),
                    profile: None,
                    model: "deepseek-flash".into(),
                    connection_id: None,
                    message_id: None,
                    response_id: None,
                    replay: None,
                }),
                completed_at_ms: 4,
            },
        )
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    document
}

/// The frozen format hashes the durable empty text of a resource-only input, while requests still receive the note.
/// 冻结格式哈希仅资源输入的可靠空文字，而请求仍收到说明文字。
#[test]
fn fingerprint_hashes_durable_text_of_resource_only_input() {
    let document = resource_only_history();
    let mut events = document.events().to_vec();
    let durable_bytes = br#"[{"kind":"user_input","text":"","resource_ids":[1]},{"kind":"assistant_text","text":"ack","reasoning":null,"tool_calls":[]}]"#;
    assert_eq!(
        source_mut(&mut events).fingerprint_sha256,
        hex_encode(&Sha256::digest(durable_bytes))
    );
    let projected = project_provider_items(document.events(), None, &[]).unwrap();
    assert!(projected.iter().any(|item| matches!(
        item,
        ProjectedItem::UserInput { text, .. } if text == RESOURCE_ONLY_INPUT_TEXT
    )));
    assert!(reload(&document, events).is_ok());
}

/// Stored loading quarantines a checkpoint whose digest no longer verifies instead of refusing the session.
/// 存储加载会隔离摘要已无法校验的检查点，而不是拒绝整个会话。
#[test]
fn stored_parts_quarantine_unverifiable_checkpoint() {
    let document = interrupted_history(false);
    let mut events = document.events().to_vec();
    let coverage = source_mut(&mut events).covered_until_seq;
    source_mut(&mut events).fingerprint_sha256 = "0".repeat(64);
    assert!(matches!(
        reload(&document, events.clone()),
        Err(DomainError::ProviderSourceFingerprintMismatch { .. })
    ));
    let (loaded, quarantined) = SessionDocument::from_stored_parts(
        document.metadata.clone(),
        RuntimeLock {
            initialization_state: RuntimeInitializationState::Initialized,
            ..RuntimeLock::default()
        },
        vec![],
        vec![],
        events,
    )
    .unwrap();
    assert_eq!(quarantined, vec![coverage.value()]);
    assert_eq!(loaded.events().len(), document.events().len());
    assert!(loaded.events().iter().all(|event| match &event.event {
        EventBody::AssistantReasoning(reasoning) => reasoning.provider_source.is_none(),
        EventBody::AssistantText(text) => text.provider_source.is_none(),
        EventBody::ToolCall(tool) => tool.provider_source.is_none(),
        _ => true,
    }));
}

/// One batch pass reproduces the digest of every single boundary, including boundaries inside open groups.
/// 一次批量遍历复现每个单独边界的摘要，包括位于开放组内部的边界。
#[test]
fn fingerprint_batch_matches_single_boundaries() {
    for with_tool in [false, true] {
        let events = interrupted_history(with_tool).events().to_vec();
        let coverages: Vec<Seq> = events.iter().map(ConversationEvent::seq).collect();
        let single: Vec<String> = coverages
            .iter()
            .map(|coverage| provider_visible_fingerprint(&events, *coverage).unwrap())
            .collect();
        assert_eq!(
            provider_visible_fingerprints(&events, &coverages).unwrap(),
            single
        );
    }
}
