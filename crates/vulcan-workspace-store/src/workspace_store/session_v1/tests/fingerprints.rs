//! Disk-level regression for replay fingerprints across interrupted and resumed APIs.
//! 中断及续跑 API 的重放指纹磁盘级回归。

use super::*;
use vulcan_session_core::v1::context::RuntimeLock;
use vulcan_session_core::v1::projection::{ProjectedItem, project_document};

/// Creates a completed interrupted-then-resumed history using production preparation and source placement.
/// 使用生产准备与来源放置流程构造中断后续跑并完成的历史。
/// The returned document uses synthetic text only and has no external side effects.
/// 返回文档仅使用合成文本，无外部副作用。
fn history(session_id: &str) -> SessionDocument {
    let mut document = SessionDocument::new(session_id, "project-1", "title", 1);
    submit_test_root(&mut document, "root", "inspect", 1);
    let request = ApiRequestSnapshot::new("openai", "gpt-5");
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        request.clone(),
        2,
    );
    document
        .append_reasoning(
            api,
            "partial reasoning",
            AssistantSemanticStatus::Completed,
            3,
        )
        .unwrap();
    document
        .append_assistant_text(api, "partial answer", AssistantSemanticStatus::Streaming, 4)
        .unwrap();
    document.interrupt_active_api(5).unwrap();
    document
        .append_in_turn_guidance_before_api(
            ClientActionId::try_new("stop").unwrap(),
            "stop".into(),
            6,
        )
        .unwrap();
    let api = prepare_api(
        &mut document,
        ApiPurposeKind::MainGeneration,
        request.clone(),
        7,
    );
    document
        .append_assistant_text(api, "ack", AssistantSemanticStatus::Completed, 8)
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
                    provider: "openai".into(),
                    profile: None,
                    model: "gpt-5".into(),
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

/// Versioned source envelopes stay exact across full/tail loading and a durable draft update.
/// 带版本来源封套经完整、尾段加载与可靠草稿更新后保持精确。
#[test]
fn fingerprint_disk_reload_draft_save_and_next_api() {
    let (_root, layout) = fresh_layout("fingerprint-reload");
    let document = history(&layout.session_id);
    let events = document.events().to_vec();
    let original_events = serde_json::to_vec(&events).unwrap();
    let document = SessionDocument::from_parts(
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
    publish_initial(&layout, &document);
    assert_eq!(
        serde_json::to_vec(load_session(&layout).unwrap().document.events()).unwrap(),
        original_events
    );
    let (loaded, _) = load_session_tail_only(&layout).unwrap();
    let mut loaded = loaded.document;
    assert_eq!(loaded.memory_revision(), loaded.durable_revision());
    loaded.set_draft("next question", vec![], 11).unwrap();
    flush_candidate(&mut loaded, &layout, 11);
    let (reloaded, _) = load_session_tail_only(&layout).unwrap();
    let mut reloaded = reloaded.document;
    assert_eq!(reloaded.metadata.input_state.draft.text, "next question");
    assert_eq!(
        serde_json::to_vec(reloaded.events()).unwrap(),
        original_events
    );
    submit_test_root(&mut reloaded, "next", "next question", 12);
    let projection = project_document(&reloaded, None, &[]).unwrap();
    assert!(!projection.iter().any(|item| matches!(
        item,
        ProjectedItem::AssistantText {
            reasoning: Some(_),
            ..
        }
    )));
    prepare_api(
        &mut reloaded,
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        13,
    );
    flush_candidate(&mut reloaded, &layout, 13);
    assert!(
        load_session(&layout)
            .unwrap()
            .document
            .active_api()
            .is_some()
    );
}

/// Fingerprint failures retain exact checkpoints and digests instead of masquerading as immutable writes.
/// 指纹失败保留精确检查点与摘要，不再伪装为不可变字段写入。
#[test]
fn fingerprint_error_has_distinct_storage_code_and_evidence() {
    let error = super::super::documents::map_domain_error(
        vulcan_session_core::v1::errors::DomainError::ProviderSourceFingerprintMismatch {
            seq: 31,
            stored: "a".repeat(64),
            expected: "b".repeat(64),
        },
        "session",
        "metadata.json",
    );
    assert_eq!(error.code.as_str(), "provider_source_fingerprint_mismatch");
    assert_eq!(
        error.context.as_deref(),
        Some("provider_source checkpoint seq 31")
    );
    assert_eq!(error.actual, Some("a".repeat(64)));
    assert_eq!(error.expected, Some("b".repeat(64)));
    assert!(error.to_string().contains("history was not rewritten"));
}
