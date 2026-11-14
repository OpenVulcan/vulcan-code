//! Append-only model identity notices at actual request preparation boundaries.
//! 实际请求准备边界上只追加的模型身份通知。

use super::super::context_injection::{
    MODEL_SELECTION_CONTEXT_KEY, synchronize_request_dynamic_context,
};
use super::host_guidance::{candidate_for, install_baseline};
use super::*;

/// Writes one completed request with a chosen route and optional native replay evidence.
/// 写入一条由所选路由完成的请求，并可附带原生回放证据。
/// The fixture keeps reasoning and text in the real ledger and returns the recorded request.
/// 夹具将推理和正文保留在真实账本中，并返回所记录的请求。
fn record_replay_round_trip_request(
    handle: &SessionHandle,
    route: &V1ReplayRouteIdentity,
    native: Option<ProviderReplayEnvelope>,
    reasoning: &str,
    answer: &str,
    time: i64,
) -> ApiRequestSnapshot {
    // request binds the actual producing connection and protocol to its API start.
    // request 将实际产出连接和协议绑定到 API 起始记录。
    let request = ApiRequestSnapshot {
        profile: Some(route.protocol_profile_id.clone()),
        connection_id: Some(route.connection_id.clone()),
        ..ApiRequestSnapshot::new(&route.provider_id, &route.model_id)
    };
    handle
        .mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.apply_atomic_transition(|document| {
                    document.set_pending_request(
                        vulcan_session_core::v1::config::PendingRequestConfig {
                            provider: Some(request.provider.clone()),
                            model: Some(request.model.clone()),
                            connection_id: request.connection_id.clone(),
                            thinking_level: None,
                            temperature: None,
                            set_at_ms: time,
                        },
                        time,
                    )?;
                    document.promote_pending_request(None, time)?;
                    Ok(())
                })
            },
        )
        .unwrap();
    // projection uses the same immutable prefix fingerprint as a real request.
    // projection 使用与真实请求相同的不可变前缀指纹。
    let projection = handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .unwrap(),
            }
        })
        .unwrap();
    let api = handle
        .prepare_and_start_api_durable(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::MainGeneration,
            request: request.clone(),
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection,
            locked_baseline: initial_locked_baseline_with_host_guidance(
                &LockedBaselineProjection::empty(),
            )
            .unwrap(),
            continuation_action_id: None,
            now_ms: time + 1,
        })
        .unwrap()
        .value
        .api_call_id;
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.apply_atomic_transition(|document| {
                    document.append_reasoning(
                        api,
                        reasoning,
                        AssistantSemanticStatus::Completed,
                        time + 2,
                    )?;
                    document.append_assistant_text(
                        api,
                        answer,
                        AssistantSemanticStatus::Completed,
                        time + 3,
                    )?;
                    document.complete_api_group(
                        api,
                        ApiCompletion {
                            request: request.clone(),
                            usage: Default::default(),
                            status: ApiCallStatus::Completed,
                            end_reason: Some(ApiEndReason::CompletedSuccessfully),
                            error: None,
                            continuation_eligibility: ContinuationEligibility::Forbidden,
                            timing: ApiTimings::default(),
                            provider_source: native.map(|replay| {
                                vulcan_session_core::v1::event::ProviderSourceInput {
                                    provider: request.provider.clone(),
                                    model: request.model.clone(),
                                    profile: request.profile.clone(),
                                    connection_id: request.connection_id.clone(),
                                    message_id: Some("deepseek-response".into()),
                                    response_id: None,
                                    replay: Some(replay),
                                }
                            }),
                            completed_at_ms: time + 4,
                        },
                    )
                })
            },
        )
        .unwrap();
    request
}

/// Verifies DeepSeek → custom OpenAI → DeepSeek against persisted API origins and durable history.
/// 依据已持久化 API 来源与可靠历史验证 DeepSeek → 自定义 OpenAI → DeepSeek。
#[tokio::test]
async fn deepseek_custom_deepseek_round_trip_preserves_durable_reasoning() {
    let (_directory, handle) = published_stored_open_turn();
    let deepseek = V1ReplayRouteIdentity {
        provider_id: "deepseek".into(),
        model_id: "deepseek-flash".into(),
        protocol_profile_id: "openai-responses.v1".into(),
        ..deepseek_replay_route()
    };
    let custom = V1ReplayRouteIdentity {
        provider_id: "custom:relay".into(),
        connection_id: "relay-account".into(),
        route_fingerprint: "https://relay.invalid/v1/responses".into(),
        model_id: "relay-model".into(),
        protocol_profile_id: deepseek.protocol_profile_id.clone(),
    };
    // Keep real native items on the DeepSeek turn so the return can prove byte-for-byte replay.
    // DeepSeek 回合保留真实原生项，切回时可验证回放内容完全一致。
    let envelope = ProviderReplayEnvelope::new(
        &deepseek.provider_id,
        &deepseek.connection_id,
        &deepseek.route_fingerprint,
        &deepseek.protocol_profile_id,
        &deepseek.model_id,
        durable_canonical_content(),
        durable_reasoning_envelope().provider_context_items,
    );
    record_replay_round_trip_request(
        &handle,
        &deepseek,
        Some(envelope.clone()),
        "durable-reasoning",
        "durable-answer",
        10,
    );
    let to_custom = rebuild_round_trip_request(
        &handle,
        handle.capture_controller_projection_snapshot().unwrap(),
        &custom,
    )
    .await
    .unwrap();
    let to_custom = to_custom
        .iter()
        .find(|message| message.role == MessageRole::Assistant)
        .unwrap();
    assert_eq!(
        to_custom.content,
        vec![ContentItem::Text {
            text: "durable-answer".into()
        }]
    );
    assert!(to_custom.provider_context_items.is_empty());

    let custom_request = record_replay_round_trip_request(
        &handle,
        &custom,
        None,
        "relay-private",
        "relay-answer",
        20,
    );
    let snapshot = handle.capture_controller_projection_snapshot().unwrap();
    let before = handle.read(|document| document.events().to_vec()).unwrap();
    // Use the sourced items and API origins captured by the real durable projection.
    // 使用真实可靠投影捕获的来源条目及 API 来源。
    let (index, source) = snapshot
        .items
        .iter()
        .enumerate()
        .find_map(|(index, item)| match item {
            ProjectedItem::AssistantText { text, .. } if text == "relay-answer" => {
                Some((index, item))
            }
            _ => None,
        })
        .unwrap();
    assert!(
        matches!(source, ProjectedItem::AssistantText { reasoning: Some(text), provider_source: None, .. } if text == "relay-private")
    );
    let origin = projected_assistant_request(&snapshot, index);
    assert_eq!(origin, Some(&custom_request));
    let relay_content = vec![
        ContentItem::Reasoning {
            text: "relay-private".into(),
            signature: None,
        },
        ContentItem::Text {
            text: "relay-answer".into(),
        },
    ];
    let back_to_deepseek = rebuild_round_trip_request(&handle, snapshot.clone(), &deepseek)
        .await
        .unwrap();
    let relay_answer = back_to_deepseek
        .iter()
        .find(|message| message.role == MessageRole::Assistant && !message.content.is_empty())
        .unwrap();
    assert_eq!(
        relay_answer.content,
        vec![ContentItem::Text {
            text: "relay-answer".into()
        }]
    );
    assert!(relay_answer.provider_context_items.is_empty());
    assert!(matches!(
        rebuild_round_trip_request(&handle, snapshot, &custom).await,
        Err(SessionRuntimeError::UnreplayableReasoningHistory(_))
    ));
    for candidate_origin in [None, Some(&custom_request)] {
        assert!(matches!(
            resolve_projected_assistant_replay(
                relay_content.clone(),
                None,
                candidate_origin,
                Some(&custom),
                "relay"
            ),
            Err(SessionRuntimeError::UnreplayableReasoningHistory(_)),
        ));
    }
    assert!(back_to_deepseek.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.is_empty()
            && message.provider_context_items == envelope.provider_context_items
    }));
    assert_eq!(
        handle.read(|document| document.events().to_vec()).unwrap(),
        before
    );
}

/// Prechecks and rebuilds an exact request using a non-networked execution fixture.
/// 使用不联网的执行夹具预检并重建精确请求。
async fn rebuild_round_trip_request(
    handle: &SessionHandle,
    snapshot: SessionProviderProjectionSnapshot,
    route: &V1ReplayRouteIdentity,
) -> Result<Vec<AiMessage>, SessionRuntimeError> {
    // Align the isolated execution with the identity recorded by the historical API starts.
    // 将隔离执行与历史 API 起始事件记录的身份对齐。
    let mut context = media::tests::native_input_context().await.unwrap();
    let execution = Arc::get_mut(&mut context.execution).unwrap();
    execution.route.provider.id = route.provider_id.clone();
    execution.model.id = route.model_id.clone();
    execution.route.connection.id = route.connection_id.clone();
    execution.route.protocol_route.protocol_profile_id = route.protocol_profile_id.clone();
    execution.route.protocol_route.base_url_override = Some(
        route
            .route_fingerprint
            .strip_suffix("/responses")
            .unwrap()
            .into(),
    );
    precheck_projected_assistant_replay(
        &snapshot,
        None,
        &execution.route,
        &execution.model.id,
        execution.model.capabilities.output_limit,
    )?;

    // The transport gate validates that the candidate, execution, and request facts share the same identity.
    // 传输闸门校验候选、执行和请求事实是否使用同一身份。
    let mut candidate = candidate_for(handle);
    let mut facts = request_facts();
    facts.request = ApiRequestSnapshot {
        profile: Some(route.protocol_profile_id.clone()),
        connection_id: Some(route.connection_id.clone()),
        ..ApiRequestSnapshot::new(&route.provider_id, &route.model_id)
    };
    candidate.exact_request.model_id = route.model_id.clone();
    candidate.exact_request.cache_scope_id = Some(context.session_scope_id.clone());
    candidate.request_snapshot.provider_id = route.provider_id.clone();
    candidate.request_snapshot.model_id = route.model_id.clone();
    candidate.request_snapshot.temperature = facts.request.temperature;
    candidate.controller_config.provider_id = route.provider_id.clone();
    candidate.controller_config.model_id = route.model_id.clone();
    candidate.request_budget_guard = Some(ProviderRequestBudgetGuard::new_v1(
        route.provider_id.clone(),
        route.model_id.clone(),
        route.protocol_profile_id.clone(),
        execution.frozen_route_revision().to_owned(),
        ModelCapabilities::default(),
        ProviderRuntimeReasoningResolution::default(),
        None,
        None,
        None,
    ));
    candidate.media_context = Some(context);
    rebuild_provider_transport_candidate(
        snapshot,
        &facts,
        candidate,
        media::PreparedV1UserMedia {
            prompts: BTreeMap::new(),
            bindings: Vec::new(),
        },
    )
    .map(|prepared| prepared.transport.exact_request.messages)
}

/// Checks that selection rejects opaque same-route replay before dispatch using the DeepSeek Responses contract.
/// 验证模型选择按 DeepSeek Responses 契约在派发前拒绝同路由不透明回放。
#[tokio::test]
async fn model_switch_rejects_opaque_deepseek_reasoning_before_dispatch() {
    let mut context = media::tests::native_input_context().await.unwrap();
    let execution = Arc::get_mut(&mut context.execution).unwrap();
    execution.route.provider.id = "deepseek".into();
    execution.model.id = "deepseek-flash".into();
    execution.route.protocol_route.protocol_profile_id = "openai-responses.v1".into();
    execution.route.protocol_route.reasoning_replay_contract =
        Some(vulcan_ai_contract::ReasoningReplayContract::DeepseekResponsesPlaintextV1);
    let route = V1ReplayRouteIdentity::for_execution(&execution.route, &execution.model.id);

    for (reasoning_item, replayable) in [
        (
            json!({"type": "reasoning", "encrypted_content": "opaque"}),
            false,
        ),
        (
            json!({"type": "reasoning", "content": [{"type": "summary_text", "text": "not plaintext"}]}),
            false,
        ),
        (
            durable_reasoning_envelope().provider_context_items[0].clone(),
            true,
        ),
    ] {
        let (_directory, handle) = published_stored_open_turn();
        let mut provider_items = durable_reasoning_envelope().provider_context_items;
        provider_items[0] = reasoning_item;
        let envelope = ProviderReplayEnvelope::new(
            &route.provider_id,
            &route.connection_id,
            &route.route_fingerprint,
            &route.protocol_profile_id,
            &route.model_id,
            durable_canonical_content(),
            provider_items,
        );
        record_replay_round_trip_request(
            &handle,
            &route,
            Some(envelope),
            "durable-reasoning",
            "durable-answer",
            10,
        );
        let snapshot = handle.capture_controller_projection_snapshot().unwrap();
        let precheck = precheck_projected_assistant_replay(
            &snapshot,
            None,
            &execution.route,
            &execution.model.id,
            execution.model.capabilities.output_limit,
        );
        if replayable {
            precheck.unwrap();
        } else {
            assert!(matches!(
                precheck,
                Err(SessionRuntimeError::UnreplayableReasoningHistory(_))
            ));
        }
    }
}

/// A switch informs the model through durable User context and leaves the historical prefix intact.
/// 模型切换通过持久 User 上下文通知模型，并保持历史前缀不变。
#[test]
fn model_selection_notice_is_durable_idempotent_and_corrects_failed_switch_back() {
    // Handle contains a completed main request on provider-1/model-1, without touching user files.
    // handle 含 provider-1/model-1 上已完成的主请求，不接触用户文件。
    let (_directory, handle) = published_stored_open_turn();
    let candidate = candidate_for(&handle);
    synchronize_request_dynamic_context(&handle, &request_facts(), &candidate).unwrap();
    assert!(
        !handle
            .read(|doc| doc
                .metadata
                .dynamic_context
                .contains_key(MODEL_SELECTION_CONTEXT_KEY))
            .unwrap()
    );
    install_baseline(
        &handle,
        initial_locked_baseline_with_host_guidance(&candidate.locked_baseline).unwrap(),
    );
    // Prefix is the physical request history before the switch context is published.
    // prefix 是切换上下文发布前的物理请求历史。
    let prefix = candidate_for(&handle).exact_request.messages;
    let mut facts = request_facts();
    facts.request = ApiRequestSnapshot::new("new-provider", "new-model");
    facts.request.profile = Some("openai-responses.v1".into());
    facts.prepared_at_ms = 10;
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    let after = candidate_for(&handle).exact_request.messages;
    assert!(
        after.starts_with(&prefix),
        "a model switch must never rewrite the cached prefix"
    );
    let notice = after.iter().find(|message| message.content.iter().any(|item| matches!(item, ContentItem::Text {text} if text.contains(MODEL_SELECTION_CONTEXT_KEY)))).expect("model identity must reach the actual request");
    assert_eq!(notice.role, MessageRole::User);
    assert!(notice.content.iter().any(|item| matches!(item, ContentItem::Text {text} if text.contains("new-provider") && text.contains("new-model") && text.contains("<vulcan_context_update"))));
    let revision = handle.read(|doc| doc.memory_revision()).unwrap();
    facts.request.thinking_level = Some("max".into());
    synchronize_request_dynamic_context(&handle, &facts, &candidate).unwrap();
    assert_eq!(
        handle.read(|doc| doc.memory_revision()).unwrap(),
        revision,
        "effort changes and retries must not duplicate identity notices"
    );

    // Cold reload retains the notice even if preparation failed before any new api_started event.
    // 即使准备在任何新 api_started 事件之前失败，冷加载仍保留通知。
    let locator = handle.storage_locator().unwrap();
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
    let cold = SessionHandle::new(handle.session_id(), locator, loaded.document);
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    assert_eq!(cold.read(|doc| doc.memory_revision()).unwrap(), revision);
    facts.request = ApiRequestSnapshot::new("provider-1", "model-1");
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    let body = cold
        .read(|doc| {
            doc.metadata.dynamic_context[MODEL_SELECTION_CONTEXT_KEY]
                .raw_content
                .clone()
        })
        .unwrap();
    assert!(
        body.contains("model-1") && !body.contains("new-model"),
        "switching back before dispatch must correct the last published identity"
    );
    let before_compaction = cold.read(|doc| doc.memory_revision()).unwrap();
    facts.purpose = ApiPurposeKind::Compaction;
    facts.request = ApiRequestSnapshot::new("summary-provider", "summary-model");
    synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
    assert_eq!(
        cold.read(|doc| doc.memory_revision()).unwrap(),
        before_compaction,
        "summary routing must not change conversation identity"
    );
}
