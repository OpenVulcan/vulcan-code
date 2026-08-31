//! Compaction generations, locked baselines, and publication atomicity regression tests.
//! 压缩代际、锁定基线及发布原子性回归测试。

use super::*;

/// The next compaction record trims unpublished candidates left over from a
/// failed runtime write, so an in-process failure can never make the next
/// compaction append on top of them (audit P0#7, plan 6.2/5.8.3).
/// 下一次压缩记录裁剪 runtime 写失败残留的未发布候选，同进程失败绝不可能使
/// 下一次压缩基于它们继续追加(审计 P0#7、计划 6.2/5.8.3)。
#[test]
fn next_record_trims_unpublished_candidates_after_runtime_failure() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    let occurred1 = doc
        .append_assistant_text(api, "c1", AssistantSemanticStatus::Completed, T0 + 1)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api, None, T0 + 2).unwrap();
    // First compaction publishes durably (confirmed by a full flush cycle).
    // 第一次压缩持久化发布(经完整 flush 周期确认)。
    let node1 = doc
        .record_compaction(
            CompactionCommitInput::new(
                api,
                occurred1,
                Seq::try_new(1).unwrap(),
                "c1",
                LockedBaselineProjection::empty(),
            ),
            T0 + 3,
        )
        .unwrap()
        .into_value();
    let candidate = doc.prepare_flush_candidates().unwrap().unwrap();
    doc.advance_durable(candidate.revision).unwrap();
    assert_eq!(doc.compactions.len(), 1);
    // Second compaction becomes an unpublished candidate: its runtime write
    // never confirms (simulated by not advancing).
    // 第二次压缩成为未发布候选：它的 runtime 写入从未确认(以不 advance 模拟)。
    let api2 = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 4,
        )
        .unwrap();
    let occurred2 = doc
        .append_assistant_text(api2, "c2", AssistantSemanticStatus::Completed, T0 + 5)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api2, None, T0 + 6).unwrap();
    let node2 = doc
        .record_compaction(
            CompactionCommitInput::new(
                api2,
                occurred2,
                occurred1,
                "c2",
                LockedBaselineProjection::empty(),
            ),
            T0 + 7,
        )
        .unwrap()
        .into_value();
    assert_eq!(doc.compactions.len(), 2);
    assert_eq!(doc.runtime_lock.current_compaction_id, Some(node2));
    // The next record trims the unpublished candidate before appending.
    // 下一次记录先裁剪未发布候选再追加。
    let api3 = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 8,
        )
        .unwrap();
    let occurred3 = doc
        .append_assistant_text(api3, "c3", AssistantSemanticStatus::Completed, T0 + 9)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api3, None, T0 + 10).unwrap();
    let node3 = doc
        .record_compaction(
            CompactionCommitInput::new(
                api3,
                occurred3,
                occurred2,
                "c3",
                LockedBaselineProjection::empty(),
            ),
            T0 + 11,
        )
        .unwrap()
        .into_value();
    let ids: Vec<u64> = doc
        .compactions
        .iter()
        .map(|node| node.compaction_id.value())
        .collect();
    assert_eq!(ids, vec![node1.value(), node3.value()]);
    assert_eq!(doc.runtime_lock.current_compaction_id, Some(node3));
}

/// An exact compaction commit retry returns the original node without changing
/// the document, while any retained-boundary drift is rejected atomically.
/// 精确压缩提交重试返回原节点且不修改文档；任何保留边界漂移都会被原子拒绝。
#[test]
fn compaction_commit_is_idempotent_and_rejects_drift() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("root", Vec::new(), T0)
        .unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::Compaction,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            T0 + 1,
        )
        .unwrap();
    let occurred_at_seq = doc
        .append_assistant_text(api, "summary", AssistantSemanticStatus::Completed, T0 + 2)
        .unwrap()
        .into_value();
    complete_ok(&mut doc, api, None, T0 + 3).unwrap();
    let input = CompactionCommitInput::new(
        api,
        occurred_at_seq,
        Seq::try_new(1).unwrap(),
        "summary",
        LockedBaselineProjection::empty(),
    );

    let committed = doc.record_compaction(input.clone(), T0 + 4).unwrap();
    assert!(matches!(committed, IdempotentMutation::Committed(_)));
    let committed_id = committed.into_value();
    let after_commit = doc.clone();
    let replayed = doc.record_compaction(input, T0 + 5).unwrap();
    assert_eq!(replayed, IdempotentMutation::Idempotent(committed_id));
    assert_eq!(doc, after_commit);

    let drifted = CompactionCommitInput::new(
        api,
        occurred_at_seq,
        Seq::try_new(2).unwrap(),
        "summary",
        LockedBaselineProjection::empty(),
    );
    assert!(matches!(
        doc.record_compaction(drifted, T0 + 6),
        Err(DomainError::ImmutableFieldMutation {
            entity: "compaction",
            field: "existing_node"
        })
    ));
    assert_eq!(doc, after_commit);
}

/// The context-compaction node mutation dirties only metadata: the timeline is
/// untouched, a running API keeps working, and clearing the override restores
/// model/default selection (audit P0#10).
/// 上下文压缩节点 mutation 只弄脏 metadata：时间线与运行中的 API 不受影响，
/// 清除覆盖后恢复模型或默认选择(审计 P0#10)。
#[test]
fn set_context_compaction_window_node_dirts_metadata_only() {
    let mut doc = new_document();
    doc.submit_root_input_fixture("a", Vec::new(), T0).unwrap();
    let api = doc
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
            0,
        )
        .unwrap();
    doc.append_assistant_text(api, "x", AssistantSemanticStatus::Streaming, T0 + 1)
        .unwrap();
    let dirty_before: Vec<DirtyFile> = doc.dirty_files().collect();
    doc.set_context_compaction_window_node_id(
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow2),
        T0,
    );
    assert_eq!(
        doc.metadata.config.context_compaction_window_node_id,
        Some(vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow2)
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(
        doc.dirty_files()
            .filter(|file| *file == DirtyFile::ActiveTail)
            .count()
            <= dirty_before
                .iter()
                .filter(|file| **file == DirtyFile::ActiveTail)
                .count(),
        "the mutation must not add new tail dirtiness"
    );
    assert!(
        doc.active_api().is_some(),
        "the running API must keep working"
    );
    doc.set_context_compaction_window_node_id(None, T0 + 1);
    assert_eq!(doc.metadata.config.context_compaction_window_node_id, None);
}

/// A failed compaction API cannot publish a context node even when it returned
/// text; node statistics must come from a successfully completed compaction API.
/// 失败的压缩 API 即使返回了文本也不能发布上下文节点；节点统计必须来自成功完成的压缩 API。
#[test]
fn failed_compaction_api_cannot_publish_node() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("compact", Vec::new(), T0)?;
    let api = doc.start_api(
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let occurred = doc
        .append_assistant_text(api, "summary", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    doc.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: usage(1, 1),
            status: ApiCallStatus::Failed,
            end_reason: Some(ApiEndReason::ProviderError),
            error: Some(terminal_error(ApiEndReason::ProviderError)),
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    )?;
    assert!(matches!(
        doc.record_compaction(
            CompactionCommitInput::new(
                api,
                occurred,
                Seq::try_new(1)?,
                "summary",
                LockedBaselineProjection::empty(),
            ),
            T0 + 3,
        ),
        Err(DomainError::ApiRequestMismatch {
            field: "compaction.status",
            ..
        })
    ));
    assert!(doc.compactions.is_empty());
    Ok(())
}

/// Fresh runtime state is explicit, idempotently initialized, and rejects APIs beforehand.
/// Fresh runtime 状态是显式的、可幂等初始化，并在初始化前拒绝 API。
#[test]
fn runtime_baseline_initialization_is_explicit_and_failure_atomic() -> Result<(), DomainError> {
    let mut doc = SessionDocument::new("fresh", "project", "title", T0);
    assert_eq!(
        doc.runtime_lock.initialization_state,
        RuntimeInitializationState::FreshUninitialized
    );
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let before_invalid = doc.clone();
    let invalid = PrepareAndStartApiInput {
        purpose: ApiPurposeKind::MainGeneration,
        request: ApiRequestSnapshot::new("openai", "gpt-5"),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: doc.segment_generation(),
            document_revision: doc.memory_revision(),
            retained_from_seq: Seq::try_new(1)?,
            through_seq: doc.events().last().expect("root input").seq,
            fingerprint_sha256: "invalid".to_string(),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms: T0 + 1,
    };
    assert!(matches!(
        doc.prepare_and_start_api(invalid),
        Err(DomainError::InvalidProviderProjection {
            field: "fingerprint_sha256"
        })
    ));
    assert_eq!(doc, before_invalid);

    let prepared = doc.prepare_and_start_api(PrepareAndStartApiInput {
        purpose: ApiPurposeKind::MainGeneration,
        request: ApiRequestSnapshot::new("openai", "gpt-5"),
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: doc.segment_generation(),
            document_revision: doc.memory_revision(),
            retained_from_seq: Seq::try_new(1)?,
            through_seq: doc.events().last().expect("root input").seq,
            fingerprint_sha256: "0".repeat(64),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms: T0 + 2,
    })?;
    let prepared = prepared.into_value();
    assert_eq!(
        doc.runtime_lock.initialization_state,
        RuntimeInitializationState::Initialized
    );
    assert_eq!(
        doc.active_api().expect("active api").api_call_id,
        prepared.api_call_id
    );
    assert_eq!(
        doc.metadata.config.active_request,
        ActiveRequestConfig {
            provider: Some(String::from("openai")),
            connection_id: None,
            model: Some(String::from("gpt-5")),
            thinking_level: None,
            temperature: None,
        }
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Runtime));
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));

    let before_conflict = doc.clone();
    assert!(matches!(
        doc.initialize_runtime_baseline(
            LockedBaselineProjection::project(vec![LockedBaselineSegmentInput {
                kind: LockedSourceKind::AgentsMd,
                node_id: "agents-project".to_string(),
                target: LockedPromptTarget::Developer,
                source_label: Some("AGENTS.md".to_string()),
                source_id: None,
                content: "rules".to_string(),
            }])?,
            T0 + 3,
        ),
        Err(DomainError::RuntimeBaselineAlreadyInitialized)
    ));
    assert_eq!(doc, before_conflict);
    Ok(())
}

/// Compaction allocation and truncation rebuild failures never publish partial mutations.
/// 压缩分配与截断重建失败绝不发布部分修改。
#[test]
fn compaction_and_truncation_failures_are_document_atomic() -> Result<(), DomainError> {
    let mut compaction = new_document();
    compaction.submit_root_input_fixture("compact", Vec::new(), T0)?;
    let api = compaction.start_api(
        ApiPurposeKind::Compaction,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    let occurred = compaction
        .append_assistant_text(api, "summary", AssistantSemanticStatus::Completed, T0 + 1)?
        .into_value();
    compaction.complete_api_group(
        api,
        ApiCompletion {
            request: ApiRequestSnapshot::new("openai", "gpt-5"),
            usage: usage(1, 1),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
            provider_source: None,
            completed_at_ms: T0 + 2,
        },
    )?;
    let (_, next_seq) = compaction.id_allocator.frontier();
    compaction.id_allocator = IdAllocator::from_frontier(JS_SAFE_INTEGER_MAX + 1, next_seq);
    let before_compaction = compaction.clone();
    assert!(
        compaction
            .record_compaction(
                CompactionCommitInput::new(
                    api,
                    occurred,
                    Seq::try_new(1)?,
                    "summary",
                    LockedBaselineProjection::empty(),
                ),
                T0 + 3,
            )
            .is_err()
    );
    assert_eq!(compaction, before_compaction);

    let mut truncation = new_document();
    truncation.submit_root_input_fixture("root", Vec::new(), T0)?;
    truncation.append_system_context(
        None,
        "context",
        SystemContextOperation::Upsert,
        SystemContextScope::Session,
        None,
        None,
        "context",
        T0 + 1,
    )?;
    truncation.events[0].seq = Seq::try_new(3)?;
    let before_truncation = truncation.clone();
    assert!(truncation.truncate_after(Seq::try_new(2)?, T0 + 2).is_err());
    assert_eq!(truncation, before_truncation);
    Ok(())
}
