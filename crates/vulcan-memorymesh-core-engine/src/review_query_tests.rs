//! Exercises private query-source and mapping corruption without relying on the outer checkpoint checksum.
//! 本文件属于引擎恢复测试层，不依赖外层校验和检验查询来源和映射损坏。

use super::*;
use vulcan_memorymesh_core_contracts::OperationId;

/// Retains a real policy and one internally consistent query wait for targeted corruption tests.
/// 为定向损坏测试保留真实策略及一个内部一致的查询等待。
struct QueryWait {
    /// Carries the exact compiled policy used to construct and validate the wait.
    /// 携带构造及校验该等待使用的精确已编译策略。
    snapshot: Arc<ConfigSnapshot>,
    /// Carries the original command that owns the continuation.
    /// 携带拥有该续体的原命令。
    original: CoreCommand,
    /// Carries the private continuation under test.
    /// 携带被测私有续体。
    state: FlowState,
    /// Carries the precise effect expected at that continuation.
    /// 携带该续体预期的精确效果。
    request: EffectRequest,
}

/// Constructs an admitted L1 snapshot and one completed query before a distinct second query waits.
/// 构造已准入L1快照及首个已完成查询，使不同的第二查询处于等待；返回一致夹具或构造错误。
#[allow(clippy::too_many_lines)]
fn analyze_wait() -> Result<QueryWait, Box<dyn std::error::Error>> {
    let (mut pipeline, snapshot, mut request) = observation_tests::waiting()?;
    let turn_id = TurnId::new("query-turn")?;
    let session = vulcan_memorymesh_core_contracts::SessionSourceId::new("query-session")?;
    let command = PostActionAnalyzePendingTurnCommand {
        subject_id: pipeline.work.subject_id.clone(),
        scope_access: pipeline.work.scope_access.clone(),
        replacement_scope: vulcan_memorymesh_core_contracts::MemoryReplacementScope {
            candidates: vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                scope_ids: pipeline.work.scope_access.visible_scopes.clone(),
                origin_session_source_id: None,
            },
            authorized_supersede_scopes: Vec::new(),
        },
        profile_sections: vec![ProfileViewSection {
            scope_id: pipeline.work.scope_access.write_scope.clone(),
            role: SectionRole::Subject,
            display_label: "Subject".to_owned(),
        }],
        session_source_id: session.clone(),
        vector_space_id: pipeline.work.vector_space_id.clone(),
        turn_id: turn_id.clone(),
        analysis_generation: 1,
        analysis_cutoff_ns: 10,
        direct_write_observed_after_ns: 0,
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new("query-operation")?,
            idempotency_key: IdempotencyKey::new("query-key")?,
            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
            source_kind: vulcan_memorymesh_core_contracts::MutationSourceKind::HostRpc,
            request_source_id: "query-test".to_owned(),
            observed_at_ns: 10,
        },
    };
    let turn = Turn {
        id: turn_id,
        subject_id: command.subject_id.clone(),
        scope_id: command.scope_access.write_scope.clone(),
        session_source_id: session,
        timeline: vec![TimelineItem {
            role: Role::User,
            content: "original user input".to_owned(),
            occurred_at_ns: 1,
        }],
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: 1,
        status: TurnStatus::Pending,
        user_input_kind: None,
        analysis_details: None,
    };
    let memories = ["alpha", "ALPHA", "other"]
        .into_iter()
        .map(|text| MemoryCandidate {
            category: vulcan_memorymesh_core_contracts::MemoryCategory::General,
            summary: text.to_owned(),
            content: text.to_owned(),
            evidence_source: "user_asserted".to_owned(),
            admission: Admission::Accept,
            admission_reason: None,
        })
        .collect();
    let mut pending = pending_analyze_l1(
        command.clone(),
        turn,
        vulcan_memorymesh_core_contracts::TurnInputKind::Statement,
        "source details".to_owned(),
        memories,
        vec![ProfileCandidate {
            section_role: SectionRole::Subject,
            content: "source profile".to_owned(),
            evidence_source: "user_asserted".to_owned(),
            admission: Admission::Accept,
            admission_reason: None,
        }],
        Vec::new(),
    );
    let profile = build_analyze_profile_node(
        &pending,
        &AcceptedProfileCandidate {
            candidate_index: 0,
            normalized_content: "source profile".to_owned(),
            priority: 60,
            profile_level: ProfileLevel::Stable,
            level_reason: None,
            supersede_nodes: Vec::new(),
        },
        &BTreeMap::new(),
        ProfileNodeId::new("prior-profile")?,
        1,
    )?;
    pending.current_profile_nodes.push(profile);
    let mut memory = build_memory_record(
        &snapshot,
        &pending,
        &pending.memories[0],
        Some(vec![1.0, 0.0]),
        MemoryId::new("observed-memory")?,
    )?;
    memory.version = 1;
    let hit = SearchHit {
        memory: memory.clone(),
        search_score: 0.8,
        rerank_score: None,
        final_score: 0.8,
        explanation: Some(vulcan_memorymesh_core_contracts::ScoreExplanation {
            final_score: 0.8,
            ..Default::default()
        }),
    };
    let search = PendingAnalyzeSearch {
        query_plan: analyze_query_plan(&pending),
        pending,
        embeddings: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
        completed: vec![ReviewQueryFacts {
            hits: vec![hit],
            hard_dedupe_pool: vec![memory],
            hard_dedupe_threshold: 0.99,
            hard_dedupe_contract: snapshot
                .memory_policy
                .retrieval_policy
                .vector_contract
                .contract
                .clone(),
        }],
        evidence_cache: ContextEvidenceCache::new(),
    };
    pipeline.work = analyze_query_work(&search, &snapshot)?;
    pipeline.follow_up = PipelineFollowUp::AnalyzeReview(Box::new(search));
    request.effect = pipeline_effect(&pipeline, &snapshot)?;
    let fixture = QueryWait {
        snapshot,
        original: CoreCommand::PostActionAnalyzePendingTurn(command),
        state: FlowState::SearchPipeline(Box::new(pipeline)),
        request,
    };
    validate_pipeline_checkpoint(
        &fixture.state,
        Some(&fixture.request),
        &fixture.original,
        &fixture.snapshot,
    )?;
    Ok(fixture)
}

/// Rejects independent source, admission, cursor, cached-result and explanation corruption in a valid wait.
/// 在合法等待中分别注入来源、准入、游标、缓存结果及解释损坏，所有错误必须明确拒绝。
#[test]
#[allow(clippy::too_many_lines)]
fn analyze_query_checkpoint_rejects_corrupted_facts() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = analyze_wait()?;
    let mut accepted = Vec::new();
    for scenario in 0..15 {
        let mut state = fixture.state.clone();
        let FlowState::SearchPipeline(pipeline) = &mut state else {
            return Err("missing pipeline".into());
        };
        let PipelineFollowUp::AnalyzeReview(search) = &mut pipeline.follow_up else {
            return Err("missing Analyze query state".into());
        };
        match scenario {
            0 => search.query_plan.caller_to_unique.swap(0, 2),
            1 => search.completed.clear(),
            2 => {
                search.embeddings.pop();
            }
            3 => search.pending.command.subject_id = SubjectId::new("foreign")?,
            4 => {
                search.pending.turn.status = TurnStatus::Completed;
                search.pending.turn.user_input_kind =
                    Some(vulcan_memorymesh_core_contracts::TurnInputKind::Statement);
                search.pending.turn.analysis_details = Some("already completed".to_owned());
            }
            5 => {
                search.pending.memories[0].admission = Admission::Drop;
                search.pending.memories[0].admission_reason = Some("discarded".to_owned());
            }
            6 => search
                .pending
                .context_evidence
                .push(ContextEvidenceCandidate {
                    candidate_index: 99,
                    context_key: "mode".to_owned(),
                    context_value: "test".to_owned(),
                    relation: ContextEvidenceRelation::Support,
                }),
            7 => search.completed[0].hard_dedupe_pool[0].scope_id = ScopeId::new("foreign")?,
            8 => search.completed[0].hits[0].final_score = f64::NAN,
            9 => {
                let Some(explanation) = &mut search.completed[0].hits[0].explanation else {
                    return Err("missing explanation".into());
                };
                explanation.final_score = f64::NAN;
            }
            10 => search.pending.current_profile_nodes[0].scope_id = ScopeId::new("foreign")?,
            11 => search.pending.turn.timeline[0].occurred_at_ns = 2,
            12 => pipeline.work.queries[0] = "different query".to_owned(),
            13 => search.completed[0].hard_dedupe_threshold = 0.5,
            14 => {
                "foreign-contract".clone_into(&mut search.completed[0].hard_dedupe_contract.id);
            }
            _ => return Err("unknown corruption".into()),
        }
        if validate_pipeline_checkpoint(
            &state,
            Some(&fixture.request),
            &fixture.original,
            &fixture.snapshot,
        )
        .is_ok()
        {
            accepted.push(scenario);
        }
    }
    assert!(
        accepted.is_empty(),
        "accepted invalid scenarios: {accepted:?}"
    );
    Ok(())
}

/// Creates a direct-write wait through actual initialization and typed soft-lookup/embedding confirmations.
/// 经真实初始化及强类型软查询/嵌入确认创建主动写入等待，不手工拼装拟写入记录；返回已验证夹具。
fn direct_wait(single: bool, query: bool) -> Result<QueryWait, Box<dyn std::error::Error>> {
    let basis = analyze_wait()?;
    let CoreCommand::PostActionAnalyzePendingTurn(analyze) = basis.original else {
        return Err("missing source command".into());
    };
    let items = ["alpha", "ALPHA", "other"]
        .into_iter()
        .map(|text| MemoryWriteBatchItem {
            target_scope_id: analyze.scope_access.write_scope.clone(),
            scope_class: MemoryScopeClass::Environment,
            category: vulcan_memorymesh_core_contracts::MemoryCategory::General,
            summary: text.to_owned(),
            details: text.to_owned(),
            priority: None,
            memory_level: None,
            expires_at_ns: None,
            tags: Vec::new(),
        })
        .collect();
    let mut original = CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
        replacement_scope: analyze.replacement_scope,
        subject_id: analyze.subject_id,
        authorized_write_scopes: vec![analyze.scope_access.write_scope.clone()],
        scope_access: analyze.scope_access,
        session_source_id: analyze.session_source_id,
        vector_space_id: analyze.vector_space_id,
        occurred_at_ns: 10,
        items,
        mutation_identity: analyze.mutation_identity,
    });
    if single {
        let CoreCommand::MemoryWriteBatch(batch) = original else {
            return Err("missing initial batch".into());
        };
        original = CoreCommand::MemoryWrite(MemoryWriteCommand {
            replacement_scope: batch.replacement_scope,
            subject_id: batch.subject_id,
            scope_access: batch.scope_access,
            session_source_id: batch.session_source_id,
            vector_space_id: batch.vector_space_id,
            item: batch.items[0].clone(),
            occurred_at_ns: batch.occurred_at_ns,
            mutation_identity: batch.mutation_identity,
        });
    }
    let fingerprint = vulcan_memorymesh_core_contracts::request_fingerprint(
        &original,
        basis.snapshot.id.as_str(),
    );
    match &mut original {
        CoreCommand::MemoryWriteBatch(command) => {
            command.mutation_identity.request_fingerprint = fingerprint;
        }
        CoreCommand::MemoryWrite(command) => {
            command.mutation_identity.request_fingerprint = fingerprint;
        }
        _ => return Err("missing direct command".into()),
    }
    let mut engine = CommandEngine::new();
    let (handle, _) = engine.start(original.clone(), basis.snapshot.clone())?;
    let count = if single { 1 } else { 3 };
    let mut confirmations = vec![HostEffectResult::StorageQuery(
        StorageQueryResult::RecentDirectWriteDedupe(vec![None; count]),
    )];
    if query {
        let vectors = if single {
            vec![vec![1.0, 0.0]]
        } else {
            vec![vec![1.0, 0.0], vec![0.0, 1.0]]
        };
        confirmations.push(HostEffectResult::Embedding(vectors.into()));
    }
    for result in confirmations {
        let effect_id = engine
            .records
            .get(&handle.command_id)
            .and_then(|record| record.pending_effect_id.clone())
            .ok_or("missing pending effect")?;
        engine.resume(&handle, EffectResult { effect_id, result })?;
    }
    let record = engine
        .records
        .get(&handle.command_id)
        .ok_or("missing command record")?;
    let fixture = QueryWait {
        snapshot: basis.snapshot,
        original,
        state: record.state.clone(),
        request: record
            .pending_effect
            .clone()
            .ok_or("missing query request")?,
    };
    validate_pipeline_checkpoint(
        &fixture.state,
        Some(&fixture.request),
        &fixture.original,
        &fixture.snapshot,
    )?;
    Ok(fixture)
}

/// Rejects proposed-write corruption as well as query-map corruption before any L2 or persistence continuation runs.
/// 在L2或持久化续体运行前，同时拒绝拟写入记录及查询映射损坏，确保未检查字段不能进入最终写入。
#[test]
fn direct_query_checkpoint_rejects_record_drift() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = direct_wait(false, true)?;
    let mut accepted = Vec::new();
    for scenario in 0..12 {
        let mut state = fixture.state.clone();
        let FlowState::SearchPipeline(pipeline) = &mut state else {
            return Err("missing pipeline".into());
        };
        let PipelineFollowUp::DirectWriteReview { pending, .. } = &mut pipeline.follow_up else {
            return Err("missing direct query state".into());
        };
        match scenario {
            0 => pending.queries.review_to_item.push(1),
            1 => pending.entries[1] = Some(PendingBatchMemory::Earlier(2)),
            2 => pending.proposed[0].tags.push("forged tag".to_owned()),
            3 => pending.proposed[0].source_kind = MemorySourceKind::Turn,
            4 => pending.proposed[0].state = MemoryState::Superseded,
            5 => pending.proposed[0].version = 10,
            6 => pending.proposed[0].recalled_count = 1,
            7 => pending.proposed[0].embedding = Some(vec![0.0, 1.0]),
            8 => pending.proposed[0].importance = 0.7,
            9 => pending.proposed[0].expires_at_ns = None,
            10 => pending.proposed[0].scope_class = MemoryScopeClass::Subject,
            11 => pending.scope_access.write_scope = ScopeId::new("foreign")?,
            _ => return Err("unknown direct corruption".into()),
        }
        if validate_pipeline_checkpoint(
            &state,
            Some(&fixture.request),
            &fixture.original,
            &fixture.snapshot,
        )
        .is_ok()
        {
            accepted.push(scenario);
        }
    }
    assert!(
        accepted.is_empty(),
        "accepted invalid direct scenarios: {accepted:?}"
    );
    Ok(())
}

/// Checks both direct-write embedding waits against the exact source command, hash and query effect.
/// 按精确来源命令、哈希及查询效果校验两类主动写入嵌入等待，拒绝单条入口遗漏。
#[test]
fn direct_embedding_checkpoint_binds_source() -> Result<(), Box<dyn std::error::Error>> {
    let mut accepted = Vec::new();
    for single in [false, true] {
        let fixture = direct_wait(single, false)?;
        for scenario in 0..4 {
            let mut state = fixture.state.clone();
            let mut request = fixture.request.clone();
            match scenario {
                0 | 3 => {
                    let HostEffect::Embedding(effect) = &mut request.effect else {
                        return Err("missing embedding effect".into());
                    };
                    if scenario == 0 {
                        effect.inputs[0] = "different query".to_owned();
                    } else {
                        effect.allow_partial_invalid_inputs = true;
                    }
                }
                1 | 2 => match &mut state {
                    FlowState::MemoryWriteEmbedding(pending) => {
                        if scenario == 1 {
                            pending.command.item.summary = "different item".to_owned();
                        } else {
                            pending.dedupe_hash = "different hash".to_owned();
                        }
                    }
                    FlowState::MemoryWriteBatchEmbedding(pending) => {
                        if scenario == 1 {
                            pending.command.items[0].summary = "different item".to_owned();
                        } else {
                            pending.dedupe_hashes[0] = "different hash".to_owned();
                        }
                    }
                    _ => return Err("missing embedding continuation".into()),
                },
                _ => return Err("unknown embedding corruption".into()),
            }
            if validate_pipeline_checkpoint(
                &state,
                Some(&request),
                &fixture.original,
                &fixture.snapshot,
            )
            .is_ok()
            {
                accepted.push((single, scenario));
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "accepted invalid embedding waits: {accepted:?}"
    );
    Ok(())
}
