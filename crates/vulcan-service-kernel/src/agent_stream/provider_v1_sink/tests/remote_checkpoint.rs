//! Checkpoint projection through real durable owners, cold loading, and native encoding.
//! 通过真实可靠所有者、冷加载和原生编码验证检查点投影。

use super::super::remote_checkpoint as projection;
use super::*;
use vulcan_agent_runtime::agent_runtime::events::{
    ExecutionStreamEvent, test_provider_request_durability_barrier,
};
use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::{
    ProviderRequestTelemetryEvent, ProviderRequestTelemetryEventKind,
};
use vulcan_agent_runtime::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts;
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport,
    PreparedCompactionSource,
};
use vulcan_session_core::v1::context::RemoteResponsesCheckpoint;
use vulcan_session_core::v1::document::CompactionCommitInput;

/// Returns canonical opaque output and a declared Codex route without touching external credentials.
/// 返回规范不透明输出及声明的 Codex 路由，不访问外部凭据。
fn checkpoint() -> RemoteResponsesCheckpoint {
    RemoteResponsesCheckpoint {
        provider_id: "provider-1".into(),
        connection_id: "connection-1".into(),
        route_fingerprint: "https://provider.invalid/v1/responses".into(),
        protocol_profile_id: "openai-codex-responses.v1".into(),
        model_id: "model-1".into(),
        output_items: vec![
            json!({"type":"compaction", "encrypted_content":"opaque-checkpoint-bytes"}),
        ],
        response_id: Some("remote-result-id".into()),
    }
}

/// Returns the exact route expected by the checkpoint; no optional reasoning replay contract is required.
/// 返回检查点所需的精确路由；不依赖可选推理重放契约。
fn route() -> V1ReplayRouteIdentity {
    // Reuse the explicitly declared fixture identity for the matching-route case.
    // 在匹配路由用例中复用明确声明的夹具身份。
    let checkpoint = checkpoint();
    V1ReplayRouteIdentity {
        provider_id: checkpoint.provider_id,
        connection_id: checkpoint.connection_id,
        route_fingerprint: checkpoint.route_fingerprint,
        protocol_profile_id: checkpoint.protocol_profile_id,
        model_id: checkpoint.model_id,
        reasoning_replay_contract: None,
    }
}

/// Builds a runtime report from canonical `native` bytes for shared completion verification.
/// 根据规范`native`字节构建运行时报告，以验证共用完成校验。
/// Returns a remote result with diagnostic text that must never become assistant semantics.
/// 返回带诊断文字的远端结果；该文字绝不能变成助手语义。
fn remote_execution(
    native: &RemoteResponsesCheckpoint,
) -> vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecution {
    use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
        PromptCompactionExecution, PromptCompactionExecutionProfile,
    };
    use vulcan_context_compaction::{
        CompactionPlan, CompactionResult, CompactionTailSelectionReport, ProviderContextRoute,
    };
    // This report fixture does not select physical source; the production source tests cover that boundary separately.
    // 此报告夹具不选择物理来源；生产来源测试单独覆盖该边界。
    let policy = vulcan_context::ContextCompactionPolicy::default();
    let plan = CompactionPlan {
        source_messages: Vec::new(),
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: "remote-receipt-test".into(),
        created_at: "8".into(),
        policy,
        tail_selection: CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count: 0,
            retained_message_ids: Vec::new(),
            retained_token_estimate: 0,
            previewed_tool_output_count: 0,
            omitted_tool_output_token_estimate: 0,
            newest_segment_exceeds_nominal_budget: false,
        },
    };
    PromptCompactionExecution {
        compaction_result: CompactionResult::from_remote_responses_with_metadata(
            &plan,
            ProviderContextRoute {
                provider_id: native.provider_id.clone(),
                connection_id: native.connection_id.clone(),
                route_fingerprint: native.route_fingerprint.clone(),
                protocol_profile_id: native.protocol_profile_id.clone(),
                model_id: native.model_id.clone(),
            },
            native.output_items.clone(),
            None,
            native.response_id.clone(),
        )
        .unwrap(),
        fallback_used: false,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        provider_id: Some(native.provider_id.clone()),
        model_id: Some(native.model_id.clone()),
        execution_profile: PromptCompactionExecutionProfile::RemoteResponses,
        input_prune_report: None,
    }
}

/// Native completion binds route, terminal and persisted node without creating an assistant event.
/// 原生完成绑定路由、终态与持久节点，不创建助手事件。
#[tokio::test]
async fn remote_checkpoint_terminal_verification_and_shared_commit_survive_cold_load() {
    // This fixture owns real files and a real API, while the frozen route is explicitly seeded below.
    // 此夹具持有真实文件及真实 API；冻结路由在下方明确播种。
    let (_directory, handle) = published_stored_open_turn();
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    let mut facts = request_facts();
    facts.purpose = ApiPurposeKind::Compaction;
    facts.request.profile = Some(checkpoint().protocol_profile_id);
    prepare_sink_request_with_facts(&handle, &mut sink, facts.clone());
    let mut terminal = terminal_facts(0);
    terminal.request_facts = facts;
    terminal.provider_source = None;
    // A declared checkpoint cannot convert an ordinary/local admission into a remote one.
    // 已声明检查点不能将普通或本地准入转换为远端准入。
    assert!(
        sink.accept_remote_compaction_terminal(terminal.clone(), checkpoint())
            .await
            .is_err()
    );
    sink.active.as_mut().unwrap().remote_compaction_route = Some(route());
    assert!(sink.accept_terminal(terminal.clone()).await.is_err());
    // Every route dimension is mandatory, and rejected output leaves the same API available for its real result.
    // 每个路由维度都强制校验，被拒输出仍为真实结果保留同一 API。
    for dimension in 0..5 {
        let mut changed = checkpoint();
        match dimension {
            0 => changed.provider_id.push_str("-changed"),
            1 => changed.connection_id.push_str("-changed"),
            2 => changed.route_fingerprint.push_str("/changed"),
            3 => changed.protocol_profile_id.push_str("-changed"),
            _ => changed.model_id.push_str("-changed"),
        }
        assert!(
            sink.accept_remote_compaction_terminal(terminal.clone(), changed)
                .await
                .is_err()
        );
        assert!(sink.api_terminal_completion.is_none());
        assert!(sink.active.is_some());
    }
    assert!(matches!(
        sink.accept_remote_compaction_terminal(terminal, checkpoint())
            .await
            .unwrap(),
        ProviderV1TerminalOutcome::Completed
    ));
    let mut durability = sink.take_finished_durability();
    assert!(durability.completed_assistant_text.is_none());
    let execution = remote_execution(&checkpoint());
    let retained = Seq::try_new(1).unwrap();
    let commit = crate::context_compaction::verified_compaction_commit_input(
        &execution,
        &durability,
        retained,
    )
    .unwrap();
    assert_eq!(
        commit.occurred_at_seq,
        durability.api_terminal.as_ref().unwrap().value.stats_seq
    );
    // Mutated native bytes and a late mismatching API receipt cannot pass the shared root/child verifier.
    // 改写原生字节及迟到失配 API 回执无法通过主子共用校验器。
    let mut changed = execution.clone();
    changed.compaction_result.provider_context_items[0]["encrypted_content"] = json!("other");
    assert!(
        crate::context_compaction::verified_compaction_commit_input(
            &changed,
            &durability,
            retained
        )
        .is_err()
    );
    let api = durability
        .completed_remote_compaction
        .as_ref()
        .unwrap()
        .api_call_id;
    durability
        .completed_remote_compaction
        .as_mut()
        .unwrap()
        .api_call_id = ApiCallId::try_new(999).unwrap();
    assert!(
        crate::context_compaction::verified_compaction_commit_input(
            &execution,
            &durability,
            retained
        )
        .is_err()
    );
    durability
        .completed_remote_compaction
        .as_mut()
        .unwrap()
        .api_call_id = api;
    // Result kinds cannot be interchanged even if the display body looks like a valid local summary.
    // 即使展示正文看似有效本地摘要，也不能互换结果种类。
    changed = execution.clone();
    changed.execution_profile = vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecutionProfile::PrunedMinimal;
    assert!(
        crate::context_compaction::verified_compaction_commit_input(
            &changed,
            &durability,
            retained
        )
        .is_err()
    );
    durability.completed_assistant_text = Some(ProviderV1CompletedAssistantText {
        api_call_id: api,
        seq: retained,
        text: "fake summary".into(),
    });
    assert!(
        crate::context_compaction::verified_compaction_commit_input(
            &execution,
            &durability,
            retained
        )
        .is_err()
    );
    durability.completed_assistant_text = None;
    handle.record_compaction(commit, 9).unwrap();
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(
        &handle.storage_locator().unwrap(),
    )
    .unwrap();
    assert!(!loaded.document.events().iter().any(|event| matches!(
        event.event,
        EventBody::AssistantText(_) | EventBody::AssistantReasoning(_)
    )));
    let cold = SessionHandle::new(
        handle.session_id(),
        handle.storage_locator().unwrap(),
        loaded.document,
    );
    let projection = cold.capture_controller_projection_snapshot().unwrap();
    assert!(
        matches!(&projection.items[0], ProjectedItem::RemoteCompactionCheckpoint { checkpoint: native, .. } if **native == checkpoint())
    );
    assert!(
        sink.take_finished_durability()
            .completed_remote_compaction
            .is_none()
    );
}

/// A native success cannot silently absorb assistant, reasoning, tool or failure facts.
/// 原生成功不能静默吸收助手、推理、工具或失败事实。
#[tokio::test]
async fn remote_checkpoint_terminal_rejects_mixed_semantics_and_failed_success() {
    // Use separate durable APIs so each corruption case has independent semantic ownership.
    // 使用独立可靠 API，使每个污染用例都有独立语义归属。
    for corruption in 0..5 {
        let (_directory, handle) = published_stored_open_turn();
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        let mut facts = request_facts();
        facts.purpose = ApiPurposeKind::Compaction;
        facts.request.profile = Some(checkpoint().protocol_profile_id);
        prepare_sink_request_with_facts(&handle, &mut sink, facts.clone());
        sink.active.as_mut().unwrap().remote_compaction_route = Some(route());
        let mut terminal = terminal_facts(0);
        terminal.request_facts = facts;
        terminal.provider_source = None;
        match corruption {
            0 => {
                sink.append_assistant_delta("unexpected assistant", 4)
                    .unwrap();
            }
            1 => {
                sink.append_reasoning_delta("unexpected reasoning", 4)
                    .unwrap();
            }
            2 => terminal.tool_call_count = 1,
            3 => terminal.status = ApiCallStatus::Failed,
            _ => {
                terminal.status = ApiCallStatus::Failed;
                terminal.tool_call_count = 1;
                assert!(sink.accept_terminal(terminal.clone()).await.is_err());
            }
        }
        assert!(
            sink.accept_remote_compaction_terminal(terminal, checkpoint())
                .await
                .is_err()
        );
        assert!(sink.api_terminal_completion.is_none());
        assert!(sink.completed_remote_compaction.is_none());
    }
}

/// Persists a genuine successful compaction API and opaque node; returns its directory guard and owner.
/// 持久化真实成功压缩 API 及不透明节点；返回目录守卫及所有者。
fn stored_checkpoint() -> (tempfile::TempDir, Arc<SessionHandle>) {
    // Isolated files are owned by this fixture and retained by the returned guard.
    // 隔离文件由此夹具持有，并通过返回的守卫保留。
    let (directory, handle) = published_stored_open_turn();
    // Freeze the exact compaction purpose and native protocol before admission.
    // 在准入前冻结精确压缩用途和原生协议。
    let mut facts = request_facts();
    facts.purpose = ApiPurposeKind::Compaction;
    facts.request.profile = Some(checkpoint().protocol_profile_id);
    facts.prepared_at_ms = 10;
    // The terminal must repeat the admitted request identity exactly.
    // 终态必须精确复用已准入的请求身份。
    let request = facts.request.clone();
    // The shared test helper commits a real API start through this owner.
    // 共用测试辅助函数通过此所有者提交真实 API 起始。
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request_with_facts(&handle, &mut sink, facts);
    // Use the allocated identity rather than a guessed local API number.
    // 使用实际分配身份，不猜测本地 API 编号。
    let api = sink.active.as_ref().unwrap().api_call_id;
    // Persist completion without manufacturing assistant or tool semantics.
    // 持久化完成状态，不伪造助手或工具语义。
    let terminal = handle
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
                        completed_at_ms: 11,
                    },
                )
            },
        )
        .unwrap()
        .value
        .stats_seq;
    handle
        .record_compaction(
            CompactionCommitInput::remote_responses(
                api,
                terminal,
                Seq::try_new(1).unwrap(),
                checkpoint(),
                LockedBaselineProjection::empty(),
            ),
            12,
        )
        .unwrap();
    (directory, handle)
}

/// Native checkpoint bytes survive cold projection and real Codex encoding without synthetic text or continuation ids.
/// 原生检查点字节经过冷投影与真实 Codex 编码仍被保留，且没有合成文本或延续 ID。
#[test]
fn remote_checkpoint_cold_projection_and_native_encoding_preserve_payload() {
    // Keep the real storage directory alive throughout the cold-load assertions.
    // 在冷加载断言期间保持真实存储目录存活。
    let (_directory, handle) = stored_checkpoint();
    // Warm projection is the comparison baseline for the newly loaded owner.
    // 热投影是新加载所有者的比较基线。
    let warm = handle.capture_controller_projection_snapshot().unwrap();
    // Resolve the exact registered storage identity without searching alternative paths.
    // 解析精确登记存储身份，不搜索候选路径。
    let locator = handle.storage_locator().unwrap();
    // The normal loader validates node and event files together.
    // 正常加载器共同验证节点及事件文件。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
    assert!(!loaded.document.events().iter().any(|event| matches!(
        &event.event,
        EventBody::AssistantText(_) | EventBody::AssistantReasoning(_)
    )));
    // A fresh owner cannot reuse the warm owner's cached projection.
    // 全新所有者不能复用热所有者的投影缓存。
    let cold = SessionHandle::new(handle.session_id(), locator, loaded.document);
    // Capture through the same immutable publication boundary as production.
    // 通过与生产相同的不可变发布边界捕获。
    let snapshot = cold.capture_controller_projection_snapshot().unwrap();
    assert_eq!(snapshot.sourced_items, warm.sourced_items);
    // Opaque checkpoints never acquire the local summary's synthetic continuation control.
    // 不透明检查点绝不能获得本地摘要的合成续接控制消息。
    let local_control_id = format!(
        "v1-compaction-continuation-{}",
        snapshot.current_compaction_id.unwrap().value()
    );
    assert!(!snapshot.items.iter().any(|item| matches!(
        item, ProjectedItem::SystemText { text, .. } if text.contains(&local_control_id)
    )));
    assert!(
        matches!(&snapshot.items[0], ProjectedItem::RemoteCompactionCheckpoint { checkpoint: actual, .. } if **actual == checkpoint())
    );
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|item| matches!(item, ProjectedItem::RemoteCompactionCheckpoint { .. }))
            .count(),
        1
    );
    // The controller carries native state with an empty semantic body.
    // 控制器以空语义正文携带原生状态。
    let state = provider_projection_controller_state(&snapshot).unwrap();
    assert_eq!(
        serde_json::to_value(&state.messages[0].payload).unwrap(),
        serde_json::to_value(projection::controller_payload(&checkpoint()).unwrap()).unwrap()
    );
    // Compact response identifiers must not become provider continuation cursors.
    // 压缩响应标识不能成为供应商延续游标。
    let core = build_core_conversation_snapshot(&state).unwrap();
    assert!(core.continuation.last_provider_message_id.is_none());
    // The same physical projection helper used by the production permit feeds the real encoder.
    // 生产许可使用的同一物理投影辅助函数为真实编码器提供输入。
    let native = projection::physical_message(&checkpoint(), Some(&route())).unwrap();
    // Follow the native checkpoint with one ordinary retained user message.
    // 在原生检查点后放置一条普通保留用户消息。
    let request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: "model-1".into(),
        cache_scope_id: Some(handle.session_id().to_string()),
        continuation: None,
        messages: vec![
            native,
            text_ai_message(MessageRole::User, "retained user text".into()),
        ],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: Default::default(),
        reasoning_effort: None,
        reasoning_output: Default::default(),
        model_parameters: Default::default(),
        resources: Vec::new(),
        output_token_limit: None,
    };
    // Exercise the real encoder instead of inferring wire behavior from controller JSON.
    // 执行真实编码器，不从控制器 JSON 推断线上行为。
    let encoded = vulcan_llm_core::encode_codex_responses_request(&request).unwrap();
    // The native item must stay first and occur exactly once.
    // 原生项必须保持首位且只出现一次。
    let items = encoded.body["input"].as_array().unwrap();
    assert_eq!(items[0], checkpoint().output_items[0]);
    assert_eq!(items.len(), 2);
    assert_eq!(items[1]["role"], "user");
    assert!(!encoded.body.to_string().contains("remote-result-id"));
}

/// Every route field is mandatory even when the target declares no reasoning replay obligation.
/// 即使目标未声明推理重放义务，每个路由字段也必须匹配。
#[test]
fn remote_checkpoint_rejects_missing_or_foreign_route_without_plaintext_fallback() {
    assert!(projection::physical_message(&checkpoint(), None).is_err());
    // Independently alter each mandatory route dimension.
    // 独立改变每个强制路由维度。
    for field in 0..5 {
        // Start each mismatch from the unchanged valid identity.
        // 每个失配用例从未修改的有效身份开始。
        let mut foreign = route();
        match field {
            0 => foreign.provider_id = "foreign".into(),
            1 => foreign.connection_id = "foreign".into(),
            2 => foreign.route_fingerprint = "https://foreign.invalid/responses".into(),
            3 => foreign.protocol_profile_id = "openai-responses.v1".into(),
            4 => foreign.model_id = "foreign".into(),
            _ => unreachable!(),
        }
        // Diagnostics identify the mismatch without exposing opaque content.
        // 诊断标识失配，但不暴露不透明内容。
        let error = projection::physical_message(&checkpoint(), Some(&foreign))
            .unwrap_err()
            .to_string();
        assert!(!error.contains("opaque-checkpoint-bytes"));
        assert!(error.contains("route mismatch"));
    }
}

/// Recompaction restores native authority in both full and pruned selections and rejects missing or altered carriers.
/// 再次压缩在完整与剪枝选择中恢复原生权威，并拒绝缺失或修改的载体。
#[test]
fn remote_checkpoint_recompaction_source_preserves_authority_and_rejects_loss() {
    // Use the same real durable checkpoint as the next-request projection test.
    // 使用与下一请求投影测试相同的真实可靠检查点。
    let (_directory, handle) = stored_checkpoint();
    // Planning may prune expired host rows but must retain native checkpoint identity.
    // 规划可以裁剪失效宿主行，但必须保留原生检查点身份。
    let snapshot = handle
        .capture_context_compaction_planning_snapshot()
        .unwrap();
    // Convert only typed sources; diagnostic display records are never authoritative.
    // 只转换类型化来源；诊断展示记录绝非权威。
    let rows = snapshot
        .sourced_items
        .iter()
        .filter_map(crate::context_compaction::compaction_message_from_v1_projection)
        .map(|result| result.unwrap().0)
        .collect::<Vec<_>>();
    // Recover opaque state from its unique persisted node identity.
    // 根据唯一持久节点身份恢复不透明状态。
    let state = compaction_state_from_projection(&snapshot, &rows).unwrap();
    assert_eq!(
        serde_json::to_value(&state.messages[0].payload).unwrap(),
        serde_json::to_value(projection::controller_payload(&checkpoint()).unwrap()).unwrap()
    );
    // Freeze the exact planning source before selecting any summary attempt.
    // 在选择任何摘要尝试前冻结精确规划来源。
    let mut source =
        super::super::compaction_source::CompactionSourceWindow::capture(snapshot.clone(), &state)
            .unwrap();
    // Full fidelity selects canonical rows while the host retains their native authority.
    // 完整保真选择规范行，同时由宿主保留其原生权威。
    let selected = PreparedCompactionSource {
        source_messages: vulcan_context::conversation_state_to_compaction_source_messages(&state)
            .unwrap(),
        prune_report: CompactionInputPruneReport::no_pruning(
            CompactionInputBudget {
                target_input_budget_tokens: 10000,
                safety_reserve_tokens: 0,
            },
            0,
        ),
    };
    source
        .select(&selected, CompactionInputPruneMode::None)
        .unwrap();
    // Minimal attempts use original planning row kinds and restore native authority before physical preparation.
    // 最小尝试使用原规划行类型，在物理准备前恢复原生权威。
    let minimal = PreparedCompactionSource {
        source_messages: rows.clone(),
        prune_report: selected.prune_report.clone(),
    };
    source
        .select(&minimal, CompactionInputPruneMode::RetryAfterFailure)
        .unwrap();
    // Dropped or substituted native carriers remain forbidden after a valid selection.
    // 有效选择之后，丢弃或替换原生载体仍被禁止。
    let mut missing = minimal.clone();
    missing.source_messages.remove(0);
    assert!(
        source
            .select(&missing, CompactionInputPruneMode::RetryAfterFailure)
            .is_err()
    );
    // Reject text replacement through the real source-selection API as well as the planning helper.
    // 除规划辅助函数外，真实来源选择 API 也拒绝文本替换。
    let mut changed = minimal;
    changed.source_messages[0].body = "fake checkpoint diagnostic".into();
    assert!(
        source
            .select(&changed, CompactionInputPruneMode::RetryAfterFailure)
            .is_err()
    );
    // A text substitution must fail instead of turning diagnostics into checkpoint semantics.
    // 文本替换必须失败，不能将诊断文案变成检查点语义。
    let mut altered = rows.clone();
    altered[0].body = "fake checkpoint diagnostic".into();
    assert!(compaction_state_from_projection(&snapshot, &altered).is_err());
    assert!(compaction_state_from_projection(&snapshot, &rows[1..]).is_err());
}

/// Minimal transport validates native bytes and rejects missing frozen route after canonical restoration succeeds.
/// 最小传输在规范恢复成功后校验原生字节，并拒绝缺失冻结路由。
#[test]
fn remote_checkpoint_pruned_transport_requires_exact_native_payload_and_route() {
    // The normal physical builder supplies the expected ordered native item list.
    // 正常物理构建器提供预期有序原生项列表。
    let native = projection::physical_message(&checkpoint(), Some(&route())).unwrap();
    projection::validate_physical_message(&checkpoint(), &native, Some(&route())).unwrap();
    for case in 0..3 {
        // Every corruption preserves route identity, isolating native payload validation.
        // 每项损坏保留路由身份，单独验证原生载荷检查。
        let mut altered = native.clone();
        match case {
            0 => altered.provider_context_items.clear(),
            1 => altered.provider_context_items[0]["encrypted_content"] = json!("replacement"),
            2 => altered.content.push(ContentItem::Text {
                text: "fake diagnostic".into(),
            }),
            _ => unreachable!(),
        }
        assert!(
            projection::validate_physical_message(&checkpoint(), &altered, Some(&route())).is_err()
        );
    }
    // Real published node ownership binds the minimal selection independently of the wire candidate.
    // 真实已发布节点归属独立于出站候选绑定最小选择。
    let (_directory, handle) = stored_checkpoint();
    // The source owner uses the actual planning snapshot and canonical conversion.
    // 来源所有者使用实际规划快照与规范转换。
    let snapshot = handle
        .capture_context_compaction_planning_snapshot()
        .unwrap();
    // Preserve planning record kinds for the minimal source selection.
    // 为最小来源选择保留规划记录类型。
    let rows = snapshot
        .sourced_items
        .iter()
        .filter_map(crate::context_compaction::compaction_message_from_v1_projection)
        .map(|result| result.unwrap().0)
        .collect::<Vec<_>>();
    // Recover native authority from the typed node, never from forged row metadata.
    // 从类型化节点恢复原生权威，绝不从伪造行元数据恢复。
    let state = compaction_state_from_projection(&snapshot, &rows).unwrap();
    // Select through the production compaction owner before testing its transport extraction.
    // 在测试传输提取前，通过生产压缩所有者选择来源。
    let mut source =
        super::super::compaction_source::CompactionSourceWindow::capture(snapshot, &state).unwrap();
    source
        .select(
            &PreparedCompactionSource {
                source_messages: rows.clone(),
                prune_report: CompactionInputPruneReport::no_pruning(
                    CompactionInputBudget {
                        target_input_budget_tokens: 10000,
                        safety_reserve_tokens: 0,
                    },
                    0,
                ),
            },
            CompactionInputPruneMode::RetryAfterFailure,
        )
        .unwrap();
    // Reuse the existing credential-free transport fixture without manufacturing a frozen route.
    // 复用既有无凭据传输夹具，不伪造冻结路由。
    let (_base_directory, base_handle) = published_stored_open_turn();
    // This fixture supplies ordinary budget/configuration fields; the native candidate still lacks route authority.
    // 此夹具提供普通预算与配置字段；原生候选仍缺少路由权威。
    let mut candidate = super::super::memory_acceptance::rebuild_memory_acceptance_request(
        base_handle
            .capture_controller_projection_snapshot()
            .unwrap(),
        Default::default(),
    )
    .unwrap();
    // Use the same shared restore as runtime and host, ensuring rejection occurs at the physical route check.
    // 使用与运行时和宿主相同的共用恢复，确保拒绝发生于物理路由检查。
    let restored = vulcan_context_compaction::NativeCompactionSources::capture(&state)
        .unwrap()
        .restore(&rows)
        .unwrap();
    candidate.controller_snapshot = build_core_conversation_snapshot(&restored).unwrap();
    candidate.exact_request.messages = vec![native];
    candidate.source_message_positions = BTreeMap::from([(restored.messages[0].id.clone(), 0)]);
    assert!(
        source
            .pruned_transport(&candidate)
            .unwrap_err()
            .to_string()
            .contains("requires a frozen execution route")
    );
}

/// Remote selection uses exact projected prefix positions even when the checkpoint seq follows retained history.
/// 即使检查点序号晚于保留历史，远端选择仍使用精确投影前缀位置。
#[test]
fn remote_checkpoint_native_source_requires_exact_prefix_and_keeps_attempt_modes_exclusive() {
    // Publish a real opaque node whose terminal seq is newer than the retained root input.
    // 发布真实不透明节点，其终态序号比保留根输入更新。
    let (_directory, handle) = stored_checkpoint();
    // Preserve the complete authoritative window for source validation and later local retry.
    // 保留完整权威窗口，用于来源校验及后续本地重试。
    let snapshot = handle
        .capture_context_compaction_planning_snapshot()
        .unwrap();
    assert!(snapshot.sourced_items[0].source_to_seq > snapshot.sourced_items[1].source_to_seq);
    // Read exact planner rows beside their original positions.
    // 在原位置旁读取精确规划行。
    let rows = snapshot
        .sourced_items
        .iter()
        .filter_map(crate::context_compaction::compaction_message_from_v1_projection)
        .map(|result| result.unwrap().0)
        .collect::<Vec<_>>();
    // The planning authority contains the complete opaque payload.
    // 规划权威包含完整不透明载荷。
    let state = compaction_state_from_projection(&snapshot, &rows).unwrap();
    // A source without an admitted attempt cannot prepare any request.
    // 未准入尝试的来源不能准备任何请求。
    let mut source =
        super::super::compaction_source::CompactionSourceWindow::capture(snapshot.clone(), &state)
            .unwrap();
    // Only the compaction purpose may consume the frozen source.
    // 只有压缩用途可以消费冻结来源。
    let mut facts = request_facts();
    facts.purpose = ApiPurposeKind::Compaction;
    assert!(source.bound_snapshot(snapshot.clone(), &facts).is_err());
    assert!(source.select_remote(&[]).is_err());
    source.select_remote(&rows[..1]).unwrap();
    assert!(source.is_remote());
    // Invalid replacement must leave the previously admitted prefix unchanged.
    // 无效替换必须保持此前准入前缀不变。
    for case in 0..4 {
        // Independently corrupt prefix membership, content, order, or uniqueness.
        // 独立破坏前缀成员、内容、顺序或唯一性。
        let mut altered = rows.clone();
        match case {
            0 => {
                altered.remove(0);
            }
            1 => altered[0].body = "modified".into(),
            2 => altered.swap(0, 1),
            3 => altered.insert(1, altered[0].clone()),
            _ => unreachable!(),
        }
        assert!(source.select_remote(&altered).is_err());
        // The current durable frontier stays intact while only model-visible source items are shortened.
        // 当前可靠前沿保持完整，只有模型可见来源项被缩短。
        let selected = source.bound_snapshot(snapshot.clone(), &facts).unwrap();
        assert_eq!(selected.sourced_items, snapshot.sourced_items[..1]);
        assert_eq!(selected.items, snapshot.items[..1]);
        assert_eq!(selected.through_seq, snapshot.through_seq);
    }
    // A later local full retry explicitly replaces remote selection and restores its own full-source semantics.
    // 后续本地完整重试明确替换远端选择，并恢复自身完整来源语义。
    source
        .select(
            &PreparedCompactionSource {
                source_messages: vulcan_context::conversation_state_to_compaction_source_messages(
                    &state,
                )
                .unwrap(),
                prune_report: CompactionInputPruneReport::no_pruning(
                    CompactionInputBudget {
                        target_input_budget_tokens: 10000,
                        safety_reserve_tokens: 0,
                    },
                    0,
                ),
            },
            CompactionInputPruneMode::None,
        )
        .unwrap();
    assert!(!source.is_remote());
    assert_eq!(
        source
            .bound_snapshot(snapshot.clone(), &facts)
            .unwrap()
            .sourced_items,
        snapshot.sourced_items
    );
}

/// Owns one real durable session, its registered service state and its isolated storage directory.
/// 持有真实可靠会话、其已注册服务状态及隔离存储目录。
fn remote_attempt_session() -> (
    tempfile::TempDir,
    Arc<SessionHandle>,
    crate::ServiceBridgeState,
) {
    let (directory, handle) = published_stored_open_turn();
    // The service registry is the same owner a real compaction stream resolves before dispatch.
    // 服务注册表就是真实压缩流在分派前解析的同一所有者。
    let service_state =
        crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
    service_state
        .session_runtime_registry
        .register(Arc::clone(&handle))
        .expect("register the durable session owner");
    (directory, handle, service_state)
}

/// Returns the exact request facts of one physical remote compaction attempt.
/// 返回一次真实远端压缩尝试的精确请求事实。
fn compaction_facts(provider_call_id: &str) -> ProviderV1RequestFacts {
    let mut facts = request_facts();
    facts.provider_call_id = provider_call_id.to_owned();
    facts.purpose = ApiPurposeKind::Compaction;
    facts.request.profile = Some(checkpoint().protocol_profile_id);
    facts
}

/// Starts one real durable compact attempt and freezes its native route on the sink.
/// 启动一次真实可靠压缩尝试，并在接收器上冻结其原生路由。
fn start_remote_attempt(
    handle: &SessionHandle,
    sink: &mut ProviderV1StreamSink,
    facts: ProviderV1RequestFacts,
) -> ApiCallId {
    prepare_sink_request_with_facts(handle, sink, facts);
    let active = sink
        .active
        .as_mut()
        .expect("an admitted attempt must own the sink");
    // The admitted route is the fixture's own frozen Codex identity.
    // 准入路由使用夹具自身的冻结 Codex 身份。
    active.remote_compaction_route = Some(route());
    active.api_call_id
}

/// Returns one rejected physical attempt terminal for `facts`.
/// 返回针对`facts`的一份被拒物理尝试终态。
fn rejected_attempt_terminal(
    facts: ProviderV1RequestFacts,
    http_status: u16,
) -> ProviderV1ApiTerminalFacts {
    let mut terminal = terminal_facts(0);
    terminal.request_facts = facts;
    terminal.status = ApiCallStatus::Failed;
    // An upstream auth rejection is a provider error with an auth category, exactly as
    // `provider_end_reason` classifies it; any other pairing is rejected by the durable document.
    // 上游鉴权拒绝属于 provider error 且分类为 auth，与`provider_end_reason`的分类一致；
    // 其它组合会被可靠文档拒绝。
    terminal.end_reason = Some(ApiEndReason::ProviderError);
    terminal.provider_source = None;
    terminal.error = Some(RedactedApiError {
        category: ApiErrorCategory::Auth,
        error_code: Some(format!("http_{http_status}")),
        message: format!("the upstream rejected this physical attempt with HTTP {http_status}"),
    });
    terminal
}

/// Returns one successful native compact terminal for `facts`; it carries no assistant semantics.
/// 返回针对`facts`的一份成功原生压缩终态；它不携带助手语义。
fn native_attempt_terminal(facts: ProviderV1RequestFacts) -> ProviderV1ApiTerminalFacts {
    let mut terminal = terminal_facts(0);
    terminal.request_facts = facts;
    terminal.provider_source = None;
    terminal
}

/// Drives one real durability boundary through the production service consumer.
/// 通过生产服务消费方驱动一次真实可靠性边界。
/// Returns the host result and the exact receipt the waiting producer observed.
/// 返回宿主结果与等待中生产者观察到的精确回执。
async fn drive_terminal_boundary(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    handle: &SessionHandle,
    sink: &mut ProviderV1StreamSink,
    telemetry: &mut crate::ProviderTelemetryHookGuard,
    terminal: ProviderV1ApiTerminalFacts,
) -> (
    Result<Option<ExecutionStreamEvent>, String>,
    Result<(), String>,
) {
    let mut output_id = None;
    let mut raw = String::new();
    // completion is the same one-shot barrier a real Provider completion uses.
    // completion 与真实 Provider 完成使用同一一次性屏障。
    let (completion, waiter) = test_provider_request_durability_barrier();
    let host = crate::context_compaction::apply_context_compaction_stream_event(
        app,
        state,
        None,
        &mut output_id,
        &mut raw,
        sink,
        telemetry,
        ExecutionStreamEvent::ProviderRequestDurabilityBoundary {
            event: ProviderRequestTelemetryEvent {
                kind: ProviderRequestTelemetryEventKind::Completed,
                session_id: handle.session_id(),
                payload: json!({}),
                completed_usage: None,
                v1_lifecycle: Some(ProviderV1LifecycleFacts::ApiTerminal(terminal)),
            },
            completion,
        },
    )
    .await;
    let receipt = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .expect("the producer barrier must settle instead of hanging");
    (host, receipt)
}

/// Verifies a rejected compact attempt is durably settled before a new permit owns the retried result.
/// 验证被拒压缩尝试先被可靠结算，再由新许可持有重试结果。
#[tokio::test]
async fn remote_compaction_retry_settles_failed_permit_then_confirms_only_the_new_permit() {
    let (_directory, handle, service_state) = remote_attempt_session();
    let state = Arc::new(std::sync::Mutex::new(service_state));
    let app = crate::ServiceEventEmitter::new(Vec::new());
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    let mut telemetry =
        crate::ProviderTelemetryHookGuard::new_with_persistence(Arc::clone(&state), None);

    // firstAttempt is the physical request the upstream actually rejected.
    // firstAttempt 是被上游实际拒绝的物理请求。
    let first_facts = compaction_facts("remote-attempt-1");
    let first_api = start_remote_attempt(&handle, &mut sink, first_facts.clone());
    let (host, receipt) = drive_terminal_boundary(
        &app,
        &state,
        &handle,
        &mut sink,
        &mut telemetry,
        rejected_attempt_terminal(first_facts.clone(), 401),
    )
    .await;
    assert!(
        host.expect("the rejected attempt must settle durably")
            .is_none()
    );
    receipt.expect("the producer must observe its failed attempt settlement");
    // The failed attempt is durably closed, so its permit can no longer be reused.
    // 失败尝试已可靠闭合，因此其许可不可再次使用。
    let first_durability = sink.take_finished_durability();
    assert!(first_durability.api_terminal_failure_persisted);
    assert!(
        sink.active.is_none(),
        "a settled failed attempt must release its permit"
    );
    handle
        .read(|document| {
            assert!(document.active_api().is_none());
            let failed = document
                .events()
                .iter()
                .find_map(|row| match &row.event {
                    EventBody::ApiStats(stats) if stats.api_call_id == first_api => Some(stats),
                    _ => None,
                })
                .expect("the rejected attempt must persist its own statistics fact");
            assert_eq!(failed.status, ApiCallStatus::Failed);
        })
        .unwrap();

    // The retried attempt obtains a new independently identified physical permit.
    // 重试尝试获得一份新的、身份独立的物理许可。
    let second_facts = compaction_facts("remote-attempt-2");
    let second_api = start_remote_attempt(&handle, &mut sink, second_facts.clone());
    assert_ne!(second_api, first_api);
    // The settled attempt's stale result can never be attributed to the new permit.
    // 已结算尝试的过期结果绝不能归属到新许可。
    let (stale_host, stale_receipt) = drive_terminal_boundary(
        &app,
        &state,
        &handle,
        &mut sink,
        &mut telemetry,
        rejected_attempt_terminal(first_facts, 401),
    )
    .await;
    assert!(stale_host.is_err());
    assert_eq!(stale_receipt.as_ref().err(), stale_host.as_ref().err());
    assert!(
        sink.active
            .as_ref()
            .is_some_and(|active| active.api_call_id == second_api)
    );

    // The retried attempt confirms its own dedicated native terminal.
    // 重试尝试确认属于自身的专用原生终态。
    let (completion, waiter) = test_provider_request_durability_barrier();
    let applied = crate::context_compaction::apply_remote_compaction_completion(
        &app,
        &state,
        "",
        &mut sink,
        &mut telemetry,
        ProviderRequestTelemetryEvent {
            kind: ProviderRequestTelemetryEventKind::Completed,
            session_id: handle.session_id(),
            payload: json!({}),
            completed_usage: None,
            v1_lifecycle: Some(ProviderV1LifecycleFacts::ApiTerminal(
                native_attempt_terminal(second_facts),
            )),
        },
        checkpoint(),
        completion,
    )
    .await;
    applied.expect("the retried attempt must confirm its own native checkpoint");
    tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .expect("the native terminal barrier must settle")
        .expect("the producer must observe the confirmed native terminal");
    let durability = sink.take_finished_durability();
    assert_eq!(
        durability
            .completed_remote_compaction
            .expect("the confirmed native receipt")
            .api_call_id,
        second_api,
        "the native checkpoint may only be attributed to the permit that produced it"
    );
    handle
        .read(|document| {
            let status = |api_call_id: ApiCallId| {
                document.events().iter().find_map(|row| match &row.event {
                    EventBody::ApiStats(stats) if stats.api_call_id == api_call_id => {
                        Some(stats.status)
                    }
                    _ => None,
                })
            };
            assert_eq!(status(first_api), Some(ApiCallStatus::Failed));
            assert_eq!(status(second_api), Some(ApiCallStatus::Completed));
            assert!(document.active_api().is_none());
        })
        .unwrap();
    assert!(sink.active.is_none());
}

/// Verifies the sink refuses a second physical permit while one attempt is still active.
/// 验证接收器在已有尝试仍活动时拒绝第二份物理许可。
#[tokio::test]
async fn remote_compaction_permit_refusal_keeps_the_admitted_attempt_unmodified() {
    let (_directory, handle, service_state) = remote_attempt_session();
    let state = Arc::new(std::sync::Mutex::new(service_state));
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    let active_facts = compaction_facts("remote-attempt-active");
    // candidate is the production state-only fixture; it is built before the first admitted API so
    // the refusal under test is the active-request guard rather than an unrelated baseline guard.
    // candidate 是生产纯状态夹具；它在第一份 API 准入前构建，使被测拒绝来自活动请求守卫，
    // 而不是无关的基线守卫。
    let candidate = super::super::memory_acceptance::rebuild_memory_acceptance_request(
        handle.capture_controller_projection_snapshot().unwrap(),
        Default::default(),
    )
    .expect("state-only candidate");
    let active_api = start_remote_attempt(&handle, &mut sink, active_facts);
    let refused = sink
        .prepare_request(
            &state,
            compaction_facts("remote-attempt-refused"),
            candidate,
        )
        .await;
    let error = match refused {
        Ok(_) => panic!("a second physical permit must be refused while one attempt is active"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("already active"), "{error}");
    assert_eq!(
        sink.active
            .as_ref()
            .expect("the admitted attempt stays active")
            .api_call_id,
        active_api
    );
    handle
        .read(|document| {
            assert_eq!(
                document
                    .active_api()
                    .expect("the admitted API stays durable")
                    .api_call_id,
                active_api
            );
            assert_eq!(
                document
                    .events()
                    .iter()
                    .filter(|row| matches!(row.event, EventBody::ApiStats(_)))
                    .count(),
                0,
                "a refused permit must not consume or close any physical API"
            );
        })
        .unwrap();
}

/// Verifies a failed preparation or durability confirmation blocks the producing attempt.
/// 验证准备或可靠确认失败会阻断正在生产的尝试。
#[tokio::test]
async fn remote_compaction_confirmation_failure_blocks_the_producer_without_fabricating_success() {
    for case in 0..2 {
        let (_directory, handle, service_state) = remote_attempt_session();
        let state = Arc::new(std::sync::Mutex::new(service_state));
        let app = crate::ServiceEventEmitter::new(Vec::new());
        let mut sink = ProviderV1StreamSink::new(
            Arc::clone(&handle),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        let mut telemetry =
            crate::ProviderTelemetryHookGuard::new_with_persistence(Arc::clone(&state), None);
        let facts = compaction_facts("remote-attempt-confirmation");
        let api = start_remote_attempt(&handle, &mut sink, facts.clone());
        // case 0 rejects assistant semantics on a native terminal; case 1 rejects foreign bytes.
        // 用例 0 拒绝原生终态携带助手语义；用例 1 拒绝外来字节。
        let (raw_assistant_output, confirmed_checkpoint) = if case == 0 {
            ("unexpected assistant summary", checkpoint())
        } else {
            let mut foreign = checkpoint();
            foreign.provider_id.push_str("-foreign");
            ("", foreign)
        };
        let (completion, waiter) = test_provider_request_durability_barrier();
        let applied = crate::context_compaction::apply_remote_compaction_completion(
            &app,
            &state,
            raw_assistant_output,
            &mut sink,
            &mut telemetry,
            ProviderRequestTelemetryEvent {
                kind: ProviderRequestTelemetryEventKind::Completed,
                session_id: handle.session_id(),
                payload: json!({}),
                completed_usage: None,
                v1_lifecycle: Some(ProviderV1LifecycleFacts::ApiTerminal(
                    native_attempt_terminal(facts),
                )),
            },
            confirmed_checkpoint,
            completion,
        )
        .await;
        let error = applied.expect_err("an unconfirmable native terminal must fail");
        // The waiting producer receives the identical error instead of a silent acknowledgement.
        // 等待中的生产者收到同一错误，而不是静默确认。
        let receipt = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("the producer barrier must settle instead of hanging");
        assert_eq!(receipt.as_ref().err(), Some(&error));
        // No native success may be fabricated and the admitted permit stays attributable.
        // 不得伪造原生成功，且已准入许可保持可归属。
        assert!(sink.completed_remote_compaction.is_none());
        assert!(sink.api_terminal_completion.is_none());
        assert_eq!(
            sink.active
                .as_ref()
                .expect("the unconfirmed attempt keeps its permit")
                .api_call_id,
            api
        );
        handle
            .read(|document| {
                assert!(document.active_api().is_some());
                assert_eq!(
                    document
                        .events()
                        .iter()
                        .filter(|row| matches!(row.event, EventBody::ApiStats(_)))
                        .count(),
                    0
                );
                assert!(
                    !document
                        .events()
                        .iter()
                        .any(|row| matches!(row.event, EventBody::AssistantText(_)))
                );
            })
            .unwrap();
    }
}

/// Commits one real durable remote checkpoint whose admitted baseline carries the fixed host guidance.
/// 提交一份真实可靠的远端检查点，其准入基线携带固定宿主指引。
///
/// Production records the baseline admitted with the physical compact API; committing the shared
/// empty-baseline fixture would instead trip the unrelated host-guidance guard before any entry
/// could even read the checkpoint.
/// 生产记录随物理压缩 API 准入的基线；若使用空基线夹具，会在任何入口读到检查点之前先触发
/// 无关的宿主指引守卫。
fn stored_host_guided_checkpoint() -> (tempfile::TempDir, Arc<SessionHandle>) {
    let (directory, handle) = published_stored_open_turn();
    // baseline is exactly the projection `prepare_request` admits for a real physical request.
    // baseline 正是`prepare_request`为真实物理请求准入的投影。
    let baseline = initial_locked_baseline_with_host_guidance(&LockedBaselineProjection::empty())
        .expect("admitted locked baseline");
    let mut facts = request_facts();
    facts.purpose = ApiPurposeKind::Compaction;
    facts.request.profile = Some(checkpoint().protocol_profile_id);
    facts.prepared_at_ms = 10;
    let request = facts.request.clone();
    let projection = handle
        .read(|document| {
            let through_seq = document
                .events()
                .last()
                .map(|event| event.seq)
                .expect("submitted root input");
            ProviderProjectionProof {
                segment_generation: document.segment_generation(),
                document_revision: document.memory_revision(),
                retained_from_seq: Seq::try_new(1).unwrap(),
                through_seq,
                fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                    .unwrap(),
            }
        })
        .expect("exact provider projection proof");
    let prepared = handle
        .prepare_and_start_api_durable(PrepareAndStartApiInput {
            purpose: facts.purpose,
            request: request.clone(),
            request_budget_evidence:
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
            projection,
            locked_baseline: baseline.clone(),
            continuation_action_id: None,
            now_ms: facts.prepared_at_ms,
        })
        .expect("durably prepare the compact API");
    assert!(
        prepared.changed,
        "the admitted compact API preparation must commit"
    );
    let api = prepared.value.api_call_id;
    let terminal = handle
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
                        completed_at_ms: 11,
                    },
                )
            },
        )
        .unwrap()
        .value
        .stats_seq;
    handle
        .record_compaction(
            CompactionCommitInput::remote_responses(
                api,
                terminal,
                Seq::try_new(1).unwrap(),
                checkpoint(),
                baseline,
            ),
            12,
        )
        .unwrap();
    (directory, handle)
}

/// Verifies the main-session physical entry consumes the durable native checkpoint without a text fallback.
/// 验证主会话物理入口消费可靠原生检查点，且不降级为文本。
#[test]
fn main_session_physical_entry_consumes_the_durable_native_checkpoint() {
    let (_directory, handle) = stored_host_guided_checkpoint();
    let snapshot = handle.capture_controller_projection_snapshot().unwrap();
    // The durable main-session projection carries exactly one native checkpoint item.
    // 可靠主会话投影恰好携带一个原生检查点条目。
    assert_eq!(
        snapshot
            .items
            .iter()
            .filter(|item| matches!(item, ProjectedItem::RemoteCompactionCheckpoint { .. }))
            .count(),
        1
    );
    // The logical controller projection consumes those exact durable bytes, located by payload
    // rather than by an evolving position, and never turns them into assistant text.
    // 逻辑控制器投影消费同一份可靠字节，并按载荷而非易变位置定位，且绝不把它们变成助手正文。
    let controller =
        provider_projection_controller_state(&snapshot).expect("controller projection");
    let expected_native =
        serde_json::to_value(projection::controller_payload(&checkpoint()).unwrap()).unwrap();
    assert_eq!(
        controller
            .messages
            .iter()
            .filter(|message| serde_json::to_value(&message.payload).unwrap() == expected_native)
            .count(),
        1
    );
    assert!(controller.messages.iter().all(|message| !matches!(
        &message.payload,
        SessionConversationPayload::Assistant { text, .. } if !text.is_empty()
    )));
    // The same production entry used by `prepare_request` must consume the checkpoint for the main
    // session's ordinary next request; dropping it would silently corrupt the compacted history.
    // `prepare_request` 使用的同一生产入口必须为主会话下一次普通请求消费检查点；丢弃它会静默
    // 破坏已压缩历史。
    let error = match super::super::memory_acceptance::rebuild_memory_acceptance_request(
        snapshot,
        Default::default(),
    ) {
        Ok(_) => panic!("a native checkpoint cannot be rebuilt without its frozen execution route"),
        Err(error) => error,
    };
    assert!(
        error.contains("Opaque compaction checkpoint requires a frozen execution route"),
        "{error}"
    );
}
