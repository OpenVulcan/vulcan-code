//! Local compaction continuation through persistence, V1 preparation and real protocol encoding.
//! 经持久化、V1 准备及真实协议编码验证本地压缩续接。

mod vmm_epoch;

use super::*;
use serde_json::Value;
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID, COHERE_CHAT_PROFILE_ID,
    GEMINI_GENERATE_CONTENT_PROFILE_ID, GEMINI_INTERACTIONS_PROFILE_ID,
    GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_CODEX_RESPONSES_PROFILE_ID,
    OPENAI_RESPONSES_PROFILE_ID, ProtocolRegistry,
};
use vulcan_session_core::v1::document::CompactionCommitInput;
use vulcan_session_core::v1::event::AssistantSemanticStatus;

/// Builds exact `profile` request facts with the supplied execution `purpose` and logical `time`.
/// 按精确 `profile`、执行 `purpose` 与逻辑 `time` 构建请求事实。
/// Returns an isolated route without credentials or external dispatch.
/// 返回不含凭据且不进行外部分派的隔离路由。
fn facts(profile: &str, purpose: ApiPurposeKind, time: i64) -> ProviderV1RequestFacts {
    // All protocol variants share one provider/model selection, while each API freezes its exact profile.
    // 所有协议变体共享一个供应商及模型选择，每个 API 则冻结其精确档案。
    let mut request = ApiRequestSnapshot::new("provider-1", "model-1");
    request.profile = Some(profile.into());
    ProviderV1RequestFacts {
        provider_call_id: format!("compaction-wire-{time}"),
        purpose,
        continuation_action_id: None,
        request,
        prepared_at_ms: time,
    }
}

/// Builds `handle`'s production candidate with `profile` and optional persistent `context`.
/// 使用 `profile` 与可选持久 `context` 构建 `handle` 的生产候选。
/// Returns a candidate whose final budget and messages are rebuilt by the real V1 gate.
/// 返回由真实 V1 闸门重建最终预算和消息的候选。
fn candidate(
    handle: &SessionHandle,
    profile: &str,
    context: Option<&str>,
) -> ProviderRequestPreparationCandidate {
    // The shared fixture begins with actual durable history and a valid host-guided baseline.
    // 共享样例以实际可靠历史及合法宿主指引基线开始。
    let mut candidate = super::host_guidance::candidate_for(handle);
    // Anthropic's real encoder requires the output cap already resolved by model preparation.
    // Anthropic 真实编码器要求模型准备阶段已经解析输出上限。
    candidate.exact_request.output_token_limit = Some(1024);
    candidate.controller_config.context_injection =
        Some(vulcan_core::types::ProviderContextInjection {
            persistent_blocks: context.into_iter().map(str::to_owned).collect(),
            ..Default::default()
        });
    candidate.request_budget_guard = Some(ProviderRequestBudgetGuard::new_v1(
        "provider-1",
        "model-1",
        profile,
        format!("sha256:{profile}"),
        ModelCapabilities::default(),
        ProviderRuntimeReasoningResolution::default(),
        None,
        None,
        None,
    ));
    candidate
}

/// Completes `api` on `handle` with exact `request`, semantic `text` and terminal time `time`.
/// 使用精确 `request`、语义 `text` 及终态时间 `time` 完成 `handle` 上的 `api`。
/// Returns the committed semantic sequence without closing the enclosing business turn.
/// 返回已提交语义序号，不关闭外层业务回合。
fn finish(
    handle: &SessionHandle,
    api: ApiCallId,
    request: ApiRequestSnapshot,
    text: &str,
    time: i64,
) -> Seq {
    // Both writes use the production domain and on-disk publication boundary.
    // 两次写入均使用生产领域及磁盘发布边界。
    let summary = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                document.append_assistant_text(api, text, AssistantSemanticStatus::Completed, time)
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
                        timing: Default::default(),
                        provider_source: None,
                        completed_at_ms: time + 1,
                    },
                )
            },
        )
        .unwrap();
    summary
}

/// Prepares one `facts`/`candidate` pair for `handle` in `state` through a fresh stream sink.
/// 在 `state` 中通过全新流 sink 为 `handle` 准备一组 `facts`/`candidate`。
/// Returns the exact production permit and panics if the isolated fixture is rejected.
/// 返回精确生产许可；隔离样例被拒绝时令测试失败。
async fn prepare(
    handle: &Arc<SessionHandle>,
    state: &SharedServiceBridgeState,
    facts: ProviderV1RequestFacts,
    candidate: ProviderRequestPreparationCandidate,
) -> PreparedPhysicalApi {
    // A fresh sink models controller restarts without retaining old physical request identities.
    // 全新 sink 模拟控制器重启，不保留旧物理请求身份。
    let mut sink = ProviderV1StreamSink::new(
        handle.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    sink.prepare_request(state, facts, candidate).await.unwrap()
}

/// Asserts that `request` encodes one user-ended continuation on the exact registered `profile`.
/// 断言 `request` 在精确登记的 `profile` 上编码出以用户角色结尾的续接。
/// Uses protocol-owned field paths; returns nothing and never dispatches a request.
/// 使用协议自有字段路径，无返回值且绝不分派请求。
pub(super) fn assert_user_tail(profile: &str, request: &AiExecutionRequest) {
    // Run the real encoder; controller roles alone do not prove the upstream message shape.
    // 执行真实编码器，仅有控制器角色不足以证明上游消息形态。
    let encoded = ProtocolRegistry::first_batch()
        .unwrap()
        .require(profile)
        .unwrap()
        .encode(request)
        .unwrap();
    // Each supported protocol has one explicit message list and role/type contract.
    // 每个受测协议只有一个明确的消息列表及角色或类型契约。
    let (pointer, field, value) = match profile {
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | OPENAI_CHAT_PROFILE_ID
        | COHERE_CHAT_PROFILE_ID => ("/messages", "role", "user"),
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            ("/input", "role", "user")
        }
        GEMINI_GENERATE_CONTENT_PROFILE_ID => ("/contents", "role", "user"),
        GOOGLE_CODE_ASSIST_PROFILE_ID => ("/request/contents", "role", "user"),
        GEMINI_INTERACTIONS_PROFILE_ID => ("/input", "type", "user_input"),
        _ => panic!("unregistered test protocol: {profile}"),
    };
    // Missing protocol lists are fixture drift, never a reason to try another JSON path.
    // 协议列表缺失属于样例漂移，绝不能因此尝试其他 JSON 路径。
    let tail = encoded
        .body
        .pointer(pointer)
        .and_then(Value::as_array)
        .and_then(|messages| messages.last())
        .expect("wire message list is missing; protocol fixture may need updating");
    assert_eq!(tail[field], value, "{profile}");
}

/// Exercises summary commit, optional cold reload and first physical request without relying on context updates.
/// 验证摘要提交、可选冷重载与首次物理请求，不依赖上下文更新。
/// Covers missing/unchanged/changed persistent context and every supported text protocol; no network is used.
/// 覆盖持久上下文缺失、不变、变化及各受测文本协议，不使用网络。
#[tokio::test]
async fn compaction_first_request_reaches_real_encoders_without_incidental_user_updates() {
    for profile in [
        ANTHROPIC_MESSAGES_PROFILE_ID,
        ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        OPENAI_RESPONSES_PROFILE_ID,
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
        OPENAI_CHAT_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID,
        GOOGLE_CODE_ASSIST_PROFILE_ID,
        GEMINI_INTERACTIONS_PROFILE_ID,
        COHERE_CHAT_PROFILE_ID,
    ] {
        for (cold, before_context, after_context) in [
            (false, None, None),
            (true, Some("persistent-fixture"), Some("persistent-fixture")),
            (true, Some("persistent-fixture"), Some("updated-fixture")),
        ] {
            // Each case owns real temporary storage and a registered service owner.
            // 每种情况拥有真实临时存储和已登记服务所有者。
            let (_directory, handle) = published_stored_open_turn();
            // The same service is used at every preparation boundary in this case.
            // 此情况的所有准备边界均使用同一个服务。
            let service = crate::tests::test_support::test_service_state_with_parent_session(
                handle.session_id(),
            );
            service
                .session_runtime_registry
                .register(handle.clone())
                .unwrap();
            // Shared state carries the production request admission services.
            // 共享状态承载生产请求准入服务。
            let state = Arc::new(std::sync::Mutex::new(service));
            // Establish baseline and dynamic producer state through a real business permit.
            // 通过真实业务许可建立基线和动态生产者状态。
            let initial_facts = facts(profile, ApiPurposeKind::MainGeneration, 10);
            let PreparedPhysicalApi::SessionV1 { core_start, .. } = prepare(
                &handle,
                &state,
                initial_facts.clone(),
                candidate(&handle, profile, before_context),
            )
            .await
            else {
                panic!("initial business permit");
            };
            finish(
                &handle,
                core_start.api_call_id,
                initial_facts.request,
                "retained-business-fixture",
                11,
            );
            // The summary itself is admitted as an auxiliary physical request with its own instruction.
            // 摘要自身作为携带独立指令的辅助物理请求准入。
            let summary_facts = facts(profile, ApiPurposeKind::Compaction, 20);
            let PreparedPhysicalApi::SessionV1 {
                core_start: summary_start,
                ..
            } = prepare(
                &handle,
                &state,
                summary_facts.clone(),
                candidate(&handle, profile, before_context),
            )
            .await
            else {
                panic!("summary permit");
            };
            // Keep or discard the business tail according to this case while always retaining the authoritative summary.
            // 按此情况保留或丢弃业务尾段，但始终保留权威摘要。
            let summary_seq = finish(
                &handle,
                summary_start.api_call_id,
                summary_facts.request,
                "compaction-summary-fixture",
                21,
            );
            let retained = if cold {
                Seq::try_new(1).unwrap()
            } else {
                summary_seq
            };
            handle
                .record_compaction(
                    CompactionCommitInput::new(
                        summary_start.api_call_id,
                        summary_seq,
                        retained,
                        "compaction-summary-fixture",
                        super::super::memory_acceptance::host_guided_locked_baseline(),
                    ),
                    23,
                )
                .unwrap();
            // Projection before physical preparation must already contain the continuation.
            // 物理准备之前的投影就必须包含续接信息。
            let warm = handle.capture_controller_projection_snapshot().unwrap();
            let durable_revision = warm.document_revision;
            // A restarted owner reads only persisted facts; no transient continuation seed survives.
            // 重启所有者只读取持久事实，不保留任何瞬态续接种子。
            let handle = if cold {
                let locator = handle.storage_locator().unwrap();
                let loaded =
                    vulcan_workspace_store::workspace_store::load_session_v1_full(&locator)
                        .unwrap();
                SessionHandle::new(handle.session_id(), locator, loaded.document)
            } else {
                handle
            };
            assert_eq!(
                handle
                    .capture_controller_projection_snapshot()
                    .unwrap()
                    .sourced_items,
                warm.sourced_items
            );
            assert_eq!(
                handle.read(|doc| doc.memory_revision()).unwrap(),
                durable_revision
            );
            // Use a registry containing this exact warm or cold owner.
            // 使用登记此精确热所有者或冷所有者的注册表。
            let service = crate::tests::test_support::test_service_state_with_parent_session(
                handle.session_id(),
            );
            service
                .session_runtime_registry
                .register(handle.clone())
                .unwrap();
            let state = Arc::new(std::sync::Mutex::new(service));
            let PreparedPhysicalApi::SessionV1 {
                exact_request,
                controller_snapshot,
                request_snapshot,
                core_start: resumed,
                ..
            } = prepare(
                &handle,
                &state,
                facts(profile, ApiPurposeKind::MainGeneration, 30),
                candidate(&handle, profile, after_context),
            )
            .await
            else {
                panic!("first post-compaction permit");
            };
            // Find the control by its durable node identity, not prose or an unstable message index.
            // 按可靠节点身份定位控制消息，不依赖文案或不稳定消息下标。
            let control_id = format!(
                "v1-compaction-continuation-{}",
                warm.current_compaction_id.unwrap().value()
            );
            assert_eq!(exact_request.messages.iter().filter(|message| message.content.iter().any(|part| matches!(part, ContentItem::Text { text } if text.contains(&control_id)))).count(), 1);
            assert_eq!(controller_snapshot.messages.iter().filter(|message| matches!(&message.payload, vulcan_core::types::ConversationPayload::UserText { text, .. } if text.contains(&control_id))).count(), 1);
            assert_eq!(
                request_snapshot.history.len(),
                controller_snapshot.messages.len()
            );
            assert!(request_snapshot.prompt_in_history);
            assert!(exact_request.continuation.is_none());
            assert_user_tail(profile, &exact_request);
            // Unchanged producer state creates no incidental event between summary completion and the new permit.
            // 生产者状态不变时，摘要完成与新许可之间不得出现偶然补位事件。
            if before_context == after_context {
                assert!(
                    handle
                        .read(|doc| !doc
                            .events()
                            .iter()
                            .any(|event| event.seq() > warm.through_seq
                                && matches!(event.event, EventBody::SystemContext(_))))
                        .unwrap()
                );
            }
            // The final request fingerprint covers the continuation before ApiStarted is committed.
            // 最终请求指纹在提交 ApiStarted 之前就包含续接消息。
            let expected_fingerprint = sha256_hex(&serde_json::to_vec(&exact_request).unwrap());
            assert!(handle.read(|doc| doc.events().iter().any(|event| matches!(&event.event, EventBody::ApiStarted(start) if start.api_call_id == resumed.api_call_id && start.projection.fingerprint_sha256 == expected_fingerprint))).unwrap());

            // Subsequent assistant output must remain after the fixed control, never trigger a new trailing control.
            // 后续助手输出必须保留在固定控制之后，绝不能触发新的尾部控制。
            let prepared_prefix = handle.capture_controller_projection_snapshot().unwrap();
            finish(
                &handle,
                resumed.api_call_id,
                facts(profile, ApiPurposeKind::MainGeneration, 30).request,
                "post-compaction-business-fixture",
                31,
            );
            // Repeated projection is read-only and preserves the complete previously prepared prefix.
            // 重复投影只读，并保留之前准备的完整前缀。
            let assistant_tail = handle.capture_controller_projection_snapshot().unwrap();
            assert!(
                assistant_tail
                    .sourced_items
                    .starts_with(&prepared_prefix.sourced_items)
            );
            assert!(
                matches!(assistant_tail.items.last(), Some(ProjectedItem::AssistantText { text, .. }) if text == "post-compaction-business-fixture")
            );
            assert_eq!(
                handle
                    .capture_controller_projection_snapshot()
                    .unwrap()
                    .sourced_items,
                assistant_tail.sourced_items
            );

            // A real later user supplement enters through its durable admission and promotion APIs.
            // 真实的后续用户补充通过可靠准入及晋升 API 入流。
            handle
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::InputAccepted,
                    |document| {
                        document.add_pending_in_turn_input(
                            ClientInputSubmission::new(
                                ClientActionId::try_new("compaction-followup-user")?,
                                "newer-user-fixture",
                                Vec::new(),
                            ),
                            33,
                        )
                    },
                )
                .unwrap();
            handle
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::InputAccepted,
                    |document| document.promote_pending_in_turn_inputs_after_api(34),
                )
                .unwrap();
            // Switch across protocol families on this same durable owner, including every source profile in the matrix.
            // 在同一可靠所有者上跨协议族切换，覆盖矩阵中的每个来源档案。
            let switched_profile = if profile == ANTHROPIC_MESSAGES_PROFILE_ID {
                OPENAI_RESPONSES_PROFILE_ID
            } else {
                ANTHROPIC_MESSAGES_PROFILE_ID
            };
            // No stale continuation action or protocol-specific summary cursor is carried across the switch.
            // 切换时不携带过期续接动作或协议专属摘要游标。
            let switched_facts = facts(switched_profile, ApiPurposeKind::MainGeneration, 40);
            let PreparedPhysicalApi::SessionV1 {
                exact_request: switched_request,
                core_start: switched_start,
                ..
            } = prepare(
                &handle,
                &state,
                switched_facts.clone(),
                candidate(&handle, switched_profile, after_context),
            )
            .await
            else {
                panic!("switched business permit");
            };
            assert_user_tail(switched_profile, &switched_request);
            assert!(switched_request.continuation.is_none());
            // Switching may append a host notice, but cannot rewrite or move the earlier compaction prefix.
            // 切换可以追加宿主通知，但不能改写或移动较早的压缩前缀。
            let switched_projection = handle.capture_controller_projection_snapshot().unwrap();
            assert!(
                switched_projection
                    .sourced_items
                    .starts_with(&assistant_tail.sourced_items)
            );
            assert_eq!(switched_projection.items.iter().filter(|item| matches!(item, ProjectedItem::SystemText { text, .. } if text.contains(&control_id))).count(), 1);
            assert!(switched_projection.sourced_items.iter().any(|item| item.source_from_seq > warm.through_seq && matches!(&item.item, ProjectedItem::UserInput { text, .. } if text == "newer-user-fixture")));
            finish(
                &handle,
                switched_start.api_call_id,
                switched_facts.request,
                "switched-business-fixture",
                41,
            );

            // A second committed summary starts a new generation and replaces only the derived control.
            // 第二份已提交摘要开启新代际，仅替换派生控制消息。
            let next_summary_facts = facts(switched_profile, ApiPurposeKind::Compaction, 50);
            let PreparedPhysicalApi::SessionV1 {
                core_start: next_summary_start,
                ..
            } = prepare(
                &handle,
                &state,
                next_summary_facts.clone(),
                candidate(&handle, switched_profile, after_context),
            )
            .await
            else {
                panic!("second summary permit");
            };
            // Completing an auxiliary API alone does not publish a new continuation boundary.
            // 单独完成辅助 API 不会发布新的续接边界。
            let next_summary_seq = finish(
                &handle,
                next_summary_start.api_call_id,
                next_summary_facts.request,
                "second-summary-fixture",
                51,
            );
            assert_eq!(
                handle
                    .capture_controller_projection_snapshot()
                    .unwrap()
                    .current_compaction_id,
                warm.current_compaction_id
            );
            handle
                .record_compaction(
                    CompactionCommitInput::new(
                        next_summary_start.api_call_id,
                        next_summary_seq,
                        retained,
                        "second-summary-fixture",
                        super::super::memory_acceptance::host_guided_locked_baseline(),
                    ),
                    53,
                )
                .unwrap();
            // Retaining the older summary must not retain its obsolete synthetic control.
            // 保留较早摘要时，不得保留其过期的合成控制消息。
            let second = handle.capture_controller_projection_snapshot().unwrap();
            let next_control_id = format!(
                "v1-compaction-continuation-{}",
                second.current_compaction_id.unwrap().value()
            );
            assert_ne!(second.current_compaction_id, warm.current_compaction_id);
            assert!(!second.items.iter().any(|item| matches!(item, ProjectedItem::SystemText { text, .. } if text.contains(&control_id))));
            assert_eq!(second.items.iter().filter(|item| matches!(item, ProjectedItem::SystemText { text, .. } if text.contains(&next_control_id))).count(), 1);
            // Switch back after recompaction and verify the actual first request again.
            // 再次压缩后切回原协议，并再次验证实际首次请求。
            let PreparedPhysicalApi::SessionV1 {
                exact_request: second_request,
                ..
            } = prepare(
                &handle,
                &state,
                facts(profile, ApiPurposeKind::MainGeneration, 60),
                candidate(&handle, profile, after_context),
            )
            .await
            else {
                panic!("second post-compaction permit");
            };
            assert_user_tail(profile, &second_request);
        }
    }
}
