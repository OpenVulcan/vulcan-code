//! Checkpoint projection through real durable owners, cold loading, and native encoding.
//! 通过真实可靠所有者、冷加载和原生编码验证检查点投影。

use super::super::remote_checkpoint as projection;
use super::*;
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
