//! Revalidates candidate-query continuations without reconstructing their observed database state.
//! 本文件属于引擎恢复层，校验候选查询续体，不重新读取其已观察数据库状态。

use super::{
    Admission, CoreCommand, CoreError, DirectRecordSource, DirectWriteResultMode, EffectRequest,
    EmbeddingEffect, FlowState, HostEffect, LlmOutput, LlmPurpose, MAX_DOMAIN_COLLECTION_ITEMS,
    MemoryWriteBatchCommand, PendingAnalyze, PendingBatchMemory, PendingDirectWriteReview,
    PipelineFollowUp, ReviewQueryFacts, SearchWork, StorageQuery, TurnStatus, analyze_query_plan,
    analyze_query_work, build_direct_memory_record, direct_query_representative, direct_query_work,
    direct_write_dedupe_hash, effect_type_error, pipeline_effect, plan_direct_queries,
    retrieval_kernel, validate_and_filter_profile_nodes, validate_candidate_memory_record,
    validate_host_turn, validate_recent_direct_write_hit,
};
use vulcan_memorymesh_core_config::ConfigSnapshot;

/// Checks query-specific source bindings, representative mappings and the exact pending effect.
/// 校验查询专属来源绑定、代表映射及精确等待效果；参数为恢复状态、效果、原命令和快照，错误明确返回。
#[allow(clippy::too_many_lines)]
pub(super) fn validate(
    state: &FlowState,
    request: Option<&EffectRequest>,
    original: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let invalid = || effect_type_error("review query checkpoint mismatch");
    let expected = match state {
        FlowState::AnalyzeProfileQuery(pending) | FlowState::AnalyzeEmbedding(pending) => {
            validate_l1(pending, original)?;
            if matches!(state, FlowState::AnalyzeProfileQuery(_)) {
                if pending.profiles.is_empty() || !pending.current_profile_nodes.is_empty() {
                    return Err(invalid());
                }
                HostEffect::StorageQuery(StorageQuery::ProfileNodes {
                    subject_id: pending.command.subject_id.clone(),
                    scope_access: pending.command.scope_access.clone(),
                    sections: pending.command.profile_sections.clone(),
                    evaluated_at_ns: pending.command.analysis_cutoff_ns,
                })
            } else {
                if pending.memories.is_empty()
                    || pending.memories.len()
                        > snapshot.memory_policy.retrieval_policy.max_query_items as usize
                {
                    return Err(invalid());
                }
                validate_profiles(pending)?;
                HostEffect::Embedding(EmbeddingEffect {
                    allow_partial_invalid_inputs: false,
                    vector_space_id: pending.command.vector_space_id.clone(),
                    expected_dimension: snapshot.embedding_dimension,
                    inputs: analyze_query_plan(pending).unique_queries,
                })
            }
        }
        FlowState::MemoryWriteEmbedding(pending) => {
            let CoreCommand::MemoryWrite(original) = original else {
                return Err(invalid());
            };
            let mut normalized = original.clone();
            normalized.item.summary = snapshot
                .pii_scrubber
                .scrub(normalized.item.summary.trim())?
                .text;
            normalized.item.details = snapshot
                .pii_scrubber
                .scrub(normalized.item.details.trim())?
                .text;
            if pending.command != normalized
                || pending.dedupe_hash
                    != direct_write_dedupe_hash(
                        snapshot,
                        &normalized.item,
                        &normalized.session_source_id,
                    )
            {
                return Err(invalid());
            }
            HostEffect::Embedding(EmbeddingEffect {
                allow_partial_invalid_inputs: false,
                vector_space_id: normalized.vector_space_id,
                expected_dimension: snapshot.embedding_dimension,
                inputs: vec![super::memory_review_query(
                    &normalized.item.summary,
                    &normalized.item.details,
                )],
            })
        }
        FlowState::MemoryWriteBatchEmbedding(pending) => {
            if !matches!(original, CoreCommand::MemoryWriteBatch(_)) {
                return Err(invalid());
            }
            let normalized = normalized_direct_command(original, snapshot)?;
            if normalized != pending.command
                || pending.dedupe_hashes.len() != normalized.items.len()
                || pending.queries
                    != plan_direct_queries(
                        snapshot,
                        &normalized.items,
                        &pending.existing,
                        normalized.occurred_at_ns,
                    )?
            {
                return Err(invalid());
            }
            for (index, item) in normalized.items.iter().enumerate() {
                let hash = direct_write_dedupe_hash(snapshot, item, &normalized.session_source_id);
                if pending.dedupe_hashes[index] != hash {
                    return Err(invalid());
                }
                if let Some(memory) = &pending.existing[index] {
                    validate_recent_direct_write_hit(
                        snapshot,
                        &normalized.subject_id,
                        &normalized.session_source_id,
                        normalized.occurred_at_ns,
                        item,
                        &hash,
                        memory,
                    )?;
                }
            }
            HostEffect::Embedding(EmbeddingEffect {
                allow_partial_invalid_inputs: false,
                vector_space_id: normalized.vector_space_id,
                expected_dimension: snapshot.embedding_dimension,
                inputs: pending.queries.query_plan.unique_queries.clone(),
            })
        }
        FlowState::SearchPipeline(pipeline) => {
            let expected_work = match &pipeline.follow_up {
                PipelineFollowUp::AnalyzeReview(search) => {
                    validate_l1(&search.pending, original)?;
                    validate_profiles(&search.pending)?;
                    if search.query_plan != analyze_query_plan(&search.pending)
                        || search.pending.memories.len()
                            > snapshot.memory_policy.retrieval_policy.max_query_items as usize
                        || search.embeddings.len() != search.query_plan.unique_queries.len()
                        || search.embeddings.iter().any(|vector| {
                            vector.len() != snapshot.embedding_dimension as usize
                                || vector.iter().any(|value| !value.is_finite())
                        })
                    {
                        return Err(invalid());
                    }
                    let work = analyze_query_work(search, snapshot)?;
                    validate_completed(&search.completed, &work, snapshot)?;
                    work
                }
                PipelineFollowUp::DirectWriteReview {
                    pending,
                    item_index,
                } => {
                    validate_direct_origin(pending, original, snapshot)?;
                    if direct_query_representative(pending)? != *item_index
                        || !pending.reviewer_to_item.is_empty()
                        || !pending.memory_reviews.is_empty()
                    {
                        return Err(invalid());
                    }
                    let work = direct_query_work(pending, *item_index, snapshot)?;
                    validate_completed(&pending.query_facts, &work, snapshot)?;
                    work
                }
                _ => return Ok(()),
            };
            let work = &pipeline.work;
            if work.subject_id != expected_work.subject_id
                || work.scope_access != expected_work.scope_access
                || work.queries != expected_work.queries
                || work.embeddings != expected_work.embeddings
                || work.vector_space_id != expected_work.vector_space_id
                || work.boundary != expected_work.boundary
                || work.candidate_scope != expected_work.candidate_scope
                || work.limit != expected_work.limit
                || work.enable_hard_dedupe_pool != expected_work.enable_hard_dedupe_pool
                || work.now_ns != expected_work.now_ns
            {
                return Err(invalid());
            }
            pipeline_effect(pipeline, snapshot)?
        }
        _ => return Ok(()),
    };
    if request.is_none_or(|request| request.effect != expected) {
        return Err(invalid());
    }
    Ok(())
}

/// Ensures an unreviewed L1 continuation has no partial decision or different turn owner.
/// 确保未评审L1续体没有部分决定或不同轮次归属；输入L1续体及原命令，错误明确返回。
// The source turn stores id while the command references turn_id; compare this explicit cross-type mapping.
// 来源轮次存储id，命令引用turn_id；这里校验的是明确的跨类型字段映射。
#[allow(clippy::suspicious_operation_groupings)]
fn validate_l1(pending: &PendingAnalyze, original: &CoreCommand) -> Result<(), CoreError> {
    validate_host_turn(&pending.turn)?;
    // The retained output has already passed admission, so a valid model-level drop is invalid in this later phase.
    // 保留的输出已经通过准入，模型层合法的丢弃项在此后续阶段仍属非法，不能恢复为可写入候选。
    vulcan_memorymesh_core_contracts::validate_llm_output(
        LlmPurpose::PostActionAnalysis,
        &LlmOutput::PostActionAnalysis {
            user_input_kind: pending.user_input_kind,
            turn_id: pending.turn.id.clone(),
            details: pending.analysis_details.clone(),
            memories: pending.memories.clone(),
            profiles: pending.profiles.clone(),
            context_evidence: pending.context_evidence.clone(),
        },
    )?;
    if !matches!(original, CoreCommand::PostActionAnalyzePendingTurn(command) if command == &pending.command)
        || pending.turn.status != TurnStatus::Pending
        || pending
            .memories
            .iter()
            .any(|candidate| candidate.admission != Admission::Accept)
        || pending
            .profiles
            .iter()
            .any(|candidate| candidate.admission != Admission::Accept)
        || pending.turn.id != pending.command.turn_id
        || pending.turn.subject_id != pending.command.subject_id
        || pending.turn.scope_id != pending.command.scope_access.write_scope
        || pending.turn.session_source_id != pending.command.session_source_id
        || pending.review_fallback_to_l1
        || !pending.memory_reviews.is_empty()
        || !pending.reviewer_to_candidate.is_empty()
        || !pending.hard_dropped.is_empty()
        || !pending.embeddings.is_empty()
        || !pending.embedding_dropped.is_empty()
    {
        return Err(effect_type_error("unreviewed L1 query state mismatch"));
    }
    if !matches!(&pending.review, LlmOutput::PostActionReview {
        accepted_candidates, dropped_candidates, accepted_profile_candidates,
        invalid_profile_candidate_indexes, retired_profile_nodes, reason: None,
    } if accepted_candidates.is_empty() && dropped_candidates.is_empty() && accepted_profile_candidates.is_empty()
        && invalid_profile_candidate_indexes.is_empty() && retired_profile_nodes.is_empty())
    {
        return Err(effect_type_error(
            "unreviewed L1 contains a prior review result",
        ));
    }
    Ok(())
}

/// Revalidates the already observed pre-query profile snapshot without storage IO.
/// 不执行存储IO，重新校验已观察的查询前画像快照；输入L1续体，范围或生命周期不符时返回错误。
fn validate_profiles(pending: &PendingAnalyze) -> Result<(), CoreError> {
    if pending.current_profile_nodes.len() > MAX_DOMAIN_COLLECTION_ITEMS
        || pending.profiles.is_empty() && !pending.current_profile_nodes.is_empty()
        || validate_and_filter_profile_nodes(
            pending.current_profile_nodes.clone(),
            &pending.command.subject_id,
            &pending.command.profile_sections,
            pending.command.analysis_cutoff_ns,
        )? != pending.current_profile_nodes
    {
        return Err(effect_type_error("pre-query profile observation mismatch"));
    }
    Ok(())
}

/// Reconstructs normalized direct input only from its explicit original variant and frozen scrubber.
/// 仅按明确原命令分支和冻结脱敏器重建主动输入；单条投影其既有单范围授权，不新增权限，返回规范化批次。
fn normalized_direct_command(
    original: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<MemoryWriteBatchCommand, CoreError> {
    let mut command = match original {
        CoreCommand::MemoryWriteBatch(command) => command.clone(),
        CoreCommand::MemoryWrite(command) => MemoryWriteBatchCommand {
            replacement_scope: command.replacement_scope.clone(),
            subject_id: command.subject_id.clone(),
            scope_access: command.scope_access.clone(),
            authorized_write_scopes: vec![command.scope_access.write_scope.clone()],
            session_source_id: command.session_source_id.clone(),
            vector_space_id: command.vector_space_id.clone(),
            occurred_at_ns: command.occurred_at_ns,
            items: vec![command.item.clone()],
            mutation_identity: command.mutation_identity.clone(),
        },
        _ => return Err(effect_type_error("not a direct-write checkpoint")),
    };
    for item in &mut command.items {
        item.summary = snapshot.pii_scrubber.scrub(item.summary.trim())?.text;
        item.details = snapshot.pii_scrubber.scrub(item.details.trim())?.text;
    }
    Ok(command)
}

/// Recomputes direct-query partitions before decisions from the original fields and validated soft-hit records.
/// 在决定前根据原字段及已校验软命中记录重新计算主动查询划分；输入续体、命令和快照，错误别名或权限均拒绝。
#[allow(clippy::too_many_lines)]
fn validate_direct_origin(
    pending: &PendingDirectWriteReview,
    original: &CoreCommand,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    let command = normalized_direct_command(original, snapshot)?;
    let invalid = || effect_type_error("direct query origin mismatch");
    if !matches!(
        (&pending.mode, original),
        (DirectWriteResultMode::Single, CoreCommand::MemoryWrite(_))
            | (
                DirectWriteResultMode::Batch,
                CoreCommand::MemoryWriteBatch(_)
            )
    ) || pending.proposed.len() != command.items.len()
        || pending.entries.len() != command.items.len()
        || pending.embeddings.len() != command.items.len()
        || pending.scope_access != command.scope_access
        || pending.occurred_at_ns != command.occurred_at_ns
        || pending.candidate_scope != command.replacement_scope.candidates
        || pending.authorized_supersede_scopes
            != command.replacement_scope.authorized_supersede_scopes
    {
        return Err(invalid());
    }
    let existing = pending
        .entries
        .iter()
        .map(|entry| match entry {
            Some(PendingBatchMemory::Existing(memory)) => Some(memory.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if pending.queries
        != plan_direct_queries(snapshot, &command.items, &existing, command.occurred_at_ns)?
    {
        return Err(invalid());
    }
    let source = DirectRecordSource {
        subject_id: &command.subject_id,
        session_source_id: &command.session_source_id,
        vector_space_id: &command.vector_space_id,
        occurred_at_ns: command.occurred_at_ns,
    };
    for (index, (item, proposed)) in command.items.iter().zip(&pending.proposed).enumerate() {
        let hash = direct_write_dedupe_hash(snapshot, item, &command.session_source_id);
        // Use the execution constructor so new fields cannot escape a manually maintained comparison list.
        // 共用执行构造器，防止新增字段逃逸手工维护的比较清单。
        let expected = build_direct_memory_record(
            snapshot,
            &source,
            item,
            &hash,
            index,
            pending.embeddings[index].clone(),
        )?;
        if proposed != &expected {
            return Err(invalid());
        }
        match (&pending.entries[index], pending.queries.earlier[index]) {
            (Some(PendingBatchMemory::Existing(memory)), None) => validate_recent_direct_write_hit(
                snapshot,
                &command.subject_id,
                &command.session_source_id,
                command.occurred_at_ns,
                item,
                &hash,
                memory,
            )?,
            (Some(PendingBatchMemory::Earlier(actual)), Some(expected)) if *actual == expected => {}
            (None, None) if pending.queries.review_to_item.contains(&index) => {
                let vector = &pending.embeddings[index];
                if vector.len() != snapshot.embedding_dimension as usize
                    || vector.iter().any(|value| !value.is_finite())
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
    }
    for (index, item) in pending.queries.review_to_item.iter().copied().enumerate() {
        let group = pending.queries.query_plan.caller_to_unique[index];
        let first = pending
            .queries
            .query_plan
            .caller_to_unique
            .iter()
            .position(|candidate| *candidate == group)
            .ok_or_else(invalid)?;
        if pending.embeddings[item] != pending.embeddings[pending.queries.review_to_item[first]] {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Revalidates cached query records and finite scores inside the retained visibility boundary.
/// 在保留的可见范围内重新校验缓存查询记录和有限分值；输入事实、查询范围和快照，不重新查询数据库。
fn validate_completed(
    facts: &[ReviewQueryFacts],
    work: &SearchWork,
    snapshot: &ConfigSnapshot,
) -> Result<(), CoreError> {
    for result in facts {
        let retrieval = &snapshot.memory_policy.retrieval_policy;
        let selected_contract = result.hard_dedupe_contract == retrieval.vector_contract.contract
            || retrieval
                .separate_channel_fallback
                .as_ref()
                .is_some_and(|fallback| {
                    result.hard_dedupe_contract == fallback.vector_contract.contract
                });
        let expected_threshold = retrieval_kernel::hard_duplicate_similarity(
            &retrieval.hard_dedupe,
            &result.hard_dedupe_contract,
        )
        .unwrap_or(0.99);
        if result.hits.len() > work.limit as usize
            || result.hard_dedupe_pool.len()
                > snapshot.memory_policy.retrieval_policy.candidate_pool_max as usize
            || !selected_contract
            || !result.hard_dedupe_threshold.is_finite()
            || !(0.0..=1.0).contains(&result.hard_dedupe_threshold)
            || result.hard_dedupe_threshold.to_bits() != expected_threshold.to_bits()
        {
            return Err(effect_type_error(
                "completed review query exceeds its bounded result",
            ));
        }
        for memory in &result.hard_dedupe_pool {
            validate_candidate_memory_record(
                work,
                &memory.id,
                memory,
                snapshot.embedding_dimension,
            )?;
        }
        for hit in &result.hits {
            validate_candidate_memory_record(
                work,
                &hit.memory.id,
                &hit.memory,
                snapshot.embedding_dimension,
            )?;
            if !hit.search_score.is_finite()
                || !hit.final_score.is_finite()
                || hit.rerank_score.is_some_and(|score| !score.is_finite())
                || hit
                    .explanation
                    .as_ref()
                    .is_some_and(|explanation| !finite_explanation(explanation))
            {
                return Err(effect_type_error(
                    "completed review query has a non-finite score",
                ));
            }
        }
    }
    Ok(())
}

/// Checks every floating-point explanation field, including optional channels, instead of validating only the published score.
/// 校验解释中所有浮点字段及可选通道，而不只校验最终分值；输入解释，返回是否全部有限。
fn finite_explanation(explanation: &vulcan_memorymesh_core_contracts::ScoreExplanation) -> bool {
    explanation.final_score.is_finite()
        && [
            explanation.raw_vector_score,
            explanation.raw_lexical_score,
            explanation.vector_decision_score,
            explanation.lexical_decision_score,
            explanation.rrf_score,
            explanation.base_decision_score,
            explanation.decay_multiplier,
            explanation.context_delta,
            explanation.rerank_score,
            explanation.pre_review_score,
        ]
        .into_iter()
        .flatten()
        .all(f64::is_finite)
        && explanation
            .vector_channel
            .iter()
            .chain(&explanation.lexical_channel)
            .all(|channel| channel.rrf_contribution.is_finite())
}
