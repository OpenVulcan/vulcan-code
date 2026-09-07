//! Tests private observation continuation validation without reconstructing host database state.
//! 测试私有观察续体校验，不重建宿主数据库状态。

use super::*;
use vulcan_memorymesh_core_config::{ConfigBundle, ConfigRegistry, MemoryPolicy};
use vulcan_memorymesh_core_contracts::{CandidateReadMode, OperationId, PhysicalCandidateScore};

/// Builds an exact empty observation wait bound to a two-dimensional frozen policy.
/// 构造绑定二维冻结策略的精确空观察等待；返回流水线、快照与匹配效果，失败传播。
pub(super) fn waiting()
-> Result<(SearchPipeline, Arc<ConfigSnapshot>, EffectRequest), Box<dyn std::error::Error>> {
    let mut policy = MemoryPolicy::default();
    policy.retrieval_policy.candidate_read_mode = CandidateReadMode::SingleObservation;
    let snapshot = ConfigRegistry::new()?.register(ConfigBundle {
        memory_policy: policy,
        embedding_dimension: 2,
        ..ConfigBundle::default()
    })?;
    let pipeline = SearchPipeline {
        stage: PipelineStage::Observation,
        follow_up: PipelineFollowUp::MemorySearch,
        fused: vec![],
        rerank_indices: vec![],
        work: SearchWork {
            candidate_scope: None,
            subject_id: SubjectId::new("subject")?,
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("scope")?,
                visible_scopes: vec![ScopeId::new("scope")?],
            },
            queries: vec!["one query".to_owned()],
            embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector")?,
            boundary: None,
            limit: 2,
            enable_hard_dedupe_pool: false,
            now_ns: 10,
            separate_fallback_active: false,
            vector: vec![],
            lexical: vec![],
            records: BTreeMap::new(),
            evidence: vec![],
        },
    };
    let request = EffectRequest {
        effect_id: EffectId::new("effect")?,
        command_id: CommandId::new("command")?,
        config_snapshot_id: snapshot.id.clone(),
        operation_id: None,
        deadline_class: EffectDeadlineClass::Interactive,
        retryable: true,
        side_effect_possible: false,
        effect: HostEffect::StorageQuery(StorageQuery::CandidateObservation(
            candidate_observation_query(&pipeline.work, &snapshot)?,
        )),
    };
    Ok((pipeline, snapshot, request))
}

/// Rejects mode, stage and query drift even when a checkpoint container itself has been correctly rehashed.
/// 即使检查点容器已重新计算正确摘要，仍拒绝模式、阶段及查询漂移；返回测试错误或成功。
#[test]
fn observation_continuation_rejects_semantic_drift() -> Result<(), Box<dyn std::error::Error>> {
    let (pipeline, snapshot, request) = waiting()?;
    let state = FlowState::SearchPipeline(Box::new(pipeline.clone()));
    validate_observation_checkpoint(&state, Some(&request), &snapshot)?;
    for stage in [
        PipelineStage::Vector,
        PipelineStage::Lexical,
        PipelineStage::Records,
    ] {
        let mut changed = pipeline.clone();
        changed.stage = stage;
        assert_eq!(
            validate_observation_checkpoint(
                &FlowState::SearchPipeline(Box::new(changed)),
                Some(&request),
                &snapshot
            )
            .map_err(|error| error.code),
            Err(ErrorCode::CheckpointIncompatible)
        );
    }
    let mut changed_request = request.clone();
    if let HostEffect::StorageQuery(StorageQuery::CandidateObservation(query)) =
        &mut changed_request.effect
    {
        query.now_ns += 1;
    }
    assert!(validate_observation_checkpoint(&state, Some(&changed_request), &snapshot).is_err());
    let separate = ConfigRegistry::new()?.register(ConfigBundle::default())?;
    assert!(validate_observation_checkpoint(&state, Some(&request), &separate).is_err());
    let mut multiple = pipeline.work;
    multiple.queries.push("second query".to_owned());
    assert!(candidate_observation_query(&multiple, &snapshot).is_err());
    Ok(())
}

/// Rejects premature facts and retained candidate unions with missing records while accepting a genuinely empty observation.
/// 拒绝过早事实和缺记录的保留候选并集，同时允许真实空观察；返回测试错误或成功。
#[test]
fn observation_checkpoint_never_reloads_missing_records() -> Result<(), Box<dyn std::error::Error>>
{
    let (mut pipeline, snapshot, request) = waiting()?;
    pipeline.stage = PipelineStage::Evidence;
    validate_observation_checkpoint(
        &FlowState::SearchPipeline(Box::new(pipeline.clone())),
        None,
        &snapshot,
    )?;
    let descriptor = &snapshot.memory_policy.retrieval_policy.vector_contract;
    pipeline.work.vector.push(PhysicalCandidate {
        memory_id: "missing-record".to_owned(),
        physical_rank: 0,
        score: PhysicalCandidateScore {
            raw_value: 0.5,
            raw_score_kind: descriptor.raw_score_kind,
            direction: descriptor.direction,
            contract: descriptor.contract.clone(),
            decision_score: Some(0.5),
        },
    });
    for stage in [
        PipelineStage::Observation,
        PipelineStage::Embedding,
        PipelineStage::Evidence,
        PipelineStage::Rerank,
    ] {
        pipeline.stage = stage;
        assert!(
            validate_observation_checkpoint(
                &FlowState::SearchPipeline(Box::new(pipeline.clone())),
                Some(&request),
                &snapshot
            )
            .is_err()
        );
    }
    Ok(())
}

/// Rejects prefetched, malformed and oversized cached observations independently of outer-container checksums.
/// 独立于外层校验和拒绝提前塞入、畸形及超量缓存观察；返回测试错误或成功。
#[test]
fn evidence_cache_checkpoint_revalidates_retained_rows() -> Result<(), Box<dyn std::error::Error>> {
    let (mut pipeline, snapshot, _) = waiting()?;
    let command = MemorySearchBatchCommand {
        subject_id: pipeline.work.subject_id.clone(),
        scope_access: pipeline.work.scope_access.clone(),
        queries: vec!["first".to_owned(), "second".to_owned()],
        top_k: 2,
        vector_space_id: pipeline.work.vector_space_id.clone(),
        boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        now_ns: 10,
    };
    let identity = MemoryId::new("previously-observed")?;
    pipeline.stage = PipelineStage::Embedding;
    pipeline.follow_up = PipelineFollowUp::MemorySearchBatchGroup {
        evidence_cache: BTreeMap::from([(identity.clone(), vec![])]),
        query_plan: QueryBatchPlan::new(&command.queries),
        command,
        embeddings: vec![],
        group: 1,
        completed: vec![],
    };
    // An empty successful row remains a real observation; no current evidence read is required at embedding time.
    // 成功空行集仍是真实观察；嵌入阶段不需要当前证据读取效果。
    validate_evidence_cache_checkpoint(
        &FlowState::SearchPipeline(Box::new(pipeline.clone())),
        None,
        &snapshot,
    )?;
    let mut premature = pipeline.clone();
    if let PipelineFollowUp::MemorySearchBatchGroup { group, .. } = &mut premature.follow_up {
        *group = 0;
    }
    assert!(
        validate_evidence_cache_checkpoint(
            &FlowState::SearchPipeline(Box::new(premature)),
            None,
            &snapshot
        )
        .is_err()
    );
    let mut malformed = pipeline.clone();
    let row = vulcan_memorymesh_core_contracts::MemoryContextEvidence {
        memory_id: MemoryId::new("foreign-id")?,
        context_key: "mode".to_owned(),
        context_value: "deployment".to_owned(),
        support_count: 1,
        rebuttal_count: 0,
        last_supported_at_ns: Some(11),
        last_rebutted_at_ns: None,
        version: 1,
    };
    pipeline_evidence_cache_mut(&mut malformed)
        .ok_or("cache missing")?
        .insert(identity, vec![row]);
    assert!(
        validate_evidence_cache_checkpoint(
            &FlowState::SearchPipeline(Box::new(malformed)),
            None,
            &snapshot
        )
        .is_err()
    );
    let cache = pipeline_evidence_cache_mut(&mut pipeline).ok_or("cache missing")?;
    for index in 0..snapshot.memory_policy.retrieval_policy.candidate_pool_max {
        cache.insert(MemoryId::new(format!("extra-{index}"))?, vec![]);
    }
    assert!(
        validate_evidence_cache_checkpoint(
            &FlowState::SearchPipeline(Box::new(pipeline)),
            None,
            &snapshot
        )
        .is_err()
    );
    Ok(())
}

/// Rejects inconsistent unique-query maps, completed cursors, prepared vectors and pending effects.
/// 拒绝不一致的唯一查询映射、已完成游标、准备向量和待处理效果；返回测试错误或成功。
#[test]
#[allow(clippy::too_many_lines)]
fn search_batch_checkpoint_revalidates_query_plan() -> Result<(), Box<dyn std::error::Error>> {
    let (mut pipeline, snapshot, mut request) = waiting()?;
    let original = MemorySearchBatchCommand {
        subject_id: pipeline.work.subject_id.clone(),
        scope_access: pipeline.work.scope_access.clone(),
        queries: vec![
            " First ".to_owned(),
            "second".to_owned(),
            "FIRST".to_owned(),
        ],
        top_k: 2,
        vector_space_id: pipeline.work.vector_space_id.clone(),
        boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        now_ns: 10,
    };
    let mut normalized = original.clone();
    normalized.queries = original
        .queries
        .iter()
        .map(|query| normalize_text(query))
        .collect();
    pipeline.work.queries = vec!["second".to_owned()];
    pipeline.work.embeddings = vec![vec![0.0, 1.0]];
    pipeline.follow_up = PipelineFollowUp::MemorySearchBatchGroup {
        query_plan: QueryBatchPlan::new(&normalized.queries),
        command: normalized,
        evidence_cache: ContextEvidenceCache::new(),
        embeddings: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
        group: 1,
        completed: vec![MemorySearchGroup {
            query_index: 0,
            query: "First".to_owned(),
            hits: vec![],
        }],
    };
    request.effect = pipeline_effect(&pipeline, &snapshot)?;
    let original = CoreCommand::MemorySearchBatch(original);
    validate_search_batch_checkpoint(
        &FlowState::SearchPipeline(Box::new(pipeline.clone())),
        Some(&request),
        &original,
        &snapshot,
    )?;
    for scenario in 0..10 {
        let mut corrupted = pipeline.clone();
        let PipelineFollowUp::MemorySearchBatchGroup {
            query_plan,
            command,
            embeddings,
            group,
            completed,
            ..
        } = &mut corrupted.follow_up
        else {
            return Err("missing batch continuation".into());
        };
        match scenario {
            0 => query_plan.caller_to_unique[2] = 1,
            1 => query_plan.unique_queries.swap(0, 1),
            2 => *group = 2,
            3 => completed.clear(),
            4 => completed[0].query_index = 1,
            5 => {
                embeddings.pop();
            }
            6 => embeddings[1][0] = f32::NAN,
            7 => {
                corrupted.work.embeddings[0].pop();
            }
            8 => command.now_ns += 1,
            9 => corrupted.work.queries[0] = "First".to_owned(),
            _ => return Err("unknown scenario".into()),
        }
        assert!(
            validate_search_batch_checkpoint(
                &FlowState::SearchPipeline(Box::new(corrupted)),
                Some(&request),
                &original,
                &snapshot
            )
            .is_err(),
            "scenario {scenario}"
        );
    }
    request.effect = HostEffect::Embedding(EmbeddingEffect {
        allow_partial_invalid_inputs: false,
        vector_space_id: pipeline.work.vector_space_id.clone(),
        expected_dimension: 2,
        inputs: vec!["second".to_owned()],
    });
    assert!(
        validate_search_batch_checkpoint(
            &FlowState::SearchPipeline(Box::new(pipeline.clone())),
            Some(&request),
            &original,
            &snapshot
        )
        .is_err()
    );

    // The initial batch wait contains no accepted vector or completed observation, even if the checksum is valid.
    // 初始批次等待不包含已接受向量或已完成观察，即使外层校验和有效也不能例外。
    let PipelineFollowUp::MemorySearchBatchGroup {
        query_plan,
        embeddings,
        group,
        completed,
        ..
    } = &mut pipeline.follow_up
    else {
        return Err("missing batch continuation".into());
    };
    *group = 0;
    embeddings.clear();
    completed.clear();
    pipeline.stage = PipelineStage::Embedding;
    pipeline.work.queries.clone_from(&query_plan.unique_queries);
    pipeline.work.embeddings.clear();
    request.effect = pipeline_effect(&pipeline, &snapshot)?;
    validate_search_batch_checkpoint(
        &FlowState::SearchPipeline(Box::new(pipeline.clone())),
        Some(&request),
        &original,
        &snapshot,
    )?;
    pipeline.work.embeddings = vec![vec![1.0, 0.0]];
    assert!(
        validate_search_batch_checkpoint(
            &FlowState::SearchPipeline(Box::new(pipeline)),
            Some(&request),
            &original,
            &snapshot
        )
        .is_err()
    );
    Ok(())
}

/// Verifies fallback provenance guards without trusting an outer checkpoint checksum.
/// 不依赖外层检查点校验和验证回退来源约束；返回测试错误或成功。
#[test]
#[allow(clippy::too_many_lines)]
fn analyze_fallback_checkpoint_rejects_partial_review() -> Result<(), Box<dyn std::error::Error>> {
    let (pipeline, _, _) = waiting()?;
    let turn_id = TurnId::new("fallback-turn")?;
    let session_id = vulcan_memorymesh_core_contracts::SessionSourceId::new("fallback-session")?;
    let turn = Turn {
        id: turn_id.clone(),
        subject_id: pipeline.work.subject_id.clone(),
        scope_id: pipeline.work.scope_access.write_scope.clone(),
        session_source_id: session_id.clone(),
        timeline: vec![TimelineItem {
            role: Role::User,
            content: "original".to_owned(),
            occurred_at_ns: 1,
        }],
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: 1,
        status: TurnStatus::Pending,
        user_input_kind: None,
        analysis_details: None,
    };
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
        session_source_id: session_id,
        vector_space_id: pipeline.work.vector_space_id,
        turn_id,
        analysis_generation: 1,
        analysis_cutoff_ns: 10,
        direct_write_observed_after_ns: 0,
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new("fallback-operation")?,
            idempotency_key: IdempotencyKey::new("fallback-key")?,
            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
            source_kind: vulcan_memorymesh_core_contracts::MutationSourceKind::HostRpc,
            request_source_id: "test".to_owned(),
            observed_at_ns: 10,
        },
    };
    let mut pending = pending_analyze_l1(
        command,
        turn,
        vulcan_memorymesh_core_contracts::TurnInputKind::Statement,
        "analysis".to_owned(),
        vec![MemoryCandidate {
            category: vulcan_memorymesh_core_contracts::MemoryCategory::General,
            summary: "original".to_owned(),
            content: "original".to_owned(),
            evidence_source: "user_asserted".to_owned(),
            admission: Admission::Accept,
            admission_reason: None,
        }],
        vec![ProfileCandidate {
            section_role: SectionRole::Subject,
            content: "original profile".to_owned(),
            evidence_source: "user_asserted".to_owned(),
            admission: Admission::Accept,
            admission_reason: None,
        }],
        Vec::new(),
    );
    pending.review_fallback_to_l1 = true;
    pending.review = LlmOutput::PostActionReview {
        accepted_candidates: vec![AcceptedCandidate {
            candidate_index: 0,
            supersede_memory_ids: Vec::new(),
        }],
        accepted_profile_candidates: vec![AcceptedProfileCandidate {
            candidate_index: 0,
            normalized_content: "original profile".to_owned(),
            priority: 100,
            profile_level: ProfileLevel::Transient,
            level_reason: None,
            supersede_nodes: Vec::new(),
        }],
        dropped_candidates: Vec::new(),
        invalid_profile_candidate_indexes: Vec::new(),
        retired_profile_nodes: Vec::new(),
        reason: None,
    };
    let original = CoreCommand::PostActionAnalyzePendingTurn(pending.command.clone());
    // Pending source facts must be accepted consistently by the Host boundary, but cannot carry an active or superseded identity.
    // Host边界必须一致接受待评审来源事实，但不得允许其冒充活跃或已替代身份。
    let LlmOutput::PostActionReview {
        accepted_profile_candidates,
        ..
    } = &pending.review
    else {
        return Err("missing profile review".into());
    };
    let node = build_analyze_profile_node(
        &pending,
        &vulcan_memorymesh_core_config::ProfilePolicy::default(),
        &accepted_profile_candidates[0],
        &BTreeMap::new(),
        ProfileNodeId::new("committed-profile")?,
        1,
    )?;
    validate_host_profile_node(&node)?;
    for scenario in 0..3 {
        let mut invalid = node.clone();
        match scenario {
            0 => invalid.state = ProfileNodeState::Active,
            1 => invalid.source_turn_ids.clear(),
            2 => invalid.superseded_by = Some(ProfileNodeId::new("replacement-profile")?),
            _ => return Err("unknown profile scenario".into()),
        }
        assert!(validate_host_profile_node(&invalid).is_err());
    }
    validate_analyze_fallback_checkpoint(
        &FlowState::AnalyzePersistenceEmbedding(Box::new(pending.clone())),
        &original,
    )?;
    assert!(
        validate_analyze_fallback_checkpoint(
            &FlowState::AnalyzeReview(Box::new(pending.clone())),
            &original
        )
        .is_err()
    );
    for scenario in 0..6 {
        let mut invalid = pending.clone();
        let LlmOutput::PostActionReview {
            accepted_candidates,
            accepted_profile_candidates,
            ..
        } = &mut invalid.review
        else {
            return Err("missing review".into());
        };
        match scenario {
            0 => accepted_candidates.clear(),
            1 => accepted_candidates[0]
                .supersede_memory_ids
                .push("foreign".to_owned()),
            2 => accepted_profile_candidates[0].priority = 50,
            3 => accepted_profile_candidates[0].normalized_content = "changed".to_owned(),
            4 => invalid.reviewer_to_candidate.push(0),
            5 => invalid.command.subject_id = SubjectId::new("other-subject")?,
            _ => return Err("unknown scenario".into()),
        }
        assert!(
            validate_analyze_fallback_checkpoint(
                &FlowState::AnalyzePersistenceEmbedding(Box::new(invalid)),
                &original
            )
            .is_err(),
            "scenario {scenario}"
        );
    }
    Ok(())
}

/// Verifies a mandatory PostAction L1 provider failure retains its exact continuation for the host retry queue.
/// 验证必需的 PostAction L1 提供方失败会为宿主重试队列保留精确续体。
#[test]
fn postaction_l1_failure_retains_analysis_continuation() -> Result<(), Box<dyn std::error::Error>> {
    // pipeline supplies compact, already validated domain identities for the retained command and turn.
    // pipeline 为保留的命令与回合提供精简且已验证的领域身份。
    let (pipeline, snapshot, _) = waiting()?;
    let turn_id = TurnId::new("retry-turn")?;
    let session_id = vulcan_memorymesh_core_contracts::SessionSourceId::new("retry-session")?;
    let turn = Turn {
        id: turn_id.clone(),
        subject_id: pipeline.work.subject_id.clone(),
        scope_id: pipeline.work.scope_access.write_scope.clone(),
        session_source_id: session_id.clone(),
        timeline: vec![TimelineItem {
            role: Role::User,
            content: "remember grapes".to_owned(),
            occurred_at_ns: 1,
        }],
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: 1,
        status: TurnStatus::Pending,
        user_input_kind: None,
        analysis_details: None,
    };
    let command = PostActionAnalyzePendingTurnCommand {
        subject_id: pipeline.work.subject_id,
        scope_access: pipeline.work.scope_access.clone(),
        replacement_scope: vulcan_memorymesh_core_contracts::MemoryReplacementScope {
            candidates: vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                scope_ids: pipeline.work.scope_access.visible_scopes.clone(),
                origin_session_source_id: None,
            },
            authorized_supersede_scopes: Vec::new(),
        },
        profile_sections: vec![ProfileViewSection {
            scope_id: pipeline.work.scope_access.write_scope,
            role: SectionRole::Subject,
            display_label: "Subject".to_owned(),
        }],
        session_source_id: session_id,
        vector_space_id: pipeline.work.vector_space_id,
        turn_id,
        analysis_generation: 1,
        analysis_cutoff_ns: 10,
        direct_write_observed_after_ns: 0,
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new("retry-operation")?,
            idempotency_key: IdempotencyKey::new("retry-key")?,
            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
            source_kind: vulcan_memorymesh_core_contracts::MutationSourceKind::HostRpc,
            request_source_id: "test".to_owned(),
            observed_at_ns: 10,
        },
    };
    let mut record = CommandRecord {
        handle: CommandHandle {
            command_id: CommandId::new("retry-command")?,
            config_snapshot_id: snapshot.id.clone(),
        },
        snapshot,
        status: CommandStatus::Waiting,
        state: FlowState::AnalyzeAnalysis {
            command: command.clone(),
            turn: turn.clone(),
        },
        pending_effect_id: None,
        pending_effect: None,
        pending_mutation_idempotency_key: None,
        next_effect_sequence: 0,
        last_error: None,
        mutation_identity: None,
        received_effects: Vec::new(),
        checkpoint_command_input: None,
        degraded: false,
    };
    let failure = HostEffectResult::Failure(vulcan_memorymesh_core_contracts::EffectFailure {
        kind: EffectFailureKind::Llm,
        class: FailureClass::Provider,
        retryable: true,
        allow_degrade: true,
        diagnostic: "provider temporarily rejected output".to_owned(),
    });

    let error = degrade(&mut record, &failure)
        .expect_err("mandatory PostAction L1 failure must remain retryable instead of completing");
    assert_eq!(error.code, ErrorCode::EffectDegraded);
    assert!(
        error
            .diagnostic
            .contains("post-action analysis is mandatory")
    );
    assert!(matches!(
        &record.state,
        FlowState::AnalyzeAnalysis {
            command: retained_command,
            turn: retained_turn,
        } if retained_command == &command && retained_turn == &turn
    ));
    assert!(record.degraded);
    Ok(())
}
