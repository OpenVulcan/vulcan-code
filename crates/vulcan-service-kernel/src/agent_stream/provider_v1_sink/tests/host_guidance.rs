//! Regression coverage for immutable baseline recognition and durable guidance upgrades.
//! 不可变基线识别与持久指引升级的回归覆盖。

use super::super::host_guidance::{HOST_GUIDANCE_UPDATE_KEY, baseline_version};
use super::*;
use vulcan_ai_contract::{HOST_MESSAGE_GUIDANCE_REVISIONS, HOST_MESSAGE_GUIDANCE_VERSION};
use vulcan_session_core::v1::document::CompactionCommitInput;
use vulcan_session_core::v1::event::{AssistantSemanticStatus, SystemContextOperation};

/// Builds a baseline from the exact registered `text`; returns a fully validated projection.
/// 使用精确已登记 `text` 构造基线，返回完整校验的投影。
fn baseline_with_text(text: &str) -> LockedBaselineProjection {
    LockedBaselineProjection::project(vec![LockedBaselineSegmentInput {
        kind: LockedSourceKind::BaselineHooks,
        node_id: "host-message-guidance".into(),
        target: LockedPromptTarget::System,
        source_label: Some("host_message_guidance".into()),
        source_id: Some("v1:host_message_guidance".into()),
        content: text.into(),
    }])
    .unwrap()
}

/// Checks that dispatch and preview both hide the file field of a persisted skill catalog.
/// 验证派发与预览都隐藏持久化技能目录中的文件字段。
#[test]
fn provider_projects_legacy_skill_catalog_without_file_paths() {
    // Use the historical wire format and a path that can be checked independently of the wording.
    // 使用历史线路格式，并通过样例路径独立于文案验证结果。
    let legacy = concat!(
        "<skills_instructions>\n## Skills\n",
        "When the task matches a listed skill, call `skill` with its listed name before choosing generic tools. Load only the skills relevant to the task.\n",
        "### Available skills\n- fixture: Review source (file: /only-in-saved-baseline/SKILL.md)",
        "\n### How to use skills\n",
        "- Match the user's task against the skill descriptions before loading a skill.\n",
        "- Call `skill` with the exact skill name to load its full SKILL.md content and resource directory.\n",
        "- Do not assume scripts or references until the skill content or its base directory has been loaded.\n",
        "</skills_instructions>",
    );
    // The source kind limits the transformation to skill catalogs.
    // 来源类型将转换范围限定为技能目录。
    let baseline = LockedBaselineProjection::project(vec![LockedBaselineSegmentInput {
        kind: LockedSourceKind::PromptSkillInjection,
        node_id: "fixture-skill-catalog".into(),
        target: LockedPromptTarget::Developer,
        source_label: None,
        source_id: Some("enabled-prompt-skills".into()),
        content: legacy.into(),
    }])
    .unwrap();
    // Both readers use this baseline, which still contains the original bytes.
    // 两个读取方使用同一基线，基线仍持有原始字节。
    let fixed = ProviderPromptLayers::default();
    let mut messages = Vec::new();
    let mut prompt_parts = Vec::new();
    append_provider_request_prefix(&fixed, &baseline, &mut messages, &mut prompt_parts).unwrap();
    let (preview, _) = preview_request_prefix(&fixed, &baseline).unwrap();

    assert_eq!(preview, messages);
    assert!(prompt_parts.iter().any(|text| text.contains("fixture")));
    assert!(
        prompt_parts
            .iter()
            .all(|text| !text.contains("/only-in-saved-baseline/"))
    );
    assert!(
        baseline
            .locked_sources()
            .iter()
            .any(|source| source.content == legacy)
    );
}

/// Builds a valid candidate for `handle` through the real physical reconstruction; dispatches nothing.
/// 通过真实物理重建为 `handle` 构造合法候选；不发送请求。
pub(super) fn candidate_for(handle: &SessionHandle) -> ProviderRequestPreparationCandidate {
    super::super::memory_acceptance::rebuild_memory_acceptance_request(
        handle.capture_controller_projection_snapshot().unwrap(),
        Default::default(),
    )
    .unwrap()
}

/// Initializes the durable `handle` with `baseline`; fixtures preserve the same runtime lock as production.
/// 为持久 `handle` 初始化 `baseline`；夹具保留与生产一致的运行时锁。
pub(super) fn install_baseline(handle: &SessionHandle, baseline: LockedBaselineProjection) {
    // Establish active request configuration through the historical physical admission, too.
    // 同时通过历史物理准入建立活动请求配置。
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
    let request = ApiRequestSnapshot::new("provider-1", "model-1");
    let api = handle
        .prepare_and_start_api_durable(PrepareAndStartApiInput {
            purpose: ApiPurposeKind::MainGeneration,
            request: request.clone(),
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection,
            locked_baseline: baseline,
            continuation_action_id: None,
            now_ms: 3,
        })
        .unwrap()
        .value
        .api_call_id;
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api,
                    ApiCompletion {
                        request,
                        usage: Default::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: 4,
                    },
                )
            },
        )
        .unwrap();
}

/// Every released text is accepted without rewriting; unregistered text and identity drift stay invalid.
/// 每版已发布正文原样接纳；未登记正文与身份漂移仍然无效。
#[test]
fn host_guidance_recognizes_exact_history_and_rejects_corruption() {
    for (index, text) in HOST_MESSAGE_GUIDANCE_REVISIONS.iter().enumerate() {
        let baseline = baseline_with_text(text);
        assert_eq!(baseline_version(&baseline).unwrap(), Some(index + 1));
        assert_eq!(
            initial_locked_baseline_with_host_guidance(&baseline).unwrap(),
            baseline
        );
    }
    assert!(initial_locked_baseline_with_host_guidance(&baseline_with_text("unknown")).is_err());
    for field in ["kind", "target", "label", "id"] {
        let mut segments = baseline_with_text(HOST_MESSAGE_GUIDANCE).into_locked_sources();
        let source = segments
            .iter_mut()
            .find(|source| source.node_id == "host-message-guidance")
            .unwrap();
        match field {
            "kind" => source.kind = LockedSourceKind::PluginInstructions,
            "target" => source.target = LockedPromptTarget::Developer,
            "label" => source.source_label = Some("forged".into()),
            "id" => source.source_id = Some("forged".into()),
            _ => unreachable!(),
        }
        let invalid: LockedBaselineProjection =
            serde_json::from_value(json!({"locked_sources": segments})).unwrap();
        assert!(
            initial_locked_baseline_with_host_guidance(&invalid).is_err(),
            "{field}"
        );
    }
}

/// Old baselines append one User upgrade, survive cold load, and enter the real physical API preparation.
/// 旧基线追加一条 User 升级消息，冷加载后保持幂等，并进入真实物理 API 准备。
#[tokio::test]
async fn host_guidance_upgrade_preserves_prefix_and_survives_reload_and_preparation() {
    for text in HOST_MESSAGE_GUIDANCE_REVISIONS
        .iter()
        .filter(|text| **text != HOST_MESSAGE_GUIDANCE)
    {
        let (_directory, handle) = published_stored_open_turn();
        let baseline = baseline_with_text(text);
        install_baseline(&handle, baseline.clone());
        let mut candidate = candidate_for(&handle);
        let prefix = candidate.exact_request.messages.clone();
        // A fresh composer cannot replace the persisted historical baseline's authority.
        // 新组合器不能替代持久历史基线的权威。
        candidate.locked_baseline = baseline_with_text(HOST_MESSAGE_GUIDANCE);
        let facts = ProviderV1RequestFacts {
            request: ApiRequestSnapshot::new("provider-1", "model-1"),
            prepared_at_ms: 10,
            ..request_facts()
        };
        context_injection::synchronize_request_dynamic_context(&handle, &facts, &candidate)
            .unwrap();
        let revision = handle.read(|document| {
            assert_eq!(document.runtime_lock.locked_sources, baseline.locked_sources());
            assert_eq!(document.events().iter().filter(|row| matches!(row.event, EventBody::UserInput(_))).count(), 1);
            assert_eq!(document.events().iter().filter(|row| matches!(&row.event, EventBody::SystemContext(context)
                if context.context_key == HOST_GUIDANCE_UPDATE_KEY && context.operation == SystemContextOperation::Upsert)).count(), 1);
            document.memory_revision()
        }).unwrap();
        let rebuilt = candidate_for(&handle);
        assert!(rebuilt.exact_request.messages.starts_with(&prefix));
        let upgrade = rebuilt.exact_request.messages.last().unwrap();
        assert_eq!(upgrade.role, MessageRole::User);
        assert!(
            matches!(upgrade.content.as_slice(), [ContentItem::Text { text }]
            if text.contains("<vulcan_context_update") && text.contains(HOST_GUIDANCE_UPDATE_KEY)
            && text.contains(&format!("Host message guidance revision {HOST_MESSAGE_GUIDANCE_VERSION}")))
        );

        // Reload from actual published files before checking idempotency and the dispatch gate.
        // 从真实发布文件重载后，再检查幂等与分派闸门。
        let locator = handle.storage_locator().unwrap();
        let loaded =
            vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
        let cold = Arc::new(SessionHandle::new(
            handle.session_id(),
            locator,
            loaded.document,
        ));
        context_injection::synchronize_request_dynamic_context(&cold, &facts, &candidate).unwrap();
        assert_eq!(
            cold.read(|document| document.memory_revision()).unwrap(),
            revision
        );
        assert_eq!(
            candidate_for(&cold).exact_request.messages,
            rebuilt.exact_request.messages
        );

        let service =
            crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
        service
            .session_runtime_registry
            .register(Arc::clone(&cold))
            .unwrap();
        let state = Arc::new(std::sync::Mutex::new(service));
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&cold),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        let prepared = sink
            .prepare_request(&state, facts, candidate)
            .await
            .unwrap();
        let PreparedPhysicalApi::SessionV1 { exact_request, .. } = prepared else {
            panic!("unchanged session must prepare a physical request");
        };
        assert_eq!(exact_request.messages, rebuilt.exact_request.messages);
        assert_eq!(sink.active.as_ref().unwrap().locked_baseline, baseline);
    }
}

/// Invalid initialized guidance fails before publishing otherwise valid dynamic context changes.
/// 已初始化指引无效时，在发布其他合法动态上下文改动之前失败。
#[test]
fn host_guidance_invalid_baseline_does_not_partially_publish_context() {
    for baseline in [
        baseline_with_text("unregistered"),
        LockedBaselineProjection::empty(),
    ] {
        let (_directory, handle) = published_stored_open_turn();
        let mut candidate = candidate_for(&handle);
        candidate.dynamic_prompt_layers.runtime_context = "must not be published".into();
        install_baseline(&handle, baseline);
        let before = handle
            .read(|document| {
                (
                    document.memory_revision(),
                    serde_json::to_value(document.events()).unwrap(),
                )
            })
            .unwrap();
        assert!(
            context_injection::synchronize_request_dynamic_context(
                &handle,
                &request_facts(),
                &candidate
            )
            .is_err()
        );
        assert_eq!(
            before,
            handle
                .read(|document| (
                    document.memory_revision(),
                    serde_json::to_value(document.events()).unwrap()
                ))
                .unwrap()
        );
    }
}

/// Compaction leaves the producer untouched and carries its active User overlay into the new window.
/// 压缩不改动生产者，并将其有效 User 覆盖层带入新窗口。
#[test]
fn host_guidance_upgrade_survives_compaction_and_current_baseline_needs_no_overlay() {
    for refresh_baseline in [false, true] {
        let (_directory, handle) = published_stored_open_turn();
        let old = HOST_MESSAGE_GUIDANCE_REVISIONS
            .iter()
            .find(|text| **text != HOST_MESSAGE_GUIDANCE)
            .unwrap();
        let baseline = baseline_with_text(old);
        install_baseline(&handle, baseline.clone());
        let candidate = candidate_for(&handle);
        let business_facts = ProviderV1RequestFacts {
            prepared_at_ms: 10,
            ..request_facts()
        };
        context_injection::synchronize_request_dynamic_context(
            &handle,
            &business_facts,
            &candidate,
        )
        .unwrap();
        let revision = handle.read(|document| document.memory_revision()).unwrap();
        let facts = ProviderV1RequestFacts {
            purpose: ApiPurposeKind::Compaction,
            ..request_facts()
        };
        context_injection::synchronize_request_dynamic_context(&handle, &facts, &candidate)
            .unwrap();
        assert_eq!(
            revision,
            handle.read(|document| document.memory_revision()).unwrap()
        );

        // Each durable operation publishes exactly one revision, matching production compaction ownership.
        // 每次可靠操作只发布一个修订，与生产压缩归属一致。
        let request = ApiRequestSnapshot::new("provider-1", "model-1");
        let projection = handle
            .read(|document| {
                let through_seq = document.events().last().unwrap().seq;
                ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                }
            })
            .unwrap();
        let api = handle
            .prepare_and_start_api_durable(PrepareAndStartApiInput {
                purpose: ApiPurposeKind::Compaction,
                request: request.clone(),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection,
                locked_baseline: baseline.clone(),
                continuation_action_id: None,
                now_ms: 12,
            })
            .unwrap()
            .value
            .api_call_id;
        let summary_seq = handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.append_assistant_text(
                        api,
                        "summary",
                        AssistantSemanticStatus::Completed,
                        13,
                    )
                },
            )
            .unwrap()
            .value
            .into_value();
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| {
                    document.complete_api_group(
                        api,
                        ApiCompletion {
                            request,
                            usage: Default::default(),
                            status: ApiCallStatus::Completed,
                            end_reason: Some(ApiEndReason::CompletedSuccessfully),
                            error: None,
                            continuation_eligibility: ContinuationEligibility::Forbidden,
                            timing: ApiTimings::default(),
                            provider_source: None,
                            completed_at_ms: 14,
                        },
                    )
                },
            )
            .unwrap();
        // Cover both retained old baselines and an explicitly refreshed compaction generation.
        // 同时覆盖保留旧基线和显式刷新后的压缩代次。
        let compacted_baseline = if refresh_baseline {
            baseline_with_text(HOST_MESSAGE_GUIDANCE)
        } else {
            baseline
        };
        handle
            .record_compaction(
                CompactionCommitInput::new(
                    api,
                    summary_seq,
                    summary_seq,
                    "summary",
                    compacted_baseline,
                ),
                15,
            )
            .unwrap();
        let rebuilt = candidate_for(&handle);
        assert_eq!(
            rebuilt
                .exact_request
                .messages
                .iter()
                .filter(|message| message.role == MessageRole::User
                    && message
                        .content
                        .iter()
                        .any(|content| matches!(content, ContentItem::Text { text }
                if text.contains(HOST_GUIDANCE_UPDATE_KEY))))
                .count(),
            1
        );
        let compacted_revision = handle.read(|document| document.memory_revision()).unwrap();
        let next_facts = ProviderV1RequestFacts {
            prepared_at_ms: 16,
            ..business_facts
        };
        context_injection::synchronize_request_dynamic_context(&handle, &next_facts, &candidate)
            .unwrap();
        if refresh_baseline {
            let after = candidate_for(&handle);
            assert!(
                after
                    .exact_request
                    .messages
                    .starts_with(&rebuilt.exact_request.messages)
            );
            assert!(after.exact_request.messages.last().unwrap().content.iter().any(|content|
                matches!(content, ContentItem::Text { text } if text.contains("<vulcan_context_expired")
                    && text.contains(HOST_GUIDANCE_UPDATE_KEY))));
        } else {
            assert_eq!(
                compacted_revision,
                handle.read(|document| document.memory_revision()).unwrap()
            );
        }
        let synced_revision = handle.read(|document| document.memory_revision()).unwrap();
        context_injection::synchronize_request_dynamic_context(&handle, &next_facts, &candidate)
            .unwrap();
        assert_eq!(
            synced_revision,
            handle.read(|document| document.memory_revision()).unwrap()
        );
    }

    // New baselines already contain the current guide and need no additional event.
    // 新基线已含当前指引，不需要额外事件。
    let (_directory, current) = published_stored_open_turn();
    let candidate = candidate_for(&current);
    install_baseline(&current, baseline_with_text(HOST_MESSAGE_GUIDANCE));
    // The first preparation may commit metadata (the frozen answer mode) but no event.
    // 首次准备可能提交元数据（冻结的回答模式），但不得追加事件。
    let events = current
        .read(|document| serde_json::to_value(document.events()).unwrap())
        .unwrap();
    context_injection::synchronize_request_dynamic_context(&current, &request_facts(), &candidate)
        .unwrap();
    assert_eq!(
        events,
        current
            .read(|document| serde_json::to_value(document.events()).unwrap())
            .unwrap()
    );
}
