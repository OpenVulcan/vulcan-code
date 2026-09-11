//! Actual physical retrieval provenance delivered to the native PreCheck reviewer.
//! 交付给原生 PreCheck 审核器的真实物理检索来源。

use super::*;
use vulcan_memorymesh_core::{
    CandidateReadMode, IdempotencyKey, LlmInput, LlmOutput, MemoryCandidateScope, MutationIdentity,
    MutationSourceKind, OperationId, PreCheckCommand, RecallMode, SessionRecallMode,
    SessionSourceId,
};

/// Captures real Core review inputs while substituting only model computations.
/// 捕获真实 Core 审核输入，仅替换模型计算。
struct ReviewHost {
    /// Delegates embedding fixtures and all physical storage to the existing acceptance driver.
    /// 将嵌入夹具及全部物理存储委托给既有验收驱动。
    storage: FixedEmbeddingHost,
    /// Records the production renderer's exact L2 JSON bodies.
    /// 记录生产渲染器的准确 L2 JSON 正文。
    reviews: Vec<serde_json::Value>,
    /// Counts actual intent and reranking dispatches separately.
    /// 分别统计实际意图和重排分派。
    intent_calls: usize,
    /// Counts actual reranking dispatches without inferring them from configuration.
    /// 统计实际重排分派，不根据配置推断。
    rerank_calls: usize,
}

impl AsyncHostDriver for ReviewHost {
    /// Returns closed deterministic model outputs for `request`; other effects retain production behavior.
    /// 为 `request` 返回封闭的确定性模型输出；其他效果保留生产行为。
    /// Preserves effect identity and returns real storage failures unchanged.
    /// 保留效果身份，原样返回真实存储错误。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>> {
        Box::pin(async move {
            // result substitutes no ranking, candidate merging, redaction, or prompt projection logic.
            // result 不替换排序、候选合并、脱敏或提示词投影逻辑。
            let result = match &request.effect {
                HostEffect::Llm(effect) => match &effect.input {
                    LlmInput::PreCheckIntent { user_content, .. } => {
                        assert_eq!(user_content, "用户喜欢什么水果？");
                        self.intent_calls += 1;
                        HostEffectResult::Llm(LlmOutput::PreCheckIntent {
                            reason: "查找用户水果偏好".to_owned(),
                            need_memory: true,
                            queries: vec!["西瓜".to_owned()],
                        })
                    }
                    LlmInput::PreCheckReview { .. } => {
                        self.reviews.push(
                            serde_json::from_str(&crate::native_vmm::prompt::render_llm_input(
                                &effect.input,
                            )?)
                            .expect("production L2 JSON"),
                        );
                        HostEffectResult::Llm(LlmOutput::PreCheckReview {
                            accepted_memory_ids: Vec::new(),
                        })
                    }
                    other => panic!("unexpected model stage: {other:?}"),
                },
                HostEffect::Rerank(effect) => {
                    assert_eq!(effect.query, "西瓜");
                    assert_eq!(effect.documents.len(), 4);
                    self.rerank_calls += 1;
                    HostEffectResult::Rerank(
                        effect
                            .documents
                            .iter()
                            .enumerate()
                            .map(|(index, _)| RerankEntry {
                                input_index: u32::try_from(index).expect("bounded fixture"),
                                score: 0.95
                                    - f64::from(u32::try_from(index).expect("bounded fixture"))
                                        * 0.05,
                            })
                            .collect(),
                    )
                }
                _ => return self.storage.execute_effect(request).await,
            };
            Ok(EffectResult {
                effect_id: request.effect_id,
                result,
            })
        })
    }
}

/// Verifies sixteen read/ranking/context combinations and exact reviewer provenance for four records.
/// 验证十六种读取、排序及情境组合，以及四条记录的准确审核来源。
/// Uses temporary controller storage; no user data or paid model route is accessed.
/// 使用临时控制器存储；不访问用户数据或付费模型路由。
#[tokio::test]
async fn real_controller_precheck_projects_actual_retrieval_origins() {
    // temporary owns all physical SQLite/LanceDB fixtures until explicit controller shutdown.
    // temporary 持有全部物理 SQLite/LanceDB 夹具，直至显式关闭控制器。
    let (_temporary, controller) = isolated_controller().await;
    let runtime = NativeVmmCoreRuntime::bootstrap(
        Arc::new(vulcan_ai_contract::test_support::MockHostAiPort::new()),
        bootstrap_vmm_data_plane(&controller)
            .await
            .expect("data plane"),
    )
    .await
    .expect("runtime");
    let before = InMemoryStorage::new().snapshot();
    let mut after = before.clone();
    after.dataset.memories = (130..135)
        .map(|index| indexed_memory(index, "fusion"))
        .collect();
    after.dataset.memories[0].embedding = Some(vec![0.6, 0.8]);
    after.dataset.memories[1].embedding = Some(vec![1.0, 0.0]);
    after.dataset.memories[2].embedding = Some(vec![1.0, 0.0]);
    after.dataset.memories[2].summary = "数据库架构".to_owned();
    // The fourth record falls outside the four-slot vector pool but remains a lexical hit.
    // 第四条记录落在四条向量候选池之外，但仍是全文命中。
    after.dataset.memories[3].embedding = Some(vec![-1.0, 0.0]);
    after.dataset.memories[4].embedding = Some(vec![0.0, 1.0]);
    after.dataset.memories[4].summary = "代码架构".to_owned();
    // Distinct matched counts expose cross-memory mixing; unmatched rows count only toward totals.
    // 不同匹配计数暴露跨记忆混用；未匹配行只计入总数。
    for (index, memory) in after.dataset.memories.iter().take(4).enumerate() {
        after.dataset.context_evidence.extend([
            vulcan_memorymesh_core::MemoryContextEvidence {
                memory_id: memory.id.clone(),
                context_key: "fruit".to_owned(),
                context_value: "西瓜".to_owned(),
                support_count: u32::try_from(index).expect("fixture index") + 2,
                rebuttal_count: 1,
                last_supported_at_ns: Some(1),
                last_rebutted_at_ns: Some(1),
                version: 1,
            },
            // A query word in the key is not a value match and cannot increase matched counts.
            // 键含查询词不代表值匹配，不能增加匹配计数。
            vulcan_memorymesh_core::MemoryContextEvidence {
                memory_id: memory.id.clone(),
                context_key: "西瓜".to_owned(),
                context_value: "数据库".to_owned(),
                support_count: 7,
                rebuttal_count: 4,
                last_supported_at_ns: Some(1),
                last_rebutted_at_ns: Some(1),
                version: 1,
            },
        ]);
    }
    runtime
        .host
        .data_plane
        .ensure_vector_space("test-generation", 2, r#"{"fixture":"native-review-v1"}"#)
        .await
        .expect("vector generation");
    runtime
        .host
        .persist_prepared_storage_transition("review-seed", "test", &before, &after, &())
        .await
        .expect("real indexed fixtures");

    for mode in [
        CandidateReadMode::SeparateChannels,
        CandidateReadMode::SingleObservation,
    ] {
        for (rerank, mmr, context) in [(false, false), (true, false), (false, true), (true, true)]
            .into_iter()
            .flat_map(|(rerank, mmr)| [false, true].map(|context| (rerank, mmr, context)))
        {
            // bundle keeps both real channels while isolating optional ranking participation.
            // bundle 保留两个真实通道，同时隔离可选排序的参与情况。
            let mut bundle = runtime
                .resources
                .current_host_project_bundle("native-review-v1".to_owned(), 2, "")
                .expect("native bundle");
            bundle.name = "native-review-origin-test".to_owned();
            bundle.memory_policy.precheck_policy.min_review_score = 0.0;
            bundle.memory_policy.precheck_policy.recall_top_k = 4;
            bundle.memory_policy.retrieval_policy.candidate_read_mode = mode;
            bundle.memory_policy.retrieval_policy.context =
                vulcan_memorymesh_core::ContextEvidencePolicy {
                    enabled: context,
                    support_step: 0.01,
                    support_cap: 0.04,
                    rebuttal_step: 0.03,
                    rebuttal_cap: 0.03,
                };
            bundle.memory_policy.retrieval_policy.decay.enabled = false;
            bundle.memory_policy.retrieval_policy.rerank_top_n = if rerank { 4 } else { 0 };
            bundle.memory_policy.retrieval_policy.mmr.enabled = mmr;
            // Keep selection deterministic while still running actual MMR over complete records.
            // 保持选择确定性，同时仍在完整记录上执行真实 MMR。
            bundle.memory_policy.retrieval_policy.mmr.lambda = 1.0;
            bundle
                .memory_policy
                .retrieval_policy
                .lexical_candidate_floor = 4;
            bundle.memory_policy.retrieval_policy.candidate_pool_max = 4;
            bundle.memory_policy.retrieval_policy.top_k_default = 4;
            bundle.memory_policy.retrieval_policy.top_k_max = 4;
            bundle.memory_policy.retrieval_policy.replacement_top_k = 4;
            bundle.memory_policy.retrieval_policy.hard_dedupe_pool_top_k = 4;
            runtime
                .core
                .register_config(bundle)
                .expect("register policy");
            runtime
                .core
                .activate_config("native-review-origin-test")
                .expect("activate policy");
            let snapshot = runtime.core.active_snapshot().expect("snapshot");
            let operation = format!("review-{mode:?}-{rerank}-{mmr}-{context}");
            let scope = ScopeId::new("fusion").expect("scope");
            let mut command = CoreCommand::PreCheck(PreCheckCommand {
                subject_id: SubjectId::new("subject").expect("subject"),
                scope_access: ScopeAccess {
                    write_scope: scope.clone(),
                    visible_scopes: vec![scope.clone()],
                },
                session_source_id: SessionSourceId::new("review-session").expect("session"),
                vector_space_id: VectorSpaceId::new("test-generation").expect("generation"),
                candidate_scope: MemoryCandidateScope {
                    scope_ids: vec![scope],
                    origin_session_source_id: None,
                },
                user_content: "用户喜欢什么水果？".to_owned(),
                recent_turns: Vec::new(),
                recall_mode: RecallMode::Auto,
                session_recall_mode: SessionRecallMode::Full,
                max_search_queries: 1,
                max_memories: 4,
                now_ns: 10,
                mutation_identity: MutationIdentity {
                    operation_id: OperationId::new(operation.clone()).expect("operation"),
                    idempotency_key: IdempotencyKey::new(operation.clone()).expect("idempotency"),
                    request_fingerprint: String::new(),
                    source_kind: MutationSourceKind::HostRpc,
                    request_source_id: operation,
                    observed_at_ns: 10,
                },
            });
            let fingerprint =
                vulcan_memorymesh_core::request_fingerprint(&command, snapshot.id.as_str());
            command_mutation_identity(&mut command).request_fingerprint = fingerprint;
            let mut host = ReviewHost {
                storage: FixedEmbeddingHost {
                    native: runtime.host.clone(),
                    embedding_calls: 0,
                },
                reviews: Vec::new(),
                intent_calls: 0,
                rerank_calls: 0,
            };
            let result = runtime
                .core
                .run_to_completion_with_snapshot_async(command, &snapshot.id, &mut host)
                .await
                .expect("actual PreCheck flow");
            assert!(matches!(result, CommandResult::PreCheck(_)));
            assert_eq!(host.intent_calls, 1);
            assert_eq!(host.storage.embedding_calls, 1);
            assert_eq!(host.rerank_calls, usize::from(rerank));
            assert_eq!(host.reviews.len(), 1);
            let review = &host.reviews[0];
            assert_eq!(review["user_content"], "用户喜欢什么水果？");
            assert_eq!(review["search_queries"], serde_json::json!(["西瓜"]));
            assert_eq!(review["intent_reason"], "查找用户水果偏好");
            let candidates = review["candidates"]
                .as_array()
                .expect("actual L2 candidates");
            assert_eq!(candidates.len(), 4, "{mode:?}, rerank={rerank}, mmr={mmr}");
            for (index, (id, base)) in [
                ("vmm-memory-0130", "hybrid_rrf"),
                ("vmm-memory-0131", "hybrid_rrf"),
                ("vmm-memory-0132", "vector_search"),
                ("vmm-memory-0133", "lexical_search"),
            ]
            .into_iter()
            .enumerate()
            {
                let candidate = candidates
                    .iter()
                    .find(|value| value["memory_id"] == id)
                    .expect("candidate retained");
                let mut origin = base.to_owned();
                if rerank {
                    origin = match base {
                        "vector_search" => "vector_rerank".to_owned(),
                        "lexical_search" => "lexical_rerank".to_owned(),
                        _ => format!("{base}_rerank"),
                    };
                }
                if mmr {
                    origin.push_str("_mmr");
                }
                assert_eq!(candidate["origin"], origin, "{id}, {mode:?}");
                assert_eq!(candidate["source_kind"], "direct");
                assert_eq!(candidate["score_label"], "final_score");
                // Independently compute source-contract totals, matched counts and bounded net change.
                // 独立计算来源契约总数、匹配计数及有界净变化。
                let matched_support = u32::try_from(index).expect("fixture index") + 2;
                assert_eq!(
                    candidate["support_count"],
                    if context { matched_support + 7 } else { 0 }
                );
                assert_eq!(candidate["rebuttal_count"], if context { 5 } else { 0 });
                assert_eq!(
                    candidate["matched_context_support_count"],
                    if context { matched_support } else { 0 }
                );
                assert_eq!(
                    candidate["matched_context_rebuttal_count"],
                    if context { 1 } else { 0 }
                );
                assert_eq!(
                    candidate["matched_context_values"],
                    if context {
                        serde_json::json!(["fruit=西瓜"])
                    } else {
                        serde_json::json!([])
                    }
                );
                let expected_delta = if context {
                    (f64::from(matched_support) * 0.01).min(0.04) - 0.03
                } else {
                    0.0
                };
                assert!(
                    (candidate["matched_context_score_delta"]
                        .as_f64()
                        .expect("finite delta")
                        - expected_delta)
                        .abs()
                        < 1e-12
                );
            }
        }
    }
}
