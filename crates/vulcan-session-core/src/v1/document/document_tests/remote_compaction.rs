//! Durable opaque checkpoint ownership, reload, and rejection regressions.
//! 可靠不透明检查点归属、重载与拒绝回归。

use super::*;
use crate::v1::context::{CompactionContent, RemoteResponsesCheckpoint};

/// Returns a canonical route-bound provider output with no fabricated assistant text.
/// 返回规范且绑定路由的供应商输出，不伪造助手文本。
fn checkpoint() -> RemoteResponsesCheckpoint {
    RemoteResponsesCheckpoint {
        provider_id: "openai".into(),
        connection_id: "connection-1".into(),
        route_fingerprint: "https://provider.invalid/responses".into(),
        protocol_profile_id: "openai-codex-responses.v1".into(),
        model_id: "gpt-5".into(),
        output_items: vec![
            serde_json::json!({"type":"compaction", "encrypted_content":"opaque-test-bytes"}),
        ],
        response_id: Some("compact-1".into()),
    }
}

/// Returns a document, actual completed API, and its terminal sequence for the supplied `purpose`.
/// 针对给定`purpose`返回文档、实际完成的 API 及其终态序号。
fn completed_api(purpose: ApiPurposeKind) -> (SessionDocument, ApiCallId, Seq) {
    // The request identity is captured before the API starts.
    // 请求身份在 API 开始前捕获。
    let mut request = ApiRequestSnapshot::new("openai", "gpt-5");
    request.profile = Some("openai-codex-responses.v1".into());
    request.connection_id = Some("connection-1".into());
    // A real API group has start and terminal facts, without an invented semantic response.
    // 真实 API 组包含开始及终态事实，不包含虚构语义响应。
    let mut document = new_document();
    document
        .submit_root_input_fixture("compact", Vec::new(), T0)
        .unwrap();
    let api = document
        .start_api(purpose, request.clone(), T0 + 1)
        .unwrap();
    let terminal = complete_ok_for_request(&mut document, api, request, None, T0 + 2).unwrap();
    (document, api, terminal)
}

/// Returns the remote commit for exact `api`, `terminal`, and provider `output`.
/// 返回精确`api`、`terminal`及供应商`output`对应的远端提交。
fn commit(
    api: ApiCallId,
    terminal: Seq,
    output: RemoteResponsesCheckpoint,
) -> CompactionCommitInput {
    CompactionCommitInput::remote_responses(
        api,
        terminal,
        Seq::try_new(1).unwrap(),
        output,
        LockedBaselineProjection::empty(),
    )
}

/// A remote checkpoint commits idempotently and survives strict serialized reload without assistant events.
/// 远端检查点幂等提交，并在没有助手事件的情况下通过严格序列化重载。
#[test]
fn remote_compaction_round_trip_preserves_opaque_output_and_exact_terminal() {
    let (mut document, api, terminal) = completed_api(ApiPurposeKind::Compaction);
    let input = commit(api, terminal, checkpoint());
    let id = document
        .record_compaction(input.clone(), T0 + 3)
        .unwrap()
        .into_value();
    let committed = document.clone();
    assert_eq!(
        document.record_compaction(input, T0 + 4).unwrap(),
        IdempotentMutation::Idempotent(id)
    );
    assert_eq!(document, committed);
    assert!(!document.events.iter().any(|event| matches!(
        &event.event,
        EventBody::AssistantText(_) | EventBody::AssistantReasoning(_)
    )));
    // Flush projection owns the metadata revision used by the real strict loader.
    // 刷新投影持有真实严格加载器使用的元数据修订号。
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    document.advance_durable(candidate.revision).unwrap();
    let nodes =
        serde_json::from_slice(&serde_json::to_vec(&document.compactions).unwrap()).unwrap();
    let restored = SessionDocument::from_parts(
        candidate.metadata,
        document.runtime_lock.clone(),
        nodes,
        Vec::new(),
        document.events.clone(),
    )
    .unwrap();
    assert_eq!(restored.compactions, document.compactions);
    assert_eq!(restored.runtime_lock.current_compaction_id, Some(id));
    assert_eq!(restored.compactions[0].occurred_at_seq, terminal);
    assert!(restored.active_api().is_none());
    // Exact replay identity and opaque bytes are immutable after publication.
    // 发布后精确重放身份与不透明字节不可修改。
    let mut changed = checkpoint();
    changed.output_items[0]["encrypted_content"] = serde_json::json!("changed");
    assert!(
        document
            .record_compaction(commit(api, terminal, changed), T0 + 5)
            .is_err()
    );
}

/// Wrong route fields, malformed items, noncanonical aliases, and wrong API purpose are rejected atomically.
/// 错误路由字段、畸形项、非规范别名及错误 API 用途均被原子拒绝。
#[test]
fn remote_compaction_rejects_invalid_identity_and_output_before_mutation() {
    let (document, api, terminal) = completed_api(ApiPurposeKind::Compaction);
    for index in 0..9 {
        let mut invalid = checkpoint();
        match index {
            0 => invalid.provider_id = "another".into(),
            1 => invalid.connection_id = "another".into(),
            2 => invalid.model_id = "another".into(),
            3 => invalid.protocol_profile_id = "openai-responses.v1".into(),
            4 => invalid.output_items.clear(),
            5 => invalid.output_items[0]["encrypted_content"] = serde_json::json!(""),
            6 => invalid.output_items[0]["type"] = serde_json::json!("compaction_summary"),
            7 => invalid.route_fingerprint.clear(),
            8 => invalid.response_id = Some(" ".into()),
            _ => unreachable!(),
        }
        let mut attempted = document.clone();
        assert!(
            attempted
                .record_compaction(commit(api, terminal, invalid), T0 + 3)
                .is_err(),
            "case {index}"
        );
        assert_eq!(attempted, document);
    }
    let (mut ordinary, api, terminal) = completed_api(ApiPurposeKind::MainGeneration);
    let before = ordinary.clone();
    assert!(
        ordinary
            .record_compaction(commit(api, terminal, checkpoint()), T0 + 3)
            .is_err()
    );
    assert_eq!(ordinary, before);
}

/// A completed local summary cannot be relabeled as a remote response, and remote terminal boundaries are exact.
/// 完成本地摘要不能改称远端响应，且远端终态边界必须精确。
#[test]
fn remote_compaction_rejects_fabricated_semantics_and_terminal_drift() {
    let (mut document, api, terminal) = completed_api(ApiPurposeKind::Compaction);
    let mut wrong_terminal = commit(api, terminal, checkpoint());
    wrong_terminal.occurred_at_seq = Seq::try_new(terminal.value() - 1).unwrap();
    let before = document.clone();
    assert!(document.record_compaction(wrong_terminal, T0 + 3).is_err());
    assert_eq!(document, before);
    // Create a second real API whose local semantic must exclude an opaque-only receipt.
    // 创建第二个真实 API，其本地语义必须排除纯不透明回执。
    let mut request = ApiRequestSnapshot::new("openai", "gpt-5");
    request.profile = Some("openai-codex-responses.v1".into());
    request.connection_id = Some("connection-1".into());
    let local_api = document
        .start_api(ApiPurposeKind::Compaction, request.clone(), T0 + 4)
        .unwrap();
    document
        .append_assistant_text(
            local_api,
            "diagnostic only",
            AssistantSemanticStatus::Completed,
            T0 + 5,
        )
        .unwrap();
    let terminal =
        complete_ok_for_request(&mut document, local_api, request, None, T0 + 6).unwrap();
    let before = document.clone();
    assert!(
        document
            .record_compaction(commit(local_api, terminal, checkpoint()), T0 + 7)
            .is_err()
    );
    assert_eq!(document, before);
}

/// Sealed-node payload validation still rejects corrupted opaque bytes before checking resident source events.
/// 在检查驻留来源事件之前，封存节点载荷校验仍会拒绝损坏的不透明字节。
#[test]
fn remote_compaction_validates_sealed_payload_and_strict_schema() {
    let (mut document, api, terminal) = completed_api(ApiPurposeKind::Compaction);
    document
        .record_compaction(commit(api, terminal, checkpoint()), T0 + 3)
        .unwrap();
    let mut serialized = serde_json::to_value(&document.compactions[0]).unwrap();
    serialized["content"]["checkpoint"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<crate::v1::context::CompactionNode>(serialized).is_err());
    // Legacy summary-only nodes are deliberately not accepted by the new unreleased contract.
    // 新未发布契约有意不接纳旧的仅摘要节点。
    let mut legacy = serde_json::to_value(&document.compactions[0]).unwrap();
    legacy.as_object_mut().unwrap().remove("content");
    legacy["summary"] = serde_json::json!("diagnostic");
    assert!(serde_json::from_value::<crate::v1::context::CompactionNode>(legacy).is_err());
    document.resident_start_seq = terminal.value() + 1;
    let CompactionContent::RemoteResponses { checkpoint } = &mut document.compactions[0].content
    else {
        panic!("remote checkpoint");
    };
    checkpoint.output_items[0]["encrypted_content"] = serde_json::json!("");
    assert!(document.validate_compaction_state().is_err());
}

/// An older remote result cannot append behind a newer durably published local node.
/// 较旧远端结果不能追加在较新且已可靠发布的本地节点之后。
#[test]
fn remote_compaction_late_result_cannot_reverse_published_generation_order() {
    let (mut document, old_api, old_terminal) = completed_api(ApiPurposeKind::Compaction);
    // Both compactions use the same request configuration already frozen in this turn.
    // 两次压缩使用此回合已经冻结的同一请求配置。
    let request = document
        .events
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ApiStarted(started) if started.api_call_id == old_api => {
                Some(started.request.clone())
            }
            _ => None,
        })
        .unwrap();
    // The later local API publishes before the older remote response is committed.
    // 较晚本地 API 在较旧远端响应提交之前发布。
    let local_api = document
        .start_api(ApiPurposeKind::Compaction, request.clone(), T0 + 3)
        .unwrap();
    let local_text = document
        .append_assistant_text(
            local_api,
            "newer summary",
            AssistantSemanticStatus::Completed,
            T0 + 4,
        )
        .unwrap()
        .into_value();
    complete_ok_for_request(&mut document, local_api, request, None, T0 + 5).unwrap();
    document
        .record_compaction(
            CompactionCommitInput::new(
                local_api,
                local_text,
                Seq::try_new(1).unwrap(),
                "newer summary",
                LockedBaselineProjection::empty(),
            ),
            T0 + 6,
        )
        .unwrap();
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    document.advance_durable(candidate.revision).unwrap();
    let before = document.clone();
    assert!(matches!(
        document.record_compaction(commit(old_api, old_terminal, checkpoint()), T0 + 7),
        Err(DomainError::ImmutableFieldMutation {
            entity: "compactions",
            field: "node_order"
        })
    ));
    assert_eq!(document, before);
}

/// A newer remote response cannot move the retained frontier backwards after durable publication.
/// 较新远端响应不能在可靠发布后将保留前沿倒退。
#[test]
fn remote_compaction_cannot_restore_discarded_prefix() {
    let (mut document, api, terminal) = completed_api(ApiPurposeKind::Compaction);
    let mut first = commit(api, terminal, checkpoint());
    first.retained_from_seq = Seq::try_new(2).unwrap();
    document.record_compaction(first, T0 + 3).unwrap();
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    document.advance_durable(candidate.revision).unwrap();
    // The next physical call preserves the existing route and completes before attempting publication.
    // 下一物理调用保留现有路由，并在尝试发布前完成。
    let request = document
        .events
        .iter()
        .find_map(|event| match &event.event {
            EventBody::ApiStarted(started) if started.api_call_id == api => {
                Some(started.request.clone())
            }
            _ => None,
        })
        .unwrap();
    let next_api = document
        .start_api(ApiPurposeKind::Compaction, request.clone(), T0 + 4)
        .unwrap();
    let next_terminal =
        complete_ok_for_request(&mut document, next_api, request, None, T0 + 5).unwrap();
    let before = document.clone();
    assert!(matches!(
        document.record_compaction(commit(next_api, next_terminal, checkpoint()), T0 + 6),
        Err(DomainError::ImmutableFieldMutation {
            entity: "compactions",
            field: "node_order"
        })
    ));
    assert_eq!(document, before);
}
