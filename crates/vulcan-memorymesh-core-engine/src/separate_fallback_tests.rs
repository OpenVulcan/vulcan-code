//! Exercises the complete combined-observation to split-read recovery and checkpoint path.
//! 测试组合观察失败后切换分离读取的完整恢复与检查点链路。

use super::*;
use vulcan_memorymesh_core_config::{ConfigBundle, ConfigRegistry, MemoryPolicy};
use vulcan_memorymesh_core_contracts::{
    CandidateChannelResult, CommandResult, EffectFailure, EffectFailureKind, FailureClass,
    MemoryCategory, MemoryScopeClass, MemoryStability, PhysicalCandidateScore,
    ScoreContractDescriptor, SeparateChannelFallbackPolicy,
};

/// Builds one frozen single-observation policy with an explicit split-read fallback.
/// 构建一份带显式分离读取降级的冻结单次观察策略；返回快照或配置错误。
fn fallback_snapshot() -> Result<Arc<ConfigSnapshot>, CoreError> {
    let mut policy = MemoryPolicy::default();
    let retrieval = &mut policy.retrieval_policy;
    retrieval.candidate_read_mode =
        vulcan_memorymesh_core_contracts::CandidateReadMode::SingleObservation;
    retrieval.rrf_score_mode = vulcan_memorymesh_core_contracts::RrfScoreMode::FusedRankOnly;
    retrieval.context.enabled = false;
    retrieval.rerank_top_n = 1;
    let mut fallback_vector = retrieval.vector_contract.clone();
    fallback_vector.contract.id = "fallback-vector".to_owned();
    let mut fallback_lexical = retrieval.lexical_contract.clone();
    fallback_lexical.contract.id = "fallback-lexical".to_owned();
    retrieval.separate_channel_fallback = Some(SeparateChannelFallbackPolicy {
        vector_contract: fallback_vector,
        lexical_contract: fallback_lexical,
        rrf_math_profile:
            vulcan_memorymesh_core_contracts::RrfMathProfile::Signed64WrappingThenFloat64V1,
        dual_channel_bonus: 0.03,
    });
    ConfigRegistry::new()?.register(ConfigBundle {
        memory_policy: policy,
        embedding_dimension: 2,
        ..ConfigBundle::default()
    })
}

/// Builds the one deterministic search command used by fallback state-machine tests.
/// 构建降级状态机测试使用的唯一确定性检索命令；返回合法公共命令或身份错误。
fn search_command() -> Result<CoreCommand, Box<dyn std::error::Error>> {
    Ok(CoreCommand::MemorySearch(MemorySearchCommand {
        subject_id: SubjectId::new("fallback-subject")?,
        scope_access: ScopeAccess {
            write_scope: ScopeId::new("fallback-scope")?,
            visible_scopes: vec![ScopeId::new("fallback-scope")?],
        },
        query: "fallback query".to_owned(),
        limit: 2,
        vector_space_id: VectorSpaceId::new("fallback-space")?,
        boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        now_ns: 10,
    }))
}

/// Extracts exactly one pending effect without accepting an ambiguous batch or terminal result.
/// 精确提取一个待处理效果，不接受含糊的批次或终态结果；返回效果或测试错误。
fn one_effect(step: CommandStep) -> Result<EffectRequest, Box<dyn std::error::Error>> {
    let CommandStep::Effects(mut effects) = step else {
        return Err("expected one pending effect".into());
    };
    if effects.len() != 1 {
        return Err("expected exactly one pending effect".into());
    }
    effects
        .pop()
        .ok_or_else(|| "pending effect disappeared".into())
}

/// Builds one typed storage failure with caller-controlled degradation authority.
/// 构建一条由调用方控制降级权限的强类型存储失败；返回效果结果。
fn storage_failure(effect_id: EffectId, allow_degrade: bool) -> EffectResult {
    EffectResult {
        effect_id,
        result: HostEffectResult::Failure(EffectFailure {
            kind: EffectFailureKind::StorageQuery,
            class: FailureClass::Storage,
            retryable: true,
            allow_degrade,
            diagnostic: "combined read unavailable".to_owned(),
        }),
    }
}

/// Builds one complete source-observed record for the split-channel recovery fixture.
/// 为分离通道恢复夹具构造一条完整的来源观察记录。
fn fallback_memory(
    id: &str,
    summary: &str,
    version: u64,
) -> Result<MemoryRecord, Box<dyn std::error::Error>> {
    Ok(MemoryRecord {
        id: MemoryId::new(id)?,
        access_class: vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(SubjectId::new("fallback-subject")?),
        scope_id: ScopeId::new("fallback-scope")?,
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::General,
        summary: summary.to_owned(),
        content: format!("{summary} content"),
        source_kind: MemorySourceKind::Turn,
        origin_session_source_id: None,
        priority: 50,
        stability: MemoryStability::Stable,
        tags: Vec::new(),
        evidence: Vec::new(),
        evidence_source: Some("source-observation".to_owned()),
        state: MemoryState::Active,
        importance: 0.5,
        confidence: Some(1.0),
        created_at_ns: 1,
        updated_at_ns: i64::try_from(version)?,
        expires_at_ns: None,
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        memory_level: MemoryLevel::Stable,
        refresh_weight: 1,
        dedupe_hash: None,
        recalled_count: 0,
        adopted_count: 0,
        cross_session_adopted_count: 0,
        decay_disabled: false,
        status_reason: None,
        last_adopted_at_ns: None,
        last_reinforced_at_ns: None,
        source_turn_id: None,
        version,
        vector_space_id: Some(VectorSpaceId::new("fallback-space")?),
        embedding: Some(vec![1.0, 0.0]),
    })
}

/// Builds one channel candidate bound to the exact contract requested by Core.
/// 构造一条绑定Core精确请求契约的通道候选。
fn fallback_candidate(memory_id: &str, descriptor: &ScoreContractDescriptor) -> PhysicalCandidate {
    PhysicalCandidate {
        memory_id: memory_id.to_owned(),
        physical_rank: 0,
        score: PhysicalCandidateScore {
            raw_value: 1.0,
            raw_score_kind: descriptor.raw_score_kind,
            direction: descriptor.direction,
            contract: descriptor.contract.clone(),
            decision_score: Some(1.0),
        },
    }
}

/// Recovers through vector, lexical and record reads, preserving the fallback contract across restore.
/// 经向量、词法与记录读取完成恢复，并验证检查点恢复前后保持同一降级契约。
#[test]
fn combined_failure_uses_split_contract_and_restores_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = fallback_snapshot()?;
    let fallback = snapshot
        .memory_policy
        .retrieval_policy
        .separate_channel_fallback
        .as_ref()
        .ok_or("fallback policy missing")?
        .clone();
    let mut engine = CommandEngine::new();
    let (handle, start) = engine.start(search_command()?, Arc::clone(&snapshot))?;
    let embedding = one_effect(start)?;
    if !matches!(embedding.effect, HostEffect::Embedding(_)) {
        return Err("search did not begin with embedding".into());
    }
    let observation = one_effect(engine.resume(
        &handle,
        EffectResult {
            effect_id: embedding.effect_id,
            result: HostEffectResult::Embedding(vec![vec![1.0, 0.0]].into()),
        },
    )?)?;
    if !matches!(
        observation.effect,
        HostEffect::StorageQuery(StorageQuery::CandidateObservation(_))
    ) {
        return Err("single-observation query was not emitted".into());
    }
    let vector = one_effect(engine.resume(&handle, storage_failure(observation.effect_id, true))?)?;
    let HostEffect::StorageQuery(StorageQuery::VectorCandidates {
        expected_contract, ..
    }) = &vector.effect
    else {
        return Err("fallback vector query was not emitted".into());
    };
    if expected_contract != &fallback.vector_contract.contract {
        return Err("fallback vector contract drifted".into());
    }

    // Export after activation so restore must retain provenance instead of retrying the combined read.
    // 激活后导出，确保恢复必须保留来源而不是重新尝试组合读取。
    let checkpoint = engine.export_checkpoint(&handle)?;
    let mut restored = CommandEngine::new();
    let snapshots = BTreeMap::from([(snapshot.id.as_str().to_owned(), Arc::clone(&snapshot))]);
    let (restored_handle, restored_step) = restored.restore_checkpoint(&checkpoint, &snapshots)?;
    let restored_vector = one_effect(restored_step)?;
    if restored_vector.effect != vector.effect {
        return Err("restored fallback vector request changed".into());
    }

    let lexical = one_effect(restored.resume(
        &restored_handle,
        EffectResult {
            effect_id: restored_vector.effect_id,
            result: HostEffectResult::StorageQuery(StorageQueryResult::VectorCandidates(
                vulcan_memorymesh_core_contracts::CandidateChannelResult {
                    candidates: Vec::new(),
                    memories: Vec::new(),
                },
            )),
        },
    )?)?;
    let HostEffect::StorageQuery(StorageQuery::LexicalCandidates {
        expected_contract, ..
    }) = &lexical.effect
    else {
        return Err("fallback lexical query was not emitted".into());
    };
    if expected_contract != &fallback.lexical_contract.contract {
        return Err("fallback lexical contract drifted".into());
    }
    let complete = restored.resume(&restored_handle, storage_failure(lexical.effect_id, true))?;
    if !matches!(
        complete,
        CommandStep::Complete(result) if matches!(*result, CommandResult::MemorySearch(ref hits) if hits.is_empty())
    ) {
        return Err("fallback search did not complete with the empty physical result".into());
    }
    Ok(())
}

/// Keeps source records inside the checkpoint and preserves vector-first overlap materialization.
/// 把来源记录保留在检查点内，并保持重叠身份由向量记录优先物化。
#[test]
fn split_source_records_skip_requery_and_restore_vector_first()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = fallback_snapshot()?;
    let fallback = snapshot
        .memory_policy
        .retrieval_policy
        .separate_channel_fallback
        .as_ref()
        .ok_or("fallback policy missing")?
        .clone();
    let mut engine = CommandEngine::new();
    let (handle, start) = engine.start(search_command()?, Arc::clone(&snapshot))?;
    let embedding = one_effect(start)?;
    let observation = one_effect(engine.resume(
        &handle,
        EffectResult {
            effect_id: embedding.effect_id,
            result: HostEffectResult::Embedding(vec![vec![1.0, 0.0]].into()),
        },
    )?)?;
    let vector = one_effect(engine.resume(&handle, storage_failure(observation.effect_id, true))?)?;
    let HostEffect::StorageQuery(StorageQuery::VectorCandidates {
        expected_contract,
        now_ns,
        ..
    }) = &vector.effect
    else {
        return Err("fallback vector query missing".into());
    };
    if expected_contract != &fallback.vector_contract.contract || *now_ns != 10 {
        return Err("fallback vector observation time drifted".into());
    }
    let memory_id = "vmm:v1:memory:00000000000000000001";
    let lexical = one_effect(engine.resume(
        &handle,
        EffectResult {
            effect_id: vector.effect_id,
            result: HostEffectResult::StorageQuery(StorageQueryResult::VectorCandidates(
                CandidateChannelResult {
                    candidates: vec![fallback_candidate(memory_id, &fallback.vector_contract)],
                    memories: vec![fallback_memory(memory_id, "vector-source", 1)?],
                },
            )),
        },
    )?)?;
    let HostEffect::StorageQuery(StorageQuery::LexicalCandidates {
        expected_contract,
        now_ns,
        ..
    }) = &lexical.effect
    else {
        return Err("fallback lexical query missing".into());
    };
    if expected_contract != &fallback.lexical_contract.contract || *now_ns != 10 {
        return Err("fallback lexical observation time drifted".into());
    }

    // Restore after the vector confirmation; the lexical request and retained source record must survive.
    // 在向量确认后恢复；词法请求与已保留来源记录必须同时存续。
    let checkpoint = engine.export_checkpoint(&handle)?;
    let snapshots = BTreeMap::from([(snapshot.id.as_str().to_owned(), Arc::clone(&snapshot))]);
    let mut restored = CommandEngine::new();
    let (restored_handle, restored_step) = restored.restore_checkpoint(&checkpoint, &snapshots)?;
    let restored_lexical = one_effect(restored_step)?;
    if restored_lexical.effect != lexical.effect {
        return Err("source-record checkpoint changed the lexical request".into());
    }
    let complete = restored.resume(
        &restored_handle,
        EffectResult {
            effect_id: restored_lexical.effect_id,
            result: HostEffectResult::StorageQuery(StorageQueryResult::LexicalCandidates(
                CandidateChannelResult {
                    candidates: vec![fallback_candidate(memory_id, &fallback.lexical_contract)],
                    memories: vec![fallback_memory(memory_id, "lexical-source", 2)?],
                },
            )),
        },
    )?;
    let CommandStep::Complete(result) = complete else {
        return Err("source-complete split read emitted a record requery".into());
    };
    let CommandResult::MemorySearch(hits) = *result else {
        return Err("split source record produced the wrong result".into());
    };
    if hits.len() != 1 || hits[0].memory.summary != "vector-source" || hits[0].memory.version != 1 {
        return Err("vector-first source record was not preserved".into());
    }
    Ok(())
}

/// Rejects partial or identity-shifted source records before any continuation state is accepted.
/// 在接受任何续体状态前拒绝部分闭包或身份错位的来源记录。
#[test]
fn split_source_records_require_exact_channel_closure() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = fallback_snapshot()?;
    let fallback = snapshot
        .memory_policy
        .retrieval_policy
        .separate_channel_fallback
        .as_ref()
        .ok_or("fallback policy missing")?
        .clone();
    let first_id = "vmm:v1:memory:00000000000000000001";
    let second_id = "vmm:v1:memory:00000000000000000002";
    for (name, candidates, memories) in [
        (
            "identity-shifted",
            vec![fallback_candidate(first_id, &fallback.vector_contract)],
            vec![fallback_memory(second_id, "wrong-identity", 1)?],
        ),
        (
            "partial-closure",
            {
                let first = fallback_candidate(first_id, &fallback.vector_contract);
                let mut second = fallback_candidate(second_id, &fallback.vector_contract);
                second.physical_rank = 1;
                second.score.raw_value = 0.5;
                second.score.decision_score = Some(0.5);
                vec![first, second]
            },
            vec![fallback_memory(first_id, "partial", 1)?],
        ),
    ] {
        let mut engine = CommandEngine::new();
        let (handle, start) = engine.start(search_command()?, Arc::clone(&snapshot))?;
        let embedding = one_effect(start)?;
        let observation = one_effect(engine.resume(
            &handle,
            EffectResult {
                effect_id: embedding.effect_id,
                result: HostEffectResult::Embedding(vec![vec![1.0, 0.0]].into()),
            },
        )?)?;
        let vector =
            one_effect(engine.resume(&handle, storage_failure(observation.effect_id, true))?)?;
        let error = engine
            .resume(
                &handle,
                EffectResult {
                    effect_id: vector.effect_id,
                    result: HostEffectResult::StorageQuery(StorageQueryResult::VectorCandidates(
                        CandidateChannelResult {
                            candidates,
                            memories,
                        },
                    )),
                },
            )
            .err()
            .ok_or("invalid source records were accepted")?;
        if error.code != ErrorCode::EffectMismatch {
            return Err(format!("{name} returned the wrong error: {:?}", error.code).into());
        }
    }
    Ok(())
}

/// Refuses to reinterpret a host-denied failure as an authorized split fallback.
/// 拒绝把宿主禁止降级的失败重新解释为获准的分离读取恢复。
#[test]
fn combined_failure_requires_explicit_degrade_authority() -> Result<(), Box<dyn std::error::Error>>
{
    let snapshot = fallback_snapshot()?;
    let mut engine = CommandEngine::new();
    let (handle, start) = engine.start(search_command()?, snapshot)?;
    let embedding = one_effect(start)?;
    let observation = one_effect(engine.resume(
        &handle,
        EffectResult {
            effect_id: embedding.effect_id,
            result: HostEffectResult::Embedding(vec![vec![1.0, 0.0]].into()),
        },
    )?)?;
    let error = engine
        .resume(&handle, storage_failure(observation.effect_id, false))
        .err()
        .ok_or("host-denied degradation unexpectedly succeeded")?;
    if error.code != ErrorCode::EffectDegraded {
        return Err("host-denied degradation returned the wrong error".into());
    }
    Ok(())
}

/// Freezes VMM's split-source RRF tie-break and rank-based business-score projection in Core arithmetic.
/// 在 Core 算术中冻结 VMM 分离来源的 RRF 平局规则与基于名次的业务分投影。
#[test]
fn split_fallback_score_matches_rank_projection() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = fallback_snapshot()?;
    let lower_id = "vmm:v1:memory:00000000000000000002";
    let higher_id = "vmm:v1:memory:00000000000000000010";
    let work = SearchWork {
        candidate_scope: None,
        subject_id: SubjectId::new("score-subject")?,
        scope_access: ScopeAccess {
            write_scope: ScopeId::new("score-scope")?,
            visible_scopes: vec![ScopeId::new("score-scope")?],
        },
        queries: vec!["score query".to_owned()],
        embeddings: vec![vec![1.0, 0.0]],
        vector_space_id: VectorSpaceId::new("score-space")?,
        boundary: None,
        limit: 2,
        enable_hard_dedupe_pool: false,
        now_ns: 10,
        separate_fallback_active: true,
        vector: Vec::new(),
        lexical: Vec::new(),
        records: BTreeMap::new(),
        evidence: Vec::new(),
    };
    let policy = effective_retrieval_policy(&work, &snapshot.memory_policy.retrieval_policy)?;
    let vector = vec![
        PhysicalCandidate {
            memory_id: lower_id.to_owned(),
            physical_rank: 0,
            score: PhysicalCandidateScore {
                raw_value: 0.1,
                raw_score_kind: policy.vector_contract.raw_score_kind,
                direction: policy.vector_contract.direction,
                contract: policy.vector_contract.contract.clone(),
                decision_score: Some(0.9),
            },
        },
        PhysicalCandidate {
            memory_id: higher_id.to_owned(),
            physical_rank: 1,
            score: PhysicalCandidateScore {
                raw_value: 0.2,
                raw_score_kind: policy.vector_contract.raw_score_kind,
                direction: policy.vector_contract.direction,
                contract: policy.vector_contract.contract.clone(),
                decision_score: Some(0.8),
            },
        },
    ];
    let lexical = vec![
        PhysicalCandidate {
            memory_id: higher_id.to_owned(),
            physical_rank: 0,
            score: PhysicalCandidateScore {
                raw_value: 9.0,
                raw_score_kind: policy.lexical_contract.raw_score_kind,
                direction: policy.lexical_contract.direction,
                contract: policy.lexical_contract.contract.clone(),
                decision_score: Some(0.95),
            },
        },
        PhysicalCandidate {
            memory_id: lower_id.to_owned(),
            physical_rank: 1,
            score: PhysicalCandidateScore {
                raw_value: 4.0,
                raw_score_kind: policy.lexical_contract.raw_score_kind,
                direction: policy.lexical_contract.direction,
                contract: policy.lexical_contract.contract.clone(),
                decision_score: Some(0.5),
            },
        },
    ];
    let fused = retrieval_kernel::rrf_fusion(&vector, &lexical, &policy, 2, true)?;
    if fused.len() != 2 || fused[0].0 != lower_id || fused[1].0 != higher_id {
        return Err("split fallback order no longer matches VMM RRF tie-breaking".into());
    }
    let first = fused[0]
        .1
        .base_decision_score
        .ok_or("first fallback score missing")?;
    let second = fused[1]
        .1
        .base_decision_score
        .ok_or("second fallback score missing")?;
    if first.to_bits() != 1.0_f64.to_bits() || second.to_bits() != 1.0_f64.to_bits() {
        return Err("split fallback business-score projection drifted".into());
    }
    Ok(())
}
