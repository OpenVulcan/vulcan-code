//! Final-budget rejection and durable Continue recovery through the production V1 sink.
//! 经生产 V1 sink 验证最终预算拦截及可靠 Continue 恢复。

use super::*;
use vulcan_context::{ContextCompactionPolicy, ContextWindowPolicy};
use vulcan_session_core::v1::ContinuationRequest;
use vulcan_session_core::v1::document::CompactionCommitInput;
use vulcan_session_core::v1::event::AssistantSemanticStatus;

/// Rebuilds `handle` with an eligible or auxiliary budget guard selected by `armed`; returns a real candidate.
/// 根据 `armed` 选取普通或辅助预算守卫并重建 `handle`，返回真实候选。
fn budget_candidate(handle: &SessionHandle, armed: bool) -> ProviderRequestPreparationCandidate {
    // Production reconstruction supplies exact history and immutable baseline before the final permit.
    // 生产重建在最终许可前提供精确历史和不可变基线。
    let mut candidate = super::host_guidance::candidate_for(handle);
    candidate.request_budget_guard = Some(
        ProviderRequestBudgetGuard::new_v1(
            "provider-1",
            "model-1",
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            "sha256:compaction-route",
            ModelCapabilities {
                context_limit: Some(1_000_000),
                ..Default::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy {
                window: ContextWindowPolicy {
                    auto_compact_token_limit: Some(335700),
                    ..Default::default()
                },
                ..Default::default()
            }),
            None,
            None,
        )
        .with_turn_compaction_line(armed),
    );
    candidate
}

/// Counts durable API starts on `handle`; returns an event-type count independent of sequence positions.
/// 统计 `handle` 的可靠 API 起始事件，返回不依赖序号位置的事件类型计数。
fn starts(handle: &SessionHandle) -> usize {
    handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventBody::ApiStarted(_)))
                .count()
        })
        .unwrap()
}

/// Verifies final recalibration stops before Continue consumption, then compaction and a fresh Continue succeed.
/// 验证最终重新校准在消费 Continue 前停止，随后压缩及新的 Continue 均成功。
/// Takes no arguments; returns no value, using real temporary persistence without provider dispatch.
/// 无参数、无返回值，使用真实临时持久化，不分派供应商请求。
#[tokio::test]
async fn final_compaction_boundary_preserves_continue_until_post_compaction_permit() {
    // A real on-disk session ensures anchors and action identities survive cold reload.
    // 真实磁盘会话确保锚点和动作身份跨冷重载存续。
    let (_directory, handle) = published_stored_open_turn();
    // The service supplies the same collaboration and preparation owners used by a live turn.
    // 服务提供实时回合使用的同一协作及准备所有者。
    let service =
        crate::tests::test_support::test_service_state_with_parent_session(handle.session_id());
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    // Shared service state owns no network or credentials.
    // 共享服务状态不拥有网络或凭证。
    let state = Arc::new(std::sync::Mutex::new(service));
    // Each host restart creates a new stream sink over the same durable owner.
    // 每次宿主重启在同一持久所有者之上创建新的流 sink。
    let mut sink = ProviderV1StreamSink::new(
        handle.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    // The initial physical request establishes exact accounting evidence through the production gate.
    // 初始物理请求经生产闸门建立精确记账证据。
    let request = ApiRequestSnapshot::new("provider-1", "model-1");
    // Facts identify the same route reconstructed by the fixture and never bind a Continue yet.
    // 事实标识夹具重建的同一路由，此时尚不绑定 Continue。
    let facts = ProviderV1RequestFacts {
        request: request.clone(),
        prepared_at_ms: 10,
        ..request_facts()
    };
    // The character-only precheck is comfortably under the configured line.
    // 仅字符预检远低于配置的压缩线。
    let initial = budget_candidate(&handle, true);
    assert!(!initial.context_budget.as_ref().unwrap().should_compact());
    // A durable permit proves the initial request's final evidence was recorded normally.
    // 可靠许可证明初始请求的最终证据已正常记录。
    let PreparedPhysicalApi::SessionV1 { core_start, .. } = sink
        .prepare_request(&state, facts.clone(), initial)
        .await
        .unwrap()
    else {
        panic!("initial request must receive a durable permit");
    };
    handle
        .mark_api_transport_observed_durable(core_start.api_call_id, 11)
        .await
        .unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    core_start.api_call_id,
                    ApiCompletion {
                        request: request.clone(),
                        usage: UsageSnapshot {
                            input_tokens: Some(449277),
                            uncached_input_tokens: Some(4),
                            cache_read_tokens: Some(0),
                            cache_write_tokens: Some(449273),
                            output_tokens: Some(18628),
                            ..Default::default()
                        },
                        status: ApiCallStatus::Failed,
                        end_reason: Some(ApiEndReason::NetworkError),
                        error: Some(RedactedApiError {
                            category: ApiErrorCategory::Network,
                            error_code: None,
                            message: "fixture terminal failure".into(),
                        }),
                        continuation_eligibility: ContinuationEligibility::Allowed,
                        timing: Default::default(),
                        provider_source: None,
                        completed_at_ms: 12,
                    },
                )
            },
        )
        .unwrap();
    // Cold reload removes all process-local latest-turn usage; only the durable anchor remains.
    // 冷重载移除所有进程内本回合用量，仅保留可靠锚点。
    let locator = handle.storage_locator().unwrap();
    // The reloaded document is the exact persisted frontier, not a handcrafted anchor.
    // 重载文档是精确持久前沿，而非手工构造锚点。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
    // This separate owner models the new process without mutating the earlier one.
    // 此独立所有者模拟新进程，不修改之前的所有者。
    let cold = SessionHandle::new(handle.session_id(), locator, loaded.document);
    // The admitted action must remain unconsumed until a business request receives a real permit.
    // 获准动作必须在业务请求收到真实许可前保持未消费。
    let action = ClientActionId::try_new("continue-after-budget-reopen").unwrap();
    cold.continue_turn(ContinuationRequest::new(action.clone(), "", Vec::new()), 13)
        .unwrap();
    // Use a fresh registry just as a restarted service does.
    // 与重启服务一样使用全新注册表。
    let service =
        crate::tests::test_support::test_service_state_with_parent_session(cold.session_id());
    service
        .session_runtime_registry
        .register(cold.clone())
        .unwrap();
    // The restarted state now references only the reloaded owner.
    // 重启状态现在仅引用重载所有者。
    let state = Arc::new(std::sync::Mutex::new(service));
    // Continue carries its original action through the soft-budget refusal.
    // Continue 跨软预算拒绝保留其原始动作。
    let continued = ProviderV1RequestFacts {
        purpose: ApiPurposeKind::Continue,
        continuation_action_id: Some(action),
        prepared_at_ms: 14,
        ..facts.clone()
    };
    // Record durable starts before entering the final V1 preparation boundary.
    // 进入最终 V1 准备边界前记录可靠起始次数。
    let before = starts(&cold);
    sink = ProviderV1StreamSink::new(
        cold.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    assert!(matches!(
        sink.prepare_request(&state, continued.clone(), budget_candidate(&cold, true))
            .await
            .unwrap(),
        PreparedPhysicalApi::ContextCompactionRequired { .. }
    ));
    assert_eq!(starts(&cold), before);
    assert!(sink.active.is_none());
    assert!(
        cold.read(|document| document.active_api().is_none())
            .unwrap()
    );

    // Auxiliary compaction uses the same soft threshold but must never recursively request compaction.
    // 辅助压缩使用相同软阈值，但绝不能递归请求压缩。
    let compaction_facts = ProviderV1RequestFacts {
        purpose: ApiPurposeKind::Compaction,
        prepared_at_ms: 15,
        ..facts
    };
    // The auxiliary permit consumes no Continue action and retains the same hard-window checks.
    // 辅助许可不消费 Continue 动作，且保留同一硬窗口检查。
    let PreparedPhysicalApi::SessionV1 {
        core_start: summary_start,
        ..
    } = sink
        .prepare_request(&state, compaction_facts, budget_candidate(&cold, false))
        .await
        .unwrap()
    else {
        panic!("auxiliary compaction must not recursively request host compaction");
    };
    // A completed summary follows the same domain and storage steps as the compaction coordinator.
    // 完整摘要遵循与压缩协调器相同的领域及存储步骤。
    let summary_seq = cold
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                document.append_assistant_text(
                    summary_start.api_call_id,
                    "summary",
                    AssistantSemanticStatus::Completed,
                    16,
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    cold.mutate_durable(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::ApiCompleted,
        |document| {
            document.complete_api_group(
                summary_start.api_call_id,
                ApiCompletion {
                    request,
                    usage: Default::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: Default::default(),
                    provider_source: None,
                    completed_at_ms: 17,
                },
            )
        },
    )
    .unwrap();
    cold.record_compaction(
        CompactionCommitInput::new(
            summary_start.api_call_id,
            summary_seq,
            summary_seq,
            "summary",
            super::super::memory_acceptance::host_guided_locked_baseline(),
        ),
        18,
    )
    .unwrap();
    sink = ProviderV1StreamSink::new(
        cold.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    // The original action is now consumed once by a new API built from the compacted generation.
    // 原始动作现在由基于压缩代际构建的新 API 恰好消费一次。
    let PreparedPhysicalApi::SessionV1 {
        core_start: resumed,
        context_budget,
        ..
    } = sink
        .prepare_request(
            &state,
            ProviderV1RequestFacts {
                prepared_at_ms: 19,
                ..continued
            },
            budget_candidate(&cold, true),
        )
        .await
        .unwrap()
    else {
        panic!("compacted Continue must receive a fresh durable permit");
    };
    assert_ne!(resumed.api_call_id, core_start.api_call_id);
    assert_ne!(resumed.api_call_id, summary_start.api_call_id);
    assert!(!context_budget.unwrap().should_compact());
    assert_eq!(starts(&cold), before + 2);
    // Disk reload must retain the final fresh identity and the configured compaction boundary result.
    // 磁盘重载必须保留最终新身份及配置压缩边界的结果。
    let persisted = vulcan_workspace_store::workspace_store::load_session_v1_full(
        &cold.storage_locator().unwrap(),
    )
    .unwrap();
    assert_eq!(
        persisted.document.active_api().unwrap().api_call_id,
        resumed.api_call_id
    );
}
