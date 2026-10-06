
use super::*;
use vulcan_memorymesh_core::{
    ChatCompactBoundary, CommitReceipt, EntityBinding, EntityKind, EntitySlotId, ExportDataset,
    MemoryAccessClass, MemoryContextEvidence, MemoryId, MemoryRecord, MemoryScopeClass,
    MemoryStability, ProfileLevel, ProfileNode, ProfileNodeId, ProfileNodeState, ProfileSourceId,
    ProfileSourceKind, SubjectId, TimelineItem, Turn, TurnId, TurnStatus, VectorSpaceId,
    VersionedEntity,
};

/// Builds one empty storage snapshot with the requested version.
/// 使用指定版本构建一份空存储快照。
fn empty_snapshot(storage_version: u64) -> StorageSnapshot {
    StorageSnapshot {
        dataset: ExportDataset::empty(),
        storage_version,
        next_entity_sequence: 0,
        outcomes: Vec::new(),
        analysis_attempts: Vec::new(),
        memory_sessions: Vec::new(),
        noise_prototypes: Vec::new(),
        retention_trash_batches: Vec::new(),
        retention_cold_turn_jobs: Vec::new(),
    }
}

/// Builds one committed idempotency entry that references an exact memory identity.
/// 构建一条引用准确记忆身份的已提交幂等记录。
fn push_memory_ledger_entry(snapshot: &mut StorageSnapshot, memory_id: &str) {
    let operation_id = OperationId::new(format!("ledger-{memory_id}")).expect("operation id");
    let idempotency_key =
        IdempotencyKey::new(format!("ledger-key-{memory_id}")).expect("idempotency key");
    let request_fingerprint = format!("fp256:{}", "1".repeat(64));
    let outcome = MutationOutcome::Committed {
        storage_version: 1,
        chat_compact: None,
        receipt: CommitReceipt {
            operation_id: operation_id.clone(),
            idempotency_key: idempotency_key.clone(),
            request_fingerprint: request_fingerprint.clone(),
            entity_bindings: vec![EntityBinding {
                entity_slot_id: EntitySlotId::new("memory-slot").expect("entity slot"),
                entity_kind: EntityKind::Memory,
                domain_id: memory_id.to_owned(),
            }],
            committed_versions: vec![VersionedEntity {
                entity_kind: EntityKind::Memory,
                domain_id: memory_id.to_owned(),
                version: 1,
            }],
            side_effect_summary: String::from("memory committed"),
        },
    };
    let entry = serde_json::from_value(serde_json::json!({
        "operation_id": operation_id,
        "request_fingerprint": request_fingerprint,
        "outcome": outcome,
        "idempotency_key": idempotency_key,
    }))
    .expect("storage ledger entry");
    snapshot.outcomes.push(entry);
}

/// Builds one minimal canonical turn anchored to a native project.
/// 构建一个锚定到原生项目的最小规范回合。
fn turn(id: &str, project_id: u64, session_id: &str) -> Turn {
    Turn {
        id: TurnId::new(id).expect("turn id"),
        subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
        scope_id: project_scope(project_id).expect("project scope"),
        session_source_id: SessionSourceId::new(session_id).expect("session id"),
        timeline: Vec::new(),
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: 1,
        status: TurnStatus::Pending,
        user_input_kind: None,
        analysis_details: None,
    }
}

/// Builds one complete durable memory anchored to a native project and host session.
/// 构建一条锚定到原生项目与宿主会话的完整持久记忆。
fn memory(
    id: &str,
    project_id: u64,
    session_id: &str,
    source_turn_id: Option<&str>,
    source_kind: MemorySourceKind,
    memory_level: MemoryLevel,
) -> MemoryRecord {
    MemoryRecord {
        id: MemoryId::new(id).expect("memory id"),
        access_class: MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(SubjectId::new("vmm-subject:1").expect("subject id")),
        scope_id: project_scope(project_id).expect("project scope"),
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::General,
        summary: format!("summary-{id}"),
        content: format!("content-{id}"),
        source_kind,
        origin_session_source_id: Some(SessionSourceId::new(session_id).expect("session id")),
        priority: 20,
        stability: MemoryStability::Stable,
        tags: vec![String::from("tag")],
        evidence: Vec::new(),
        evidence_source: None,
        state: MemoryState::Active,
        importance: 0.2,
        confidence: Some(0.9),
        created_at_ns: 3_000_000,
        updated_at_ns: 4_000_000,
        expires_at_ns: None,
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        memory_level,
        refresh_weight: 1,
        dedupe_hash: None,
        recalled_count: 0,
        adopted_count: 0,
        cross_session_adopted_count: 0,
        decay_disabled: false,
        status_reason: None,
        last_adopted_at_ns: None,
        last_reinforced_at_ns: None,
        source_turn_id: source_turn_id.map(|value| TurnId::new(value).expect("source turn id")),
        version: 1,
        vector_space_id: None,
        embedding: None,
    }
}

/// Builds one active profile node in the requested stable scope.
/// 在指定稳定作用域中构建一个活动画像节点。
fn profile_node(id: &str, scope_id: ScopeId, content: &str) -> ProfileNode {
    ProfileNode {
        id: ProfileNodeId::new(id).expect("profile id"),
        subject_id: Some(SubjectId::new("vmm-subject:1").expect("subject id")),
        scope_id,
        content: content.to_owned(),
        confidence: Some(0.9),
        state: ProfileNodeState::Active,
        source_turn_ids: Vec::new(),
        source_kind: ProfileSourceKind::SystemSeed,
        source_id: None,
        evidence_source: None,
        priority: 60,
        profile_level: ProfileLevel::Stable,
        level_reason: None,
        refresh_weight: 0,
        effective_date_ns: Some(1),
        expires_at_ns: None,
        status_reason: None,
        superseded_by: None,
        version: 1,
        created_at_ns: 1,
        updated_at_ns: 1,
    }
}

/// Verifies vector status distinguishes current, legacy, and missing generations without counting recycled rows.
/// 验证向量状态可区分当前、旧版与缺失代次，且不会计入已回收记录。
#[test]
fn vector_status_reports_every_incompatible_active_memory() {
    let current = VectorSpaceId::new("current-space").expect("current vector space");
    let legacy = VectorSpaceId::new("legacy-space").expect("legacy vector space");
    let mut snapshot = empty_snapshot(7);
    let mut current_memory = memory(
        "current-memory",
        1,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    current_memory.vector_space_id = Some(current.clone());
    current_memory.embedding = Some(vec![1.0, 0.0]);
    let mut legacy_memory = memory(
        "legacy-memory",
        1,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    legacy_memory.vector_space_id = Some(legacy);
    legacy_memory.embedding = Some(vec![0.0, 1.0]);
    let missing_memory = memory(
        "missing-memory",
        2,
        "session-b",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    let mut recycled_memory = memory(
        "recycled-memory",
        2,
        "session-b",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    recycled_memory.state = MemoryState::Recycled;
    snapshot.dataset.memories = vec![
        current_memory,
        legacy_memory,
        missing_memory,
        recycled_memory,
    ];
    let mut recycle_batch = retention_trash_batch(
        String::from("vector-status-trash"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        100,
        RetentionReasonCode::TerminalMemoryRecycle,
    );
    let mut current_recycled = memory(
        "current-recycled-memory",
        2,
        "session-b",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    current_recycled.vector_space_id = Some(current.clone());
    current_recycled.embedding = Some(vec![1.0, 0.0]);
    recycle_batch.dataset.memories.push(current_recycled);
    recycle_batch.dataset.memories.push(memory(
        "missing-recycled-memory",
        2,
        "session-b",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    ));
    snapshot.retention_trash_batches.push(recycle_batch);

    let status = vector_status_from_snapshot(&snapshot, Some(&current), Some(2), 3, None);

    assert!(status.configured);
    assert_eq!(status.active_memory_count, 3);
    assert_eq!(status.recycle_bin_memory_count, 2);
    assert_eq!(status.current_memory_count, 1);
    assert_eq!(status.current_recycle_bin_memory_count, 1);
    assert_eq!(status.active_rebuild_required_memory_count, 2);
    assert_eq!(status.recycle_bin_rebuild_required_memory_count, 1);
    assert_eq!(status.rebuild_required_memory_count, 3);
    assert_eq!(status.legacy_vector_space_count, 1);
    assert_eq!(status.pending_analysis_job_count, 3);
}

/// Verifies an unavailable route does not mislabel every stored vector as an old generation.
/// 验证路由不可用时不会把每条已存向量误标为旧代次。
#[test]
fn vector_status_keeps_route_failure_distinct_from_generation_mismatch() {
    let mut snapshot = empty_snapshot(8);
    snapshot.dataset.memories.push(memory(
        "unknown-generation-memory",
        1,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    ));

    let status = vector_status_from_snapshot(
        &snapshot,
        None,
        None,
        0,
        Some(String::from("embedding route unavailable")),
    );

    assert!(!status.configured);
    assert_eq!(status.active_memory_count, 1);
    assert_eq!(status.rebuild_required_memory_count, 0);
    assert_eq!(status.legacy_vector_space_count, 0);
    assert_eq!(status.error.as_deref(), Some("embedding route unavailable"));
}

/// Verifies rebuild previews are deterministic, project-aware, and split only at Core's 256-record limit.
/// 验证重建预检具有确定性、感知项目，并且只按 Core 的 256 条上限切批。
#[test]
fn vector_rebuild_preview_is_deterministic_and_bounded() {
    let target = VectorSpaceId::new("target-space").expect("target vector space");
    let descriptor = NativeVmmVectorSpaceDescriptor {
        vector_space_id: target.clone(),
        embedding_dimension: 1_536,
        capability_fingerprint: String::from("capability-fingerprint"),
    };
    let mut snapshot = empty_snapshot(21);
    for index in 0..257 {
        snapshot.dataset.memories.push(memory(
            &format!("project-one-memory-{index:03}"),
            1,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Stable,
        ));
    }
    snapshot.dataset.memories.push(memory(
        "project-two-memory",
        2,
        "session-b",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    ));
    let mut current_memory = memory(
        "already-current",
        2,
        "session-b",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    current_memory.vector_space_id = Some(target);
    current_memory.embedding = Some(vec![1.0; 1_536]);
    snapshot.dataset.memories.push(current_memory);
    let mut recycle_batch = retention_trash_batch(
        String::from("vector-preview-trash"),
        native_vmm_subject_id().expect("subject"),
        project_scope(1).expect("scope"),
        1,
        100,
        RetentionReasonCode::TerminalMemoryRecycle,
    );
    recycle_batch.dataset.memories.push(memory(
        "recycle-bin-old-generation",
        1,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    ));
    snapshot.retention_trash_batches.push(recycle_batch);

    let first = vector_rebuild_preview(&snapshot, &descriptor).expect("first preview");
    snapshot.dataset.memories.reverse();
    let reordered = vector_rebuild_preview(&snapshot, &descriptor).expect("reordered preview");

    assert_eq!(first, reordered);
    assert_eq!(first.active_memory_count, 259);
    assert_eq!(first.recycle_bin_memory_count, 1);
    assert_eq!(first.active_rebuild_memory_count, 258);
    assert_eq!(first.recycle_bin_rebuild_memory_count, 1);
    assert_eq!(first.rebuild_memory_count, 259);
    assert_eq!(first.affected_project_count, 2);
    assert_eq!(first.batch_count, 4);
    assert!(first.preview_fingerprint.starts_with("sha256:"));
}

/// Verifies recycle-bin vector migration changes only the retained vector generation.
/// 验证回收站向量迁移只修改所保留的向量代次。
#[test]
fn recycle_bin_vector_batch_preserves_recoverable_record_placement() {
    let subject = native_vmm_subject_id().expect("subject");
    let scope = project_scope(2).expect("scope");
    let target = VectorSpaceId::new("current-space").expect("vector space");
    let memory_id = MemoryId::new("recycled-vector-memory").expect("memory id");
    let mut snapshot = empty_snapshot(9);
    let mut batch = retention_trash_batch(
        String::from("vector-rebuild-trash"),
        subject.clone(),
        scope.clone(),
        1,
        100,
        RetentionReasonCode::TerminalMemoryRecycle,
    );
    let retained = memory(
        memory_id.as_str(),
        2,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    let original_state = retained.state;
    let original_updated_at_ns = retained.updated_at_ns;
    batch.dataset.memories.push(retained);
    snapshot.retention_trash_batches.push(batch);

    let summaries = recycle_bin_vector_summaries(
        &snapshot,
        &subject,
        &scope,
        std::slice::from_ref(&memory_id),
        &target,
    )
    .expect("summaries");
    let result = apply_recycle_bin_vector_batch(
        &mut snapshot,
        &subject,
        &scope,
        std::slice::from_ref(&memory_id),
        &target,
        2,
        vec![vec![0.25, 0.75]],
    )
    .expect("rebuild recycle vector");

    assert_eq!(summaries.len(), 1);
    assert!(snapshot.dataset.memories.is_empty());
    assert_eq!(result.rebuilt_memory_ids, vec![memory_id.to_string()]);
    let retained = &snapshot.retention_trash_batches[0].dataset.memories[0];
    assert_eq!(retained.vector_space_id.as_ref(), Some(&target));
    assert_eq!(retained.embedding.as_deref(), Some([0.25, 0.75].as_slice()));
    assert_eq!(retained.state, original_state);
    assert_eq!(retained.updated_at_ns, original_updated_at_ns);
}

/// Verifies audit cleanup preserves hot, recoverable, active, and no-turn idempotency rows.
/// 验证审计清理会保留活动区、可恢复、活动态以及无回合的幂等记录。
#[test]
fn orphaned_terminal_analysis_cleanup_follows_permanent_turn_removal() {
    let mut snapshot = empty_snapshot(12);
    snapshot.dataset.turns.push(turn("vmm-turn-hot", 2, "hot"));
    let mut recycle_batch = retention_trash_batch(
        String::from("audit-trash"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        100,
        RetentionReasonCode::ColdTurnRecycle,
    );
    recycle_batch
        .dataset
        .turns
        .push(turn("vmm-turn-recoverable", 2, "cold"));
    snapshot.retention_trash_batches.push(recycle_batch);
    let audit_job =
        |operation_id: &str, turn_id: Option<&str>, state: &str| NativeVmmPendingAnalysisJob {
            operation_id: operation_id.to_owned(),
            request_fingerprint: format!("fp256:{}", "1".repeat(64)),
            payload_json: String::new(),
            intake_mutation_operation_id: format!("{operation_id}:intake"),
            turn_id: turn_id.map(str::to_owned),
            state: state.to_owned(),
            attempt_count: 0,
            next_attempt_at_unix_ms: 0,
            last_error: None,
        };
    let jobs = vec![
        audit_job(
            "hot-terminal",
            Some("vmm-turn-hot"),
            VMM_PENDING_ANALYSIS_COMPLETED,
        ),
        audit_job(
            "recoverable-terminal",
            Some("vmm-turn-recoverable"),
            VMM_PENDING_ANALYSIS_COMPLETED,
        ),
        audit_job(
            "orphan-completed",
            Some("vmm-turn-gone-a"),
            VMM_PENDING_ANALYSIS_COMPLETED,
        ),
        audit_job(
            "orphan-bypassed",
            Some("vmm-turn-gone-b"),
            VMM_PENDING_ANALYSIS_BYPASSED,
        ),
        audit_job(
            "active-ready",
            Some("vmm-turn-gone-c"),
            vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_READY,
        ),
        audit_job(
            "terminal-without-turn",
            None,
            VMM_PENDING_ANALYSIS_COMPLETED,
        ),
    ];

    assert_eq!(
        orphaned_terminal_analysis_operation_ids(&snapshot, &jobs),
        vec![
            String::from("orphan-completed"),
            String::from("orphan-bypassed")
        ]
    );
}

/// Verifies activity derives from successful extraction commits without inventing timestamps or crossing projects.
/// 验证活动取自成功提炼提交，不伪造时间或跨越项目。
#[test]
fn memory_records_include_authoritative_extraction_activity() {
    // Include one normal, one bodyless and one never-completed session, plus another project's newer commit.
    // 包含普通、正文已移除和尚未成功的三个会话，以及其他项目更新的提交。
    let mut snapshot = empty_snapshot(12);
    let mut source_turn = turn("vmm-turn-11", 2, "session-with-turn");
    source_turn.created_at_ns = 1_000_000;
    snapshot.dataset.turns.push(source_turn);
    for (project_id, session_id, completed_at_ns) in [
        (2, "session-with-turn", Some(8_000_000)),
        (2, "session-bodyless", Some(6_000_000)),
        (2, "session-pending", None),
        (3, "session-with-turn", Some(100_000_000)),
    ] {
        snapshot
            .memory_sessions
            .push(vulcan_memorymesh_core::StorageMemorySession {
                subject_id: native_vmm_subject_id().expect("fixed subject"),
                status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                scope_id: project_scope(project_id).expect("scope"),
                session_source_id: SessionSourceId::new(session_id).expect("session"),
                last_extraction_observed_at_ns: 0,
                last_extraction_completed_at_ns: completed_at_ns,
            });
    }
    let before = serde_json::to_value(&snapshot).expect("before read");
    let records = build_memory_records(&snapshot, 2, None, &[]).expect("records");
    assert_eq!(records.sessions.len(), 3);
    assert_eq!(records.sessions[0].session_source_id, "session-with-turn");
    assert_eq!(records.sessions[0].first_activity_at_unix_ms, Some(1));
    assert_eq!(records.sessions[0].last_activity_at_unix_ms, Some(8));
    assert_eq!(records.sessions[1].session_source_id, "session-bodyless");
    assert_eq!(records.sessions[1].turn_count, 0);
    assert_eq!(records.sessions[1].memory_count, 0);
    assert_eq!(records.sessions[1].first_activity_at_unix_ms, Some(6));
    assert_eq!(records.sessions[1].last_activity_at_unix_ms, Some(6));
    assert_eq!(records.sessions[2].session_source_id, "session-pending");
    assert_eq!(records.sessions[2].first_activity_at_unix_ms, None);
    assert_eq!(records.sessions[2].last_activity_at_unix_ms, None);
    assert_eq!(serde_json::to_value(&snapshot).expect("after read"), before);
}

/// Verifies session browsing exposes extraction states, retries, levels, and memory provenance.
/// 验证会话浏览会暴露提取状态、重试、等级与记忆来源。
#[test]
fn memory_records_expose_turn_extraction_and_generated_memories() {
    let mut snapshot = empty_snapshot(12);
    let mut completed = turn("vmm-turn-11", 2, "session-a");
    completed.status = TurnStatus::Completed;
    completed.created_at_ns = 1_000_000;
    completed.analysis_details = Some(String::from("two candidates accepted"));
    completed.timeline = vec![
        TimelineItem {
            role: Role::User,
            content: String::from("remember apples"),
            occurred_at_ns: 1_000_000,
        },
        TimelineItem {
            role: Role::Assistant,
            content: String::from("noted"),
            occurred_at_ns: 2_000_000,
        },
    ];
    let mut pending = turn("vmm-turn-12", 2, "session-a");
    pending.created_at_ns = 5_000_000;
    snapshot.dataset.turns = vec![completed, pending, turn("vmm-turn-99", 3, "other")];
    snapshot.dataset.memories = vec![
        memory(
            "vmm-memory-21",
            2,
            "session-a",
            Some("vmm-turn-11"),
            MemorySourceKind::Turn,
            MemoryLevel::Stable,
        ),
        memory(
            "vmm-memory-22",
            2,
            "session-a",
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Phase,
        ),
        memory(
            "vmm-memory-99",
            3,
            "other",
            Some("vmm-turn-99"),
            MemorySourceKind::Turn,
            MemoryLevel::Persistent,
        ),
    ];
    let mut user_profile = profile_node(
        "vmm-profile-31",
        ScopeId::new("vmm-subject:1").expect("subject scope"),
        "likes apples",
    );
    user_profile.source_kind = ProfileSourceKind::TurnExtract;
    user_profile.source_id = Some(ProfileSourceId::new("vmm-turn-11").expect("source id"));
    user_profile.source_turn_ids = vec![TurnId::new("vmm-turn-11").expect("turn id")];
    let mut project_profile = profile_node(
        "vmm-profile-32",
        project_scope(2).expect("project scope"),
        "project uses apples",
    );
    project_profile.source_kind = ProfileSourceKind::TurnExtract;
    project_profile.source_id = Some(ProfileSourceId::new("vmm-turn-11").expect("source id"));
    project_profile.source_turn_ids = vec![TurnId::new("vmm-turn-11").expect("turn id")];
    let mut unrelated_profile = profile_node(
        "vmm-profile-99",
        project_scope(3).expect("foreign project scope"),
        "foreign profile",
    );
    unrelated_profile.source_kind = ProfileSourceKind::TurnExtract;
    unrelated_profile.source_id = Some(ProfileSourceId::new("vmm-turn-99").expect("source id"));
    unrelated_profile.source_turn_ids = vec![TurnId::new("vmm-turn-99").expect("turn id")];
    snapshot.dataset.profile_nodes = vec![user_profile, project_profile, unrelated_profile];
    let jobs = vec![
        NativeVmmPendingAnalysisJob {
            operation_id: String::from("operation-ready"),
            request_fingerprint: String::from("fingerprint-ready"),
            payload_json: String::from("{}"),
            intake_mutation_operation_id: String::from("intake-ready"),
            turn_id: Some(String::from("vmm-turn-12")),
            state: String::from("ready"),
            attempt_count: 2,
            next_attempt_at_unix_ms: 8,
            last_error: Some(String::from("provider timeout")),
        },
        NativeVmmPendingAnalysisJob {
            operation_id: String::from("operation-completed"),
            request_fingerprint: String::from("fingerprint-completed"),
            payload_json: String::new(),
            intake_mutation_operation_id: String::from("intake-completed"),
            turn_id: Some(String::from("vmm-turn-11")),
            state: String::from("completed"),
            attempt_count: 3,
            next_attempt_at_unix_ms: 0,
            last_error: None,
        },
    ];

    let records = build_memory_records(&snapshot, 2, Some("session-a"), &jobs)
        .expect("native memory records");

    assert_eq!(records.storage_version, 12);
    assert_eq!(records.sessions.len(), 1);
    assert_eq!(records.sessions[0].turn_count, 2);
    assert_eq!(records.sessions[0].turn_statuses.completed, 1);
    assert_eq!(records.sessions[0].turn_statuses.pending, 1);
    assert_eq!(records.sessions[0].memory_count, 2);
    assert_eq!(records.sessions[0].profile_node_count, 2);
    assert_eq!(records.sessions[0].direct_memory_count, 1);
    assert_eq!(records.sessions[0].memory_levels.phase, 1);
    assert_eq!(records.sessions[0].memory_levels.stable, 1);
    assert_eq!(records.turns.len(), 2);
    assert_eq!(records.turns[0].turn_id, 12);
    assert_eq!(records.turns[0].previous_turn_ids, vec![11]);
    assert!(records.turns[0].next_turn_ids.is_empty());
    assert_eq!(records.turns[0].extracted, Some(false));
    assert_eq!(records.turns[0].analysis_attempt_count, 2);
    assert_eq!(
        records.turns[0].analysis_last_error.as_deref(),
        Some("provider timeout")
    );
    assert_eq!(records.turns[1].turn_id, 11);
    assert!(records.turns[1].previous_turn_ids.is_empty());
    assert_eq!(records.turns[1].next_turn_ids, vec![12]);
    assert_eq!(records.turns[1].extracted, Some(true));
    assert_eq!(
        records.turns[1].analysis_job_state.as_deref(),
        Some("completed")
    );
    assert_eq!(records.turns[1].analysis_attempt_count, 3);
    assert_eq!(records.turns[1].generated_memory_ids, vec![21]);
    assert_eq!(records.turns[1].generated_profile_node_ids, vec![31, 32]);
    assert_eq!(records.turns[1].timeline.len(), 2);
    assert_eq!(records.memories.len(), 2);
    assert_eq!(records.memories[0].memory_id, 22);
    assert_eq!(records.memories[0].source_kind, "direct");
    assert_eq!(records.memories[1].source_turn_id, Some(11));
    assert_eq!(records.profile_nodes.len(), 2);
    assert_eq!(records.profile_nodes[0].target, "project");
    assert_eq!(records.profile_nodes[1].target, "user");
}

/// Verifies native profile inspection exposes pending and date-derived states without activating them.
/// 验证原生画像检查会暴露待处理及日期推导状态，且不会错误激活这些节点。
#[test]
fn profile_records_expose_every_effective_lifecycle_state() {
    let mut snapshot = empty_snapshot(14);
    let subject_scope = native_vmm_subject_scope().expect("subject scope");
    let mut pending = profile_node("vmm-profile-11", subject_scope.clone(), "pending user");
    pending.state = ProfileNodeState::Pending;
    pending.updated_at_ns = 8_000_000;
    let mut elapsed = profile_node("vmm-profile-12", subject_scope.clone(), "expired user");
    elapsed.expires_at_ns = Some(5_000_000);
    elapsed.updated_at_ns = 7_000_000;
    let mut scheduled = profile_node("vmm-profile-13", subject_scope.clone(), "future user");
    scheduled.effective_date_ns = Some(20_000_000);
    scheduled.updated_at_ns = 6_000_000;
    let mut foreign_subject = profile_node("vmm-profile-14", subject_scope, "foreign user");
    foreign_subject.subject_id = Some(SubjectId::new("vmm-subject:2").expect("subject id"));
    let project = profile_node(
        "vmm-profile-21",
        project_scope(2).expect("project scope"),
        "active project",
    );
    snapshot.dataset.profile_nodes = vec![pending, elapsed, scheduled, foreign_subject, project];

    let user_records =
        build_profile_records(&snapshot, 2, NativeVmmProfileTarget::User, 10_000_000)
            .expect("user profile records");
    assert_eq!(user_records.storage_version, 14);
    assert_eq!(user_records.target, "user");
    assert_eq!(user_records.nodes.len(), 3);
    assert_eq!(user_records.counts.pending, 1);
    assert_eq!(user_records.counts.expired, 1);
    assert_eq!(user_records.counts.scheduled, 1);
    assert_eq!(user_records.nodes[0].effective_state, "pending");
    assert_eq!(user_records.nodes[1].effective_state, "scheduled");
    assert_eq!(user_records.nodes[2].stored_state, "active");
    assert_eq!(user_records.nodes[2].effective_state, "expired");
    assert!(
        user_records
            .nodes
            .iter()
            .all(|node| !node.active_for_injection)
    );

    let project_records =
        build_profile_records(&snapshot, 2, NativeVmmProfileTarget::Project, 10_000_000)
            .expect("project profile records");
    assert_eq!(project_records.target, "project");
    assert_eq!(project_records.counts.active, 1);
    assert_eq!(project_records.nodes.len(), 1);
    assert!(project_records.nodes[0].active_for_injection);
}

/// Verifies that project-level browsing returns every project record, including sessionless memories.
/// 验证项目级浏览返回项目全部记录，包括没有会话归属的记忆。
#[test]
fn project_memory_records_include_sessionless_project_memories() {
    let mut snapshot = empty_snapshot(13);
    let mut completed = turn("vmm-turn-31", 2, "session-a");
    completed.status = TurnStatus::Completed;
    let session_memory = memory(
        "vmm-memory-41",
        2,
        "session-a",
        Some("vmm-turn-31"),
        MemorySourceKind::Turn,
        MemoryLevel::Stable,
    );
    let mut project_memory = memory(
        "vmm-memory-42",
        2,
        "unused-session",
        None,
        MemorySourceKind::SystemSeed,
        MemoryLevel::Persistent,
    );
    project_memory.origin_session_source_id = None;
    snapshot.dataset.turns = vec![completed, turn("vmm-turn-99", 3, "other")];
    snapshot.dataset.memories = vec![
        session_memory,
        project_memory,
        memory(
            "vmm-memory-99",
            3,
            "other",
            Some("vmm-turn-99"),
            MemorySourceKind::Turn,
            MemoryLevel::Persistent,
        ),
    ];

    let overview =
        build_memory_records(&snapshot, 2, None, &[]).expect("project-level native memory records");

    assert_eq!(overview.selected_session_source_id, None);
    assert_eq!(overview.sessions.len(), 1);
    assert_eq!(overview.turns.len(), 1);
    assert_eq!(overview.turns[0].turn_id, 31);
    assert_eq!(overview.memories.len(), 2);
    assert!(
        overview
            .memories
            .iter()
            .any(|memory| { memory.memory_id == 42 && memory.session_source_id.is_none() })
    );

    let session = build_memory_records(&snapshot, 2, Some("session-a"), &[])
        .expect("session-level native memory records");
    assert_eq!(session.turns.len(), 1);
    assert_eq!(session.memories.len(), 1);
    assert_eq!(session.memories[0].memory_id, 41);
}

/// Verifies preview reports exact counts and the first globally free suffix.
/// 验证预检报告精确计数与首个全局可用后缀。
#[test]
fn migration_preview_detects_session_conflicts_without_mutating_source() {
    let mut snapshot = empty_snapshot(7);
    snapshot.dataset.turns = vec![
        turn("source-turn", 2, "session-a"),
        turn("target-turn", 3, "session-a"),
        turn("other-turn", 4, "session-a-1"),
    ];

    let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

    assert_eq!(preview.storage_version, 7);
    assert_eq!(preview.source.sessions, 1);
    assert_eq!(preview.source.turns, 1);
    assert_eq!(preview.target_before.turns, 1);
    assert_eq!(preview.target_after.turns, 2);
    assert_eq!(preview.records.len(), 2);
    assert_eq!(preview.records[0].record_kind, "session");
    assert_eq!(preview.records[0].record_id, "session-a");
    assert_eq!(preview.records[1].record_kind, "turn");
    assert_eq!(preview.records[1].record_id, "source-turn");
    assert!(preview.preview_fingerprint.starts_with("sha256:"));
    assert_eq!(preview.preview_fingerprint.len(), 71);
    assert_eq!(
        preview.session_renames,
        vec![NativeVmmSessionRename {
            from: String::from("session-a"),
            to: String::from("session-a-2"),
        }]
    );
    assert_eq!(
        snapshot.dataset.turns[0].scope_id,
        project_scope(2).expect("source project scope")
    );

    let mut changed = snapshot.clone();
    changed.dataset.turns[0].analysis_details = Some(String::from("changed detail"));
    let changed_preview = preview_migration(&changed, 2, 3).expect("changed preview");
    assert_ne!(
        preview.preview_fingerprint,
        changed_preview.preview_fingerprint
    );
}

/// Verifies body-free session records participate in migration preview, renaming and clear.
/// 验证没有正文的会话记录参与迁移预检、改名和清空。
#[test]
fn migration_moves_session_watermarks_and_clear_resets_only_the_owner_project() {
    // snapshot keeps independent windows, even though source and target reuse the same session ID.
    // snapshot 保留独立窗口，即使来源和目标复用了同一会话 ID。
    let mut snapshot = empty_snapshot(7);
    for (project_id, id, observed, completed) in [
        (2, "session-a", 20, 25),
        (3, "session-a", 40, 45),
        (4, "session-a-1", 60, 65),
    ] {
        snapshot
            .memory_sessions
            .push(vulcan_memorymesh_core::StorageMemorySession {
                subject_id: SubjectId::new("vmm-subject:1").expect("subject"),
                status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                scope_id: project_scope(project_id).expect("scope"),
                session_source_id: SessionSourceId::new(id).expect("session"),
                last_extraction_observed_at_ns: observed,
                last_extraction_completed_at_ns: Some(completed),
            });
    }
    // preview counts metadata-only sessions and reserves IDs owned by every other project.
    // preview 统计仅有元数据的会话，并保留所有其他项目占用的 ID。
    let preview = preview_migration(&snapshot, 2, 3).expect("preview");
    assert_eq!(preview.source.sessions, 1);
    assert_eq!(preview.target_before.sessions, 1);
    assert_eq!(preview.target_after.sessions, 2);
    assert_eq!(preview.session_renames[0].to, "session-a-2");
    apply_migration(&mut snapshot, &preview).expect("move");
    assert!(project_sessions(&snapshot, &project_scope(2).expect("source")).is_empty());
    // moved retains its own successful boundary instead of inheriting the target's boundary.
    // moved 保留自己的成功边界，而不是继承目标的边界。
    let moved = snapshot
        .memory_sessions
        .iter()
        .find(|session| session.session_source_id.as_str() == "session-a-2")
        .expect("moved session");
    assert_eq!(moved.scope_id, project_scope(3).expect("target"));
    assert_eq!(moved.last_extraction_observed_at_ns, 20);
    assert_eq!(moved.last_extraction_completed_at_ns, Some(25));
    assert_eq!(
        retention_session_latest_activity_ns(
            &snapshot,
            &moved.subject_id,
            &moved.scope_id,
            &moved.session_source_id
        ),
        Some(25)
    );
    assert!(vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot.clone()).is_ok());
    clear_project(&mut snapshot, 3).expect("clear target");
    assert_eq!(snapshot.memory_sessions.len(), 1);
    assert_eq!(
        snapshot.memory_sessions[0].scope_id,
        project_scope(4).expect("unrelated project")
    );
    assert_eq!(
        snapshot.memory_sessions[0].last_extraction_observed_at_ns,
        60
    );
}

/// Verifies migration moves every project record without rebuilding vectors or moving the user profile.
/// 验证迁移会移动全部项目记录，且不重建向量、不移动用户画像。
#[test]
fn migration_moves_project_records_and_preserves_vector_identity() {
    let mut snapshot = empty_snapshot(1);
    snapshot.dataset.turns = vec![
        turn("source-turn", 2, "session-a"),
        turn("target-turn", 3, "session-a"),
    ];
    let mut source_memory = memory(
        "vmm-memory-1",
        2,
        "session-a",
        Some("source-turn"),
        MemorySourceKind::Turn,
        MemoryLevel::Stable,
    );
    source_memory.vector_space_id = Some(VectorSpaceId::new("vmm-default").expect("vector space"));
    source_memory.embedding = Some(vec![0.25, 0.75]);
    snapshot.dataset.memories = vec![source_memory];
    snapshot.dataset.profile_nodes = vec![
        profile_node(
            "vmm-profile-1",
            project_scope(2).expect("source project scope"),
            "project preference",
        ),
        profile_node(
            "vmm-profile-2",
            ScopeId::new("vmm-subject:1").expect("subject scope"),
            "user preference",
        ),
    ];
    snapshot.dataset.chat_compact_boundaries = vec![ChatCompactBoundary {
        subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
        scope_id: project_scope(2).expect("source project scope"),
        session_source_id: SessionSourceId::new("session-a").expect("session id"),
        compacted_turn_id: TurnId::new("source-turn").expect("turn id"),
        compacted_at_ns: 2,
    }];
    let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

    apply_migration(&mut snapshot, &preview).expect("apply migration");

    assert_eq!(
        snapshot.dataset.turns[0].scope_id,
        project_scope(3).expect("target project scope")
    );
    assert_eq!(
        snapshot.dataset.turns[0].session_source_id.as_str(),
        "session-a-1"
    );
    assert_eq!(
        snapshot.dataset.turns[1].session_source_id.as_str(),
        "session-a"
    );
    assert_eq!(snapshot.dataset.memories[0].id.as_str(), "vmm-memory-1");
    assert_eq!(
        snapshot.dataset.memories[0].scope_id,
        project_scope(3).expect("target project scope")
    );
    assert_eq!(
        snapshot.dataset.memories[0]
            .origin_session_source_id
            .as_ref()
            .map(SessionSourceId::as_str),
        Some("session-a-1")
    );
    assert_eq!(
        snapshot.dataset.memories[0]
            .vector_space_id
            .as_ref()
            .map(VectorSpaceId::as_str),
        Some("vmm-default")
    );
    assert_eq!(
        snapshot.dataset.memories[0].embedding,
        Some(vec![0.25, 0.75])
    );
    assert_eq!(
        snapshot.dataset.profile_nodes[0].scope_id,
        project_scope(3).expect("target project scope")
    );
    assert_eq!(
        snapshot.dataset.profile_nodes[1].scope_id,
        ScopeId::new("vmm-subject:1").expect("subject scope")
    );
    assert_eq!(
        snapshot.dataset.chat_compact_boundaries[0].scope_id,
        project_scope(3).expect("target project scope")
    );
    assert_eq!(
        snapshot.dataset.chat_compact_boundaries[0]
            .session_source_id
            .as_str(),
        "session-a-1"
    );
    assert_eq!(
        project_content_counts(&snapshot, &project_scope(2).expect("source project scope")),
        NativeVmmProjectContentCounts::default()
    );
}

/// Verifies migration preview and conflict renaming include durable recycle-bin sessions.
/// 验证迁移预检与冲突改名覆盖耐久回收站中的会话。
#[test]
fn migration_preview_includes_recycled_sessions_and_records() {
    let mut snapshot = empty_snapshot(4);
    let source_scope = project_scope(2).expect("source scope");
    let target_scope = project_scope(3).expect("target scope");
    snapshot.retention_trash_batches = vec![
        StorageRetentionTrashBatch {
            batch_id: String::from("source-trash"),
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: source_scope.clone(),
            recycled_at_ns: 1,
            expires_at_ns: 100,
            reason: RetentionReasonCode::ColdTurnRecycle,
            manual_selection: None,
            dataset: ExportDataset {
                turns: vec![turn("source-trash-turn", 2, "recycled-session")],
                ..ExportDataset::empty()
            },
            analysis_attempts: Vec::new(),
        },
        StorageRetentionTrashBatch {
            batch_id: String::from("target-trash"),
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: target_scope.clone(),
            recycled_at_ns: 1,
            expires_at_ns: 100,
            reason: RetentionReasonCode::ColdTurnRecycle,
            manual_selection: None,
            dataset: ExportDataset {
                turns: vec![turn("target-trash-turn", 3, "recycled-session")],
                ..ExportDataset::empty()
            },
            analysis_attempts: Vec::new(),
        },
    ];

    let preview = preview_migration(&snapshot, 2, 3).expect("migration preview");

    assert_eq!(preview.source.sessions, 1);
    assert_eq!(preview.source.turns, 1);
    assert_eq!(preview.target_before.sessions, 1);
    assert_eq!(preview.target_after.sessions, 2);
    assert_eq!(preview.records.len(), 2);
    assert_eq!(
        preview.session_renames,
        vec![NativeVmmSessionRename {
            from: String::from("recycled-session"),
            to: String::from("recycled-session-1"),
        }]
    );

    apply_migration(&mut snapshot, &preview).expect("apply migration");
    assert_eq!(snapshot.retention_trash_batches[0].scope_id, target_scope);
    assert_eq!(
        snapshot.retention_trash_batches[0].dataset.turns[0]
            .session_source_id
            .as_str(),
        "recycled-session-1"
    );
    assert_eq!(
        project_content_counts(&snapshot, &source_scope),
        NativeVmmProjectContentCounts::default()
    );
}

/// Verifies clear removes all selected project data and preserves the fixed personal profile.
/// 验证清空会删除所选项目全部数据并保留固定个人画像。
#[test]
fn clear_removes_project_records_and_preserves_personal_profile() {
    let mut snapshot = empty_snapshot(3);
    snapshot.dataset.turns = vec![
        turn("source-turn", 2, "session-a"),
        turn("other-turn", 3, "session-b"),
    ];
    let mut source_memory = memory(
        "vmm-memory-1",
        2,
        "session-a",
        Some("source-turn"),
        MemorySourceKind::Turn,
        MemoryLevel::Persistent,
    );
    source_memory.vector_space_id = Some(VectorSpaceId::new("vmm-default").expect("vector space"));
    source_memory.embedding = Some(vec![1.0, 0.0]);
    snapshot.dataset.memories = vec![source_memory];
    snapshot.dataset.context_evidence = vec![MemoryContextEvidence {
        memory_id: MemoryId::new("vmm-memory-1").expect("memory id"),
        context_key: String::from("workspace"),
        context_value: String::from("vulcan-code"),
        support_count: 1,
        rebuttal_count: 0,
        last_supported_at_ns: Some(1),
        last_rebutted_at_ns: None,
        version: 1,
    }];
    snapshot.dataset.profile_nodes = vec![
        profile_node(
            "vmm-profile-1",
            project_scope(2).expect("source project scope"),
            "project preference",
        ),
        profile_node(
            "vmm-profile-2",
            ScopeId::new("vmm-subject:1").expect("subject scope"),
            "user preference",
        ),
    ];
    snapshot.dataset.chat_compact_boundaries = vec![ChatCompactBoundary {
        subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
        scope_id: project_scope(2).expect("source project scope"),
        session_source_id: SessionSourceId::new("session-a").expect("session id"),
        compacted_turn_id: TurnId::new("source-turn").expect("turn id"),
        compacted_at_ns: 2,
    }];
    let mut recycled_memory = memory(
        "vmm-memory-recycled",
        2,
        "session-recycled",
        Some("recycled-turn"),
        MemorySourceKind::Turn,
        MemoryLevel::Stable,
    );
    recycled_memory.vector_space_id =
        Some(VectorSpaceId::new("vmm-default").expect("vector space"));
    recycled_memory.embedding = Some(vec![0.0, 1.0]);
    snapshot.retention_trash_batches = vec![StorageRetentionTrashBatch {
        batch_id: String::from("project-trash"),
        subject_id: native_vmm_subject_id().expect("subject"),
        scope_id: project_scope(2).expect("project scope"),
        recycled_at_ns: 1,
        expires_at_ns: 100,
        reason: RetentionReasonCode::IdleSessionRecycle,
        manual_selection: None,
        dataset: ExportDataset {
            turns: vec![turn("recycled-turn", 2, "session-recycled")],
            memories: vec![recycled_memory],
            profile_nodes: vec![profile_node(
                "vmm-profile-recycled",
                project_scope(2).expect("project scope"),
                "recycled project preference",
            )],
            context_evidence: vec![MemoryContextEvidence {
                memory_id: MemoryId::new("vmm-memory-recycled").expect("memory id"),
                context_key: String::from("workspace"),
                context_value: String::from("recycled"),
                support_count: 1,
                rebuttal_count: 0,
                last_supported_at_ns: Some(1),
                last_rebutted_at_ns: None,
                version: 1,
            }],
            chat_compact_boundaries: vec![ChatCompactBoundary {
                subject_id: SubjectId::new("vmm-subject:1").expect("subject id"),
                scope_id: project_scope(2).expect("project scope"),
                session_source_id: SessionSourceId::new("session-recycled").expect("session id"),
                compacted_turn_id: TurnId::new("recycled-turn").expect("turn id"),
                compacted_at_ns: 1,
            }],
        },
        analysis_attempts: Vec::new(),
    }];
    push_memory_ledger_entry(&mut snapshot, "vmm-memory-recycled");

    let removed = clear_project(&mut snapshot, 2).expect("clear project");

    assert_eq!(removed.sessions, 2);
    assert_eq!(removed.turns, 2);
    assert_eq!(removed.memories, 2);
    assert_eq!(removed.profile_nodes, 2);
    assert_eq!(removed.context_evidence, 2);
    assert_eq!(removed.compact_boundaries, 2);
    assert_eq!(removed.vector_rows, 2);
    assert_eq!(snapshot.dataset.turns.len(), 1);
    assert_eq!(snapshot.dataset.turns[0].id.as_str(), "other-turn");
    assert!(snapshot.dataset.memories.is_empty());
    assert!(snapshot.dataset.context_evidence.is_empty());
    assert!(snapshot.dataset.chat_compact_boundaries.is_empty());
    assert_eq!(snapshot.dataset.profile_nodes.len(), 1);
    assert_eq!(
        snapshot.dataset.profile_nodes[0].id.as_str(),
        "vmm-profile-2"
    );
    assert_eq!(
        snapshot.dataset.profile_nodes[0].scope_id,
        ScopeId::new("vmm-subject:1").expect("subject scope")
    );
    assert!(snapshot.retention_trash_batches.is_empty());
    assert!(snapshot.outcomes.is_empty());
    vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
        .expect("cleared snapshot remains valid");
}

/// Builds one minimal retention plan for direct snapshot-transform tests.
/// 为直接快照转换测试构建一份最小保留计划。
fn retention_plan(project_id: u64) -> RetentionPlanSet {
    RetentionPlanSet {
        identity: MutationIdentity {
            operation_id: OperationId::new("retention-test-operation").expect("operation id"),
            idempotency_key: IdempotencyKey::new("retention-test-key").expect("idempotency key"),
            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
            source_kind: MutationSourceKind::BackgroundWorker,
            request_source_id: String::from("retention-test"),
            observed_at_ns: 1,
        },
        subject_id: native_vmm_subject_id().expect("subject"),
        scope_id: project_scope(project_id).expect("project scope"),
        profile_expiry: Vec::new(),
        terminal_memory_recycle: Vec::new(),
        cold_turn_enqueue: Vec::new(),
        cold_turn_execute: Vec::new(),
        idle_session_recycle: Vec::new(),
        vector_gc_retry: Vec::new(),
        trash_purge: Vec::new(),
    }
}

/// Verifies terminal retention atomically preserves memory evidence in durable trash.
/// 验证终态保留会把记忆及证据原子保存在耐久回收站中。
#[test]
fn retention_recycles_terminal_memory_with_context_evidence() {
    // A non-default window must become part of the new durable batch, not a display-time guess.
    // 非默认窗口必须成为新耐久批次的一部分，不能在展示时推算。
    let mut policy = RetentionPolicy::v1();
    policy.trash_retention_ms = 12;
    let mut snapshot = empty_snapshot(5);
    let mut deleted = memory(
        "vmm-memory-terminal",
        2,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Phase,
    );
    deleted.state = MemoryState::Deleted;
    deleted.embedding = Some(vec![0.2, 0.8]);
    deleted.vector_space_id = Some(VectorSpaceId::new("vmm-default").expect("vector space"));
    snapshot.dataset.context_evidence = vec![MemoryContextEvidence {
        memory_id: deleted.id.clone(),
        context_key: String::from("workspace"),
        context_value: String::from("native"),
        support_count: 1,
        rebuttal_count: 0,
        last_supported_at_ns: Some(1),
        last_rebutted_at_ns: None,
        version: 1,
    }];
    let expected_access = MemoryAccessIdentity::from(&deleted);
    snapshot.dataset.memories.push(deleted);
    let mut plan = retention_plan(2);
    plan.terminal_memory_recycle
        .push(vulcan_memorymesh_core::PlannedMemoryRef {
            memory_id: String::from("vmm-memory-terminal"),
            expected_version: 1,
            expected_access,
            reason: RetentionReasonCode::TerminalMemoryRecycle,
        });

    let result = apply_retention_plan(&mut snapshot, &plan, &policy, 100, "retention-test")
        .expect("apply retention");

    assert_eq!(result.recycled_memories, 1);
    assert!(snapshot.dataset.memories.is_empty());
    assert!(snapshot.dataset.context_evidence.is_empty());
    assert_eq!(snapshot.retention_trash_batches.len(), 1);
    assert_eq!(snapshot.retention_trash_batches[0].recycled_at_ns, 100);
    assert_eq!(
        snapshot.retention_trash_batches[0].expires_at_ns,
        12_000_100
    );
    assert_eq!(
        snapshot.retention_trash_batches[0].dataset.memories.len(),
        1
    );
    assert_eq!(
        snapshot.retention_trash_batches[0]
            .dataset
            .context_evidence
            .len(),
        1
    );
    assert!(
        snapshot.without_embeddings().retention_trash_batches[0]
            .dataset
            .memories[0]
            .embedding
            .is_none()
    );
}

/// Verifies cold-turn jobs preserve profile sources and recycle only old unreferenced turns.
/// 验证冷回合任务会保护画像来源，仅回收旧的无引用回合。
#[test]
fn retention_cold_turn_job_preserves_profile_references_and_hot_window() {
    let mut snapshot = empty_snapshot(7);
    snapshot.dataset.turns = (1..=10)
        .map(|index| {
            let mut value = turn(format!("vmm-turn-{index}").as_str(), 2, "session-a");
            value.created_at_ns = index;
            value.status = TurnStatus::Completed;
            value
        })
        .collect();
    let mut profile = profile_node(
        "vmm-profile-source",
        project_scope(2).expect("project scope"),
        "protected source",
    );
    profile.source_turn_ids = vec![TurnId::new("vmm-turn-1").expect("turn id")];
    snapshot.dataset.profile_nodes.push(profile);

    let mut enqueue_plan = retention_plan(2);
    enqueue_plan
        .cold_turn_enqueue
        .push(vulcan_memorymesh_core::PlannedSessionRef {
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::ColdTurnRecycle,
        });
    let enqueued = apply_retention_plan(
        &mut snapshot,
        &enqueue_plan,
        &RetentionPolicy::v1(),
        100,
        "retention-enqueue",
    )
    .expect("enqueue cold job");
    assert_eq!(enqueued.enqueued_cold_jobs, 1);
    assert_eq!(snapshot.dataset.turns.len(), 10);

    let job_id = snapshot.retention_cold_turn_jobs[0].job_id.clone();
    let mut execute_plan = retention_plan(2);
    execute_plan
        .cold_turn_execute
        .push(vulcan_memorymesh_core::PlannedColdTurnJobRef {
            job_id,
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::ColdTurnRecycle,
        });
    let executed = apply_retention_plan(
        &mut snapshot,
        &execute_plan,
        &RetentionPolicy::v1(),
        100,
        "retention-execute",
    )
    .expect("execute cold job");

    assert_eq!(executed.completed_cold_jobs, 1);
    assert_eq!(executed.recycled_turns, 1);
    assert!(
        snapshot
            .dataset
            .turns
            .iter()
            .any(|value| value.id.as_str() == "vmm-turn-1")
    );
    assert!(
        !snapshot
            .dataset
            .turns
            .iter()
            .any(|value| value.id.as_str() == "vmm-turn-2")
    );
    assert_eq!(snapshot.retention_trash_batches.len(), 1);
    assert_eq!(
        snapshot.retention_trash_batches[0].dataset.turns[0]
            .id
            .as_str(),
        "vmm-turn-2"
    );
}

/// Verifies a due cold job and idle-session plan for the same session commit without rollback.
/// 验证同一会话的到期冷作业与闲置会话计划可以提交且不会回滚。
#[test]
fn retention_cold_job_and_idle_plan_share_one_atomic_pass() {
    let policy = RetentionPolicy::v1();
    let now_ns = retention_duration_ns(policy.session_idle_ms).expect("idle duration") + 100;
    let mut snapshot = empty_snapshot(7);
    snapshot.dataset.turns = (1..=10)
        .map(|index| {
            let mut value = turn(format!("vmm-turn-overlap-{index}").as_str(), 2, "session-a");
            value.created_at_ns = index;
            value.status = TurnStatus::Completed;
            value
        })
        .collect();
    snapshot
        .retention_cold_turn_jobs
        .push(StorageRetentionColdTurnJob {
            job_id: String::from("cold-job-overlap"),
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: project_scope(2).expect("scope"),
            session_source_id: SessionSourceId::new("session-a").expect("session id"),
            next_run_at_ns: 1,
        });
    let mut plan = retention_plan(2);
    plan.cold_turn_execute
        .push(vulcan_memorymesh_core::PlannedColdTurnJobRef {
            job_id: String::from("cold-job-overlap"),
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::ColdTurnRecycle,
        });
    plan.idle_session_recycle
        .push(vulcan_memorymesh_core::PlannedSessionRef {
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::IdleSessionRecycle,
        });

    let result = apply_retention_plan(&mut snapshot, &plan, &policy, now_ns, "retention-overlap")
        .expect("overlapping cold and idle plan");

    assert_eq!(result.completed_cold_jobs, 1);
    assert_eq!(result.recycled_turns, 2);
    assert!(snapshot.retention_cold_turn_jobs.is_empty());
    assert_eq!(snapshot.dataset.turns.len(), 8);
}

/// Verifies one retention pass materializes due project and fixed-user profiles as expired.
/// 验证一次保留运行会把到期的项目画像与固定用户画像物化为已过期状态。
#[test]
fn retention_materializes_project_and_user_profile_expiry() {
    let mut snapshot = empty_snapshot(9);
    let mut project_profile = profile_node(
        "vmm-profile-project-expired",
        project_scope(2).expect("project scope"),
        "project profile",
    );
    project_profile.expires_at_ns = Some(100);
    let mut user_profile = profile_node(
        "vmm-profile-user-expired",
        native_vmm_subject_scope().expect("subject scope"),
        "user profile",
    );
    user_profile.expires_at_ns = Some(90);
    snapshot.dataset.profile_nodes = vec![project_profile.clone(), user_profile.clone()];

    let mut plan = retention_plan(2);
    for node in [&project_profile, &user_profile] {
        plan.profile_expiry
            .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
                profile_node_id: node.id.clone(),
                expected_subject_id: node.subject_id.clone(),
                expected_scope_id: node.scope_id.clone(),
                expected_version: node.version,
                reason: RetentionReasonCode::ProfileExpiry,
            });
    }

    let result = apply_retention_plan(
        &mut snapshot,
        &plan,
        &RetentionPolicy::v1(),
        100,
        "retention-profile-expiry",
    )
    .expect("apply profile expiry");

    assert_eq!(result.expired_profile_nodes, 2);
    assert!(snapshot.dataset.profile_nodes.iter().all(|node| {
        node.state == ProfileNodeState::Expired && node.version == 2 && node.updated_at_ns == 100
    }));
}

/// Verifies project retention cannot expire a profile owned by another project scope.
/// 验证项目保留流程不能使另一个项目作用域拥有的画像过期。
#[test]
fn retention_rejects_cross_project_profile_expiry() {
    let mut snapshot = empty_snapshot(9);
    let mut foreign_profile = profile_node(
        "vmm-profile-foreign-project",
        project_scope(3).expect("foreign project scope"),
        "foreign project profile",
    );
    foreign_profile.expires_at_ns = Some(50);
    snapshot.dataset.profile_nodes.push(foreign_profile.clone());
    let mut plan = retention_plan(2);
    plan.profile_expiry
        .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
            profile_node_id: foreign_profile.id.clone(),
            expected_subject_id: foreign_profile.subject_id.clone(),
            expected_scope_id: foreign_profile.scope_id.clone(),
            expected_version: foreign_profile.version,
            reason: RetentionReasonCode::ProfileExpiry,
        });

    let error = apply_retention_plan(
        &mut snapshot,
        &plan,
        &RetentionPolicy::v1(),
        100,
        "retention-cross-project-profile",
    )
    .expect_err("cross-project profile expiry must fail");

    assert!(error.contains("已发生变化"));
    assert_eq!(
        snapshot.dataset.profile_nodes[0].state,
        ProfileNodeState::Active
    );
}

/// Verifies idle-session cleanup requires expiry before the idle cutoff, not merely before now.
/// 验证闲置会话清洗要求记忆在闲置截止点前已经过期，而不只是当前已经过期。
#[test]
fn idle_session_cleanup_respects_idle_cutoff_expiry() {
    let mut snapshot = empty_snapshot(10);
    let mut old_expiry = memory(
        "vmm-memory-old-expiry",
        2,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Session,
    );
    old_expiry.scope_class = MemoryScopeClass::Session;
    old_expiry.created_at_ns = 10;
    old_expiry.updated_at_ns = 10;
    old_expiry.expires_at_ns = Some(20);
    let mut recent_expiry = old_expiry.clone();
    recent_expiry.id = MemoryId::new("vmm-memory-recent-expiry").expect("memory id");
    recent_expiry.expires_at_ns = Some(80);
    snapshot.dataset.memories = vec![old_expiry, recent_expiry];

    let selected = idle_session_memory_ids(
        &snapshot,
        &native_vmm_subject_id().expect("subject"),
        &project_scope(2).expect("project scope"),
        &SessionSourceId::new("session-a").expect("session id"),
        50,
    );

    assert_eq!(
        selected,
        BTreeSet::from([String::from("vmm-memory-old-expiry")])
    );
}

/// Verifies a direct-write-only session can pass apply-time idle validation and be recycled.
/// 验证只有直接写入记忆的会话也能通过闲置应用时校验并进入回收站。
#[test]
fn retention_applies_idle_plan_to_direct_memory_only_session() {
    let mut policy = RetentionPolicy::v1();
    policy.session_idle_ms = 1;
    let now_ns = 2_000_000;
    let mut snapshot = empty_snapshot(11);
    let mut direct_memory = memory(
        "vmm-memory-direct-only",
        2,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Session,
    );
    direct_memory.scope_class = MemoryScopeClass::Session;
    direct_memory.created_at_ns = 10;
    direct_memory.updated_at_ns = 10;
    direct_memory.expires_at_ns = Some(20);
    snapshot.dataset.memories.push(direct_memory);
    let mut plan = retention_plan(2);
    plan.idle_session_recycle
        .push(vulcan_memorymesh_core::PlannedSessionRef {
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::IdleSessionRecycle,
        });

    let result = apply_retention_plan(
        &mut snapshot,
        &plan,
        &policy,
        now_ns,
        "retention-direct-only",
    )
    .expect("direct-write-only idle session should recycle");

    assert_eq!(result.recycled_memories, 1);
    assert!(snapshot.dataset.memories.is_empty());
    assert_eq!(snapshot.retention_trash_batches.len(), 1);
    assert_eq!(
        snapshot.retention_trash_batches[0].dataset.memories.len(),
        1
    );
}

/// Verifies the enqueue follow-up cannot repeat scans or exceed one external VMM job batch.
/// 验证入队后续阶段不会重复扫描，也不会超过一个外置 VMM 作业批次。
#[test]
fn cold_turn_follow_up_uses_only_remaining_execution_budget() {
    let policy = native_cold_turn_follow_up_policy(&RetentionPolicy::v1(), 7);
    assert_eq!(policy.profile_expiry_limit, 0);
    assert_eq!(policy.memory_recycle_limit, 0);
    assert_eq!(policy.cold_turn_scan_limit, 0);
    assert_eq!(policy.cold_turn_job_limit, 7);
    assert_eq!(policy.idle_session_limit, 0);
    assert_eq!(policy.vector_gc_limit, 0);
    assert_eq!(policy.trash_purge_limit, 0);
    assert_eq!(
        native_cold_turn_follow_up_policy(&RetentionPolicy::v1(), u32::MAX).cold_turn_job_limit,
        RetentionPolicy::v1().cold_turn_job_limit
    );
}

/// Verifies the same-cycle idle follow-up cannot repeat any earlier retention phase.
/// 验证同周期闲置后续阶段不会重复任何更早的保留阶段。
#[test]
fn idle_session_follow_up_keeps_only_idle_recycling_enabled() {
    let policy = native_idle_session_follow_up_policy(&RetentionPolicy::v1());
    assert_eq!(policy.profile_expiry_limit, 0);
    assert_eq!(policy.memory_recycle_limit, 0);
    assert_eq!(policy.cold_turn_scan_limit, 0);
    assert_eq!(policy.cold_turn_job_limit, 0);
    assert_eq!(
        policy.idle_session_limit,
        RetentionPolicy::v1().idle_session_limit
    );
    assert_eq!(policy.vector_gc_limit, 0);
    assert_eq!(policy.trash_purge_limit, 0);
}

/// Verifies every specialized follow-up keeps the business rules admitted by its first pass.
/// 验证所有专用后续阶段保留首轮已准入的业务规则。
#[test]
fn retention_follow_up_preserves_admitted_business_rules() {
    // Distinct values make an accidental reset to v1 defaults observable in every phase.
    // 使用不同于默认值的配置，使任一阶段意外重置为 v1 默认值都能被检测。
    let mut base = RetentionPolicy::v1();
    base.turn_hot_window_size = 17;
    base.session_idle_ms = 12_345;
    base.trash_retention_ms = 54_321;
    base.protected_priority_floor = 73;
    base.protected_memory_level_floor = 2;
    base.skip_protected_shared_memories = false;
    base.profile_expiry_limit = 9;
    base.cold_turn_job_limit = 5;
    base.idle_session_limit = 3;
    for derived in [
        native_profile_expiry_policy(&base),
        native_cold_turn_follow_up_policy(&base, u32::MAX),
        native_idle_session_follow_up_policy(&base),
    ] {
        assert_eq!(derived.turn_hot_window_size, base.turn_hot_window_size);
        assert_eq!(derived.session_idle_ms, base.session_idle_ms);
        assert_eq!(derived.trash_retention_ms, base.trash_retention_ms);
        assert_eq!(
            derived.protected_priority_floor,
            base.protected_priority_floor
        );
        assert_eq!(
            derived.protected_memory_level_floor,
            base.protected_memory_level_floor
        );
        assert_eq!(
            derived.skip_protected_shared_memories,
            base.skip_protected_shared_memories
        );
    }
    assert_eq!(native_profile_expiry_policy(&base).profile_expiry_limit, 9);
    assert_eq!(
        native_cold_turn_follow_up_policy(&base, u32::MAX).cold_turn_job_limit,
        5
    );
    assert_eq!(
        native_idle_session_follow_up_policy(&base).idle_session_limit,
        3
    );
}

/// Verifies apply-time idle validation includes compaction activity rather than turn creation alone.
/// 验证闲置计划的应用时校验会包含压缩活动，而不只检查回合创建时间。
#[test]
fn retention_rejects_idle_plan_after_recent_compaction_activity() {
    let policy = RetentionPolicy::v1();
    let idle_duration_ns = retention_duration_ns(policy.session_idle_ms).expect("idle duration");
    let idle_cutoff_ns = 50;
    let now_ns = idle_duration_ns + idle_cutoff_ns;
    let mut snapshot = empty_snapshot(11);
    let mut old_turn = turn("vmm-turn-idle-activity", 2, "session-a");
    old_turn.status = TurnStatus::Completed;
    old_turn.created_at_ns = 1;
    snapshot.dataset.turns.push(old_turn);
    snapshot
        .dataset
        .chat_compact_boundaries
        .push(ChatCompactBoundary {
            subject_id: native_vmm_subject_id().expect("subject"),
            scope_id: project_scope(2).expect("scope"),
            session_source_id: SessionSourceId::new("session-a").expect("session id"),
            compacted_turn_id: TurnId::new("vmm-turn-idle-activity").expect("turn id"),
            compacted_at_ns: idle_cutoff_ns + 1,
        });
    let mut plan = retention_plan(2);
    plan.idle_session_recycle
        .push(vulcan_memorymesh_core::PlannedSessionRef {
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::IdleSessionRecycle,
        });

    let error = apply_retention_plan(
        &mut snapshot,
        &plan,
        &policy,
        now_ns,
        "retention-recent-compaction",
    )
    .expect_err("recent compaction must invalidate the idle plan");

    assert!(error.contains("已恢复活动"));
    assert_eq!(snapshot.dataset.turns.len(), 1);
    assert!(snapshot.retention_trash_batches.is_empty());
}

/// Verifies recycle summaries preserve project identity, reason, deadline, and exact counts.
/// 验证回收摘要保留项目身份、原因、截止时间与准确计数。
#[test]
fn recycle_batch_summary_exposes_frozen_policy_deadline() {
    let mut batch = retention_trash_batch(
        String::from("batch-a"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        2_000_000,
        9_000_000,
        RetentionReasonCode::IdleSessionRecycle,
    );
    batch.dataset.turns.push(turn("vmm-turn-1", 2, "session-a"));
    batch.dataset.memories.push(memory(
        "vmm-memory-1",
        2,
        "session-a",
        Some("vmm-turn-1"),
        MemorySourceKind::Turn,
        MemoryLevel::Phase,
    ));

    let summary = retention_batch_summary(&batch).expect("recycle batch summary");

    assert_eq!(summary.project_id, 2);
    assert_eq!(summary.reason, "idle_session_recycle");
    assert_eq!(summary.recycled_at_unix_ms, 2);
    assert_eq!(summary.turn_count, 1);
    assert_eq!(summary.memory_count, 1);
    assert_eq!(summary.expires_at_unix_ms, 9);
    // Missing persisted deadlines must fail instead of being backfilled from current defaults.
    // 缺失持久化截止时间必须报错，不可按当前默认窗口回填。
    let mut json = serde_json::to_value(&batch).expect("serialize batch");
    json.as_object_mut()
        .expect("batch object")
        .remove("expires_at_ns");
    assert!(serde_json::from_value::<StorageRetentionTrashBatch>(json).is_err());
}

/// Inspects exact retained pages without leaking active records, other projects or vector arrays.
/// 查看准确保留页，不泄露活动记录、其他项目或向量数组。
#[test]
fn recycle_records_preserve_families_and_versioned_ownership() {
    use vulcan_service_api::{
        VmmNativeRecycleRecord, VmmNativeRecycleRecordKind as Kind, VmmNativeRecycleRecordQuery,
    };
    let mut snapshot = empty_snapshot(7);
    let mut batch = retention_trash_batch(
        "inspection".to_owned(),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        100,
        RetentionReasonCode::IdleSessionRecycle,
    );
    batch
        .dataset
        .turns
        .push(turn("vmm-turn-1", 2, "source-session"));
    for id in ["vmm-memory-3", "vmm-memory-1", "vmm-memory-2"] {
        batch.dataset.memories.push(memory(
            id,
            2,
            "source-session",
            Some("vmm-turn-1"),
            MemorySourceKind::Turn,
            MemoryLevel::Phase,
        ));
    }
    batch.dataset.memories[0].embedding = Some(vec![0.123, 0.456]);
    batch.dataset.profile_nodes.push(profile_node(
        "profile-project",
        project_scope(2).expect("scope"),
        "项目画像原文",
    ));
    batch.dataset.profile_nodes.push(profile_node(
        "profile-user",
        native_vmm_subject_scope().expect("personal"),
        "用户画像原文",
    ));
    batch
        .dataset
        .context_evidence
        .push(vulcan_memorymesh_core::MemoryContextEvidence {
            memory_id: MemoryId::new("vmm-memory-1").expect("memory"),
            context_key: "fruit".to_owned(),
            context_value: "西瓜".to_owned(),
            support_count: 4,
            rebuttal_count: 2,
            last_supported_at_ns: None,
            last_rebutted_at_ns: None,
            version: 1,
        });
    snapshot.retention_trash_batches.push(batch);
    snapshot.dataset.memories.push(memory(
        "active-only",
        2,
        "source-session",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Phase,
    ));
    let before = snapshot.clone();
    let query = VmmNativeRecycleRecordQuery {
        vmm_project_id: 2,
        batch_id: "inspection".to_owned(),
        record_kind: Kind::Memory,
        offset: 0,
        limit: 2,
        expected_storage_version: Some(7),
    };
    let first = recycle_records::build_page(&snapshot, query.clone()).expect("first page");
    assert_eq!(first.total, 3);
    assert_eq!(first.next_offset, Some(2));
    assert_eq!(first.records.len(), 2);
    assert!(
        matches!(&first.records[0], VmmNativeRecycleRecord::Memory { memory_id, source_turn_id: Some(turn), .. } if memory_id == "vmm-memory-1" && turn == "vmm-turn-1")
    );
    let last = recycle_records::build_page(
        &snapshot,
        VmmNativeRecycleRecordQuery {
            offset: 2,
            ..query.clone()
        },
    )
    .expect("last page");
    assert_eq!(last.records.len(), 1);
    assert_eq!(last.next_offset, None);
    assert!(
        !serde_json::to_string(&last)
            .expect("wire")
            .contains("embedding")
    );
    for (kind, total) in [(Kind::Turn, 1), (Kind::Profile, 2), (Kind::Evidence, 1)] {
        let page = recycle_records::build_page(
            &snapshot,
            VmmNativeRecycleRecordQuery {
                record_kind: kind,
                ..query.clone()
            },
        )
        .expect("family");
        assert_eq!(page.total, total);
        let wire = serde_json::to_value(&page).expect("family wire");
        match kind {
            Kind::Profile => {
                assert_eq!(wire["records"][0]["target"], "project");
                assert_eq!(wire["records"][1]["target"], "user");
            }
            Kind::Evidence => {
                assert_eq!(wire["records"][0]["contextValue"], "西瓜");
                assert_eq!(wire["records"][0]["supportCount"], 4);
                assert_eq!(wire["records"][0]["rebuttalCount"], 2);
            }
            Kind::Turn => assert_eq!(wire["records"][0]["sessionSourceId"], "source-session"),
            Kind::Memory => unreachable!(),
        }
    }
    for invalid in [
        VmmNativeRecycleRecordQuery {
            vmm_project_id: 3,
            ..query.clone()
        },
        VmmNativeRecycleRecordQuery {
            expected_storage_version: Some(6),
            ..query.clone()
        },
        VmmNativeRecycleRecordQuery {
            limit: 0,
            ..query.clone()
        },
        VmmNativeRecycleRecordQuery {
            limit: 101,
            ..query.clone()
        },
        VmmNativeRecycleRecordQuery {
            batch_id: "missing".to_owned(),
            ..query.clone()
        },
    ] {
        assert!(recycle_records::build_page(&snapshot, invalid).is_err());
    }
    assert_eq!(snapshot, before);
    snapshot.retention_trash_batches.clear();
    assert!(recycle_records::build_page(&snapshot, query).is_err());
}

/// Verifies delayed native recall and persisted leases use current visibility without touching unrelated hooks.
/// 验证迟到原生召回及持久租约使用当前可见性，不触及无关 Hook。
#[test]
fn recall_publication_reconciles_resident_and_delayed_sources() {
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
        ConversationPrecheckOutcome, HOOK_METADATA_VMM_RECALL_BATCH_EFFECT,
        VmmRecallBatchHookEffect, vmm_recall_batch_effect_from_report,
        vmm_session_status_effect_from_report,
    };
    use vulcan_vmm::{ContextItem, SessionMemoryStatus, VmmRecallBatch, VmmSessionMemoryState};
    // A real UUID preserves the host-to-native session identity contract.
    // 真实 UUID 保持宿主到原生的会话身份契约。
    let session_id = uuid::Uuid::nil();
    // Owner and linked scopes are declared explicitly; project 4 is intentionally inaccessible.
    // 明确声明归属及连带作用域；项目 4 被刻意设为不可访问。
    let access = ScopeAccess {
        write_scope: project_scope(2).expect("owner"),
        visible_scopes: vec![
            project_scope(2).expect("owner"),
            project_scope(3).expect("linked"),
        ],
    };
    // Four original rows contrast live owner, live linked, foreign and logically deleted content.
    // 四条原记录对照活动归属、活动连带、外部及逻辑删除内容。
    let mut snapshot = empty_snapshot(1);
    for (id, project) in [(1, 2), (2, 3), (3, 4), (4, 2)] {
        snapshot.dataset.memories.push(memory(
            &format!("vmm-memory-{id}"),
            project,
            &session_id.to_string(),
            None,
            MemorySourceKind::Direct,
            MemoryLevel::Persistent,
        ));
    }
    snapshot.dataset.memories[3].state = MemoryState::Deleted;
    // Missing ID 5 represents a previously recalled record now removed from active storage.
    // 缺失 ID 5 表示此前召回、现已移出活动存储的记录。
    let batch = VmmRecallBatch {
        ttl_turns: 3,
        injection_tag: "vmm".to_owned(),
        injection_description: "memory".to_owned(),
        items: (1..=5)
            .map(|id| ContextItem {
                memory_id: id,
                text: format!("memory-{id}"),
                ..Default::default()
            })
            .collect(),
    };
    for paused in [false, true] {
        // Each scenario starts with the same already persisted cache and delayed response.
        // 每种场景从相同的已有持久缓存及迟到响应开始。
        let mut state = VmmSessionMemoryState::new(session_id.to_string(), "1").expect("state");
        state
            .select_read_source(
                &session_id.to_string(),
                Some(
                    &crate::native_vmm::client::memory_read_source_from_access(&access)
                        .expect("source"),
                ),
                "1",
            )
            .expect("select source");
        state
            .apply_recall_batch(&session_id.to_string(), "user-1", &batch, "2")
            .expect("previous recall");
        // Preserve an unrelated hook payload and metadata as exact controls.
        // 保留无关 Hook 载荷及元数据作为准确对照。
        let mut report = vulcan_hooks::HookDispatchReport::empty(vulcan_hooks::HookPayload {
            value: serde_json::json!({"user_content": "unchanged"}),
        });
        report.metadata.insert(
            "unrelated".to_owned(),
            vec![serde_json::json!({"keep": true})],
        );
        report.metadata.insert(
            HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
            vec![serde_json::json!(VmmRecallBatchHookEffect {
                read_source: crate::native_vmm::client::memory_read_source_from_access(&access)
                    .expect("recall source"),
                batch: batch.clone()
            })],
        );
        // The correlated outcome deliberately arrives after source visibility changes.
        // 关联结果刻意在来源可见性改变后到达。
        let mut outcome = ConversationPrecheckOutcome {
            session_id,
            turn_id: "new-turn".to_owned(),
            report: Ok(report),
        };
        // The independent lifecycle is authoritative, even when all recalled IDs still exist.
        // 即使全部召回 ID 仍然存在，独立生命周期仍是权威。
        let mut current = snapshot.clone();
        if paused {
            current
                .memory_sessions
                .push(vulcan_memorymesh_core::StorageMemorySession {
                    subject_id: native_vmm_subject_id().expect("subject"),
                    scope_id: access.write_scope.clone(),
                    session_source_id: SessionSourceId::new(session_id.to_string())
                        .expect("session"),
                    status: vulcan_memorymesh_core::StorageSessionStatus::Recycled,
                    last_extraction_observed_at_ns: 0,
                    last_extraction_completed_at_ns: None,
                });
        }
        // Production storage validation precedes the bounded publication lookup.
        // 生产存储校验先于有界发布查询。
        let storage =
            vulcan_memorymesh_core::InMemoryStorage::from_snapshot(current).expect("storage");
        recall_publication::reconcile_recall(
            &storage,
            &native_vmm_subject_id().expect("subject"),
            &access,
            &SessionSourceId::new(session_id.to_string()).expect("session"),
            &mut state,
            Some(&mut outcome),
            "3",
        )
        .expect("reconcile");
        // Typed status and recall remain separate; an empty/paused batch is removed entirely.
        // 类型化状态与召回保持分离；空或暂停批次被整体移除。
        let report = outcome.report.as_ref().expect("report");
        assert_eq!(
            report.payload.value,
            serde_json::json!({"user_content": "unchanged"})
        );
        assert_eq!(
            report.metadata["unrelated"],
            vec![serde_json::json!({"keep": true})]
        );
        if paused {
            assert_eq!(state.session_memory_status, SessionMemoryStatus::Recycled);
            assert!(state.active_leases.is_empty());
            assert!(
                vmm_recall_batch_effect_from_report(report)
                    .expect("typed recall")
                    .is_none()
            );
        } else {
            assert_eq!(
                state.active_leases.keys().cloned().collect::<Vec<_>>(),
                ["memory:1", "memory:2"]
            );
            // Applying the delayed response cannot reintroduce removed, deleted or out-of-scope memory.
            // 应用迟到响应不能重新引入已移除、已删除或作用域外记忆。
            let effect = vmm_recall_batch_effect_from_report(report)
                .expect("typed recall")
                .expect("retained recall");
            assert_eq!(
                effect
                    .batch
                    .items
                    .iter()
                    .map(|item| item.memory_id)
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            state
                .apply_recall_batch(&session_id.to_string(), "user-2", &effect.batch, "4")
                .expect("safe delayed batch");
            assert_eq!(state.active_leases.len(), 2);
        }
        assert_eq!(
            vmm_session_status_effect_from_report(report)
                .expect("typed status")
                .expect("status")
                .status,
            state.session_memory_status
        );
    }
}

/// Builds valid source-linked content with an empty owner and same-name foreign session.
/// 构造有效来源关联内容，包含空归属及其他项目同名会话。
/// Returns a canonical snapshot for both whole-session and individual-turn moves.
/// 返回供完整会话及独立回合移动使用的规范快照。
fn manual_removal_snapshot() -> StorageSnapshot {
    // Begin with the exact production storage shape.
    // 从准确生产存储结构开始。
    let mut snapshot = empty_snapshot(17);
    for (project, session) in [(2, "a"), (2, "empty"), (3, "a")] {
        snapshot
            .memory_sessions
            .push(vulcan_memorymesh_core::StorageMemorySession {
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: project_scope(project).expect("scope"),
                session_source_id: SessionSourceId::new(session).expect("session"),
                status: vulcan_memorymesh_core::StorageSessionStatus::Archived,
                last_extraction_observed_at_ns: 20,
                last_extraction_completed_at_ns: Some(30),
            });
    }
    snapshot.dataset.turns = vec![
        turn("vmm-turn-2", 2, "a"),
        turn("vmm-turn-10", 2, "a"),
        turn("vmm-turn-20", 3, "a"),
    ];
    for (id, project, source) in [
        ("source", 2, Some("vmm-turn-2")),
        ("direct", 2, None),
        ("foreign", 3, Some("vmm-turn-20")),
    ] {
        // Include persistent direct writes; manual removal must not apply periodic eligibility filters.
        // 包含持久直写；人工移除不能应用周期清洗资格过滤。
        let mut record = memory(
            id,
            project,
            "a",
            source,
            MemorySourceKind::Direct,
            MemoryLevel::Persistent,
        );
        record.vector_space_id =
            Some(VectorSpaceId::new("original-generation").expect("generation"));
        record.embedding = Some(vec![0.25, 0.75]);
        snapshot.dataset.memories.push(record);
    }
    for (id, scope) in [
        ("project-profile", project_scope(2).expect("project")),
        (
            "user-profile",
            native_vmm_subject_scope().expect("personal"),
        ),
    ] {
        // Both profile families explicitly cite the selected turn but remain independently owned.
        // 两类画像均明确引用所选回合，但保持独立归属。
        let mut node = profile_node(id, scope, id);
        node.source_turn_ids = vec![TurnId::new("vmm-turn-2").expect("turn")];
        snapshot.dataset.profile_nodes.push(node);
    }
    snapshot
        .dataset
        .context_evidence
        .push(vulcan_memorymesh_core::MemoryContextEvidence {
            memory_id: MemoryId::new("source").expect("memory"),
            context_key: "branch".to_owned(),
            context_value: "main".to_owned(),
            support_count: 1,
            rebuttal_count: 0,
            last_supported_at_ns: None,
            last_rebutted_at_ns: None,
            version: 1,
        });
    for (project, id) in [(2, "vmm-turn-10"), (3, "vmm-turn-20")] {
        snapshot.dataset.chat_compact_boundaries.push(
            vulcan_memorymesh_core::ChatCompactBoundary {
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: project_scope(project).expect("scope"),
                session_source_id: SessionSourceId::new("a").expect("session"),
                compacted_turn_id: TurnId::new(id).expect("turn"),
                compacted_at_ns: 40,
            },
        );
    }
    vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
        .expect("valid fixture")
        .snapshot()
}

/// Checks exact source moves, profile/vector preservation, session metadata and reversible identity.
/// 检查准确来源移动、画像及向量保留、会话元数据及可恢复身份。
#[test]
fn manual_removal_moves_originals_and_restores_exact_owner() {
    use vulcan_service_api::{
        VmmNativeRemovalPreview, VmmNativeRemovalQuery, VmmNativeRemovalTarget,
    };
    for selection in [
        VmmNativeRemovalTarget::Sessions {
            ids: vec!["a".to_owned(), "empty".to_owned()],
        },
        VmmNativeRemovalTarget::Turns {
            ids: vec!["vmm-turn-2".to_owned()],
        },
    ] {
        // Each variant starts from a fresh canonical snapshot.
        // 每种变体从新的规范快照开始。
        let before = manual_removal_snapshot();
        // The target kind alone controls whether session metadata and compaction move.
        // 只有目标种类决定会话元数据及压缩是否移动。
        let whole = matches!(&selection, VmmNativeRemovalTarget::Sessions { .. });
        // Build the same impact used by the production preview.
        // 构建生产预览使用的相同影响。
        let impact = removal_impact::build_impact(
            &before,
            VmmNativeRemovalQuery {
                vmm_project_id: 2,
                selection,
            },
            2,
        )
        .expect("impact");
        // The transaction candidate preserves the original snapshot for comparison.
        // 事务候选保留原始快照用于比较。
        let mut snapshot = before.clone();
        // Both paths must keep the same source IDs and declared vector generation.
        // 两条路径均须保留相同来源 ID 及声明向量代次。
        let result = manual_removal::apply_manual_removal(
            &mut snapshot,
            VmmNativeRemovalPreview {
                preview_id: "exact-source".to_owned(),
                impact: impact.clone(),
                trash_retention_ms: 50,
            },
            100,
        )
        .expect("move originals");
        assert_eq!(result.storage_version, before.storage_version + 1);
        assert_eq!(result.impact, impact);
        assert_eq!(snapshot.dataset.profile_nodes, before.dataset.profile_nodes);
        assert_eq!(snapshot.retention_trash_batches.len(), 1);
        // The complete batch, including saved session originals, is validated by the real loader.
        // 完整批次连同保存的原会话一起由真实加载器校验。
        let mut persisted = vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
            .expect("valid moved snapshot")
            .snapshot();
        // Remove the batch as the production restore transaction does.
        // 像生产恢复事务一样取出批次。
        let batch = persisted.retention_trash_batches.remove(0);
        // UI summaries must preserve original empty-session selections rather than infer them from turns.
        // 界面摘要必须保留原始空会话选择，而非从回合推测它们。
        let summary = retention_batch_summary(&batch).expect("manual source projection");
        assert_eq!(
            summary.manual_selection,
            Some(impact.query.selection.clone())
        );
        assert_eq!(batch.expires_at_ns, 50_000_100);
        assert!(batch.dataset.profile_nodes.is_empty());
        assert_eq!(batch.dataset.memories.len(), if whole { 2 } else { 1 });
        assert_eq!(batch.dataset.turns.len(), if whole { 2 } else { 1 });
        assert_eq!(
            batch.dataset.chat_compact_boundaries.len(),
            usize::from(whole)
        );
        assert_eq!(batch.dataset.context_evidence.len(), 1);
        for moved in &batch.dataset.memories {
            assert_eq!(
                Some(moved),
                before
                    .dataset
                    .memories
                    .iter()
                    .find(|memory| memory.id == moved.id)
            );
        }
        if !whole {
            assert_eq!(persisted.memory_sessions, before.memory_sessions);
        }
        ensure_recycle_restore_has_no_conflicts(&persisted, &batch)
            .expect("restore exact identity");
        restore_retention_batch(&mut persisted, batch).expect("restore originals");
        // External whole-session restore activates only the exact restored owners.
        // 外置完整会话恢复只激活准确恢复的归属。
        let mut expected = before;
        if whole {
            for session in &mut expected.memory_sessions {
                if session.scope_id == project_scope(2).expect("scope") {
                    session.status = vulcan_memorymesh_core::StorageSessionStatus::Active;
                }
            }
        }
        assert_eq!(
            vulcan_memorymesh_core::InMemoryStorage::from_snapshot(persisted)
                .expect("valid restored")
                .snapshot(),
            expected
        );
    }
}

/// Rejects stale membership and invalid deadlines before changing any transaction content.
/// 在改变事务内容之前拒绝过期成员关系及无效截止时间。
#[test]
fn manual_removal_rejects_changed_impact_without_partial_mutation() {
    use vulcan_service_api::{
        VmmNativeRemovalPreview, VmmNativeRemovalQuery, VmmNativeRemovalTarget,
    };
    // Capture one exact turn with both ordinary and independent profile sources.
    // 捕获一个同时关联普通记忆及独立画像来源的准确回合。
    let before = manual_removal_snapshot();
    // This immutable impact is the server-owned confirmation baseline.
    // 此不可变影响是服务端确认基线。
    let impact = removal_impact::build_impact(
        &before,
        VmmNativeRemovalQuery {
            vmm_project_id: 2,
            selection: VmmNativeRemovalTarget::Turns {
                ids: vec!["vmm-turn-2".to_owned()],
            },
        },
        2,
    )
    .expect("impact");
    for case in 0..4 {
        // Each rejection receives its own untouched transaction candidate.
        // 每种拒绝情况取得自己的未改动事务候选。
        let mut snapshot = before.clone();
        // Only explicit test inputs differ, not the production selection algorithm.
        // 只改变明确测试输入，而非生产选择算法。
        let mut preview = VmmNativeRemovalPreview {
            preview_id: "reject".to_owned(),
            impact: impact.clone(),
            trash_retention_ms: 50,
        };
        match case {
            0 => preview.impact.memory_ids.clear(),
            1 => preview.impact.storage_version += 1,
            2 => preview.trash_retention_ms = 0,
            3 => preview.trash_retention_ms = u64::MAX,
            _ => unreachable!(),
        }
        assert!(manual_removal::apply_manual_removal(&mut snapshot, preview, 100).is_err());
        assert_eq!(snapshot, before);
    }
}

/// Verifies manual source inspection includes direct and persistent memories without applying retention filters.
/// 验证人工来源检查包含直写及持久记忆，不应用保留清洗过滤。
#[test]
fn removal_impact_resolves_exact_sources_and_numeric_hot_windows() {
    use vulcan_service_api::{VmmNativeRemovalQuery, VmmNativeRemovalTarget};
    // Same-name foreign sessions and empty sessions must remain separate authoritative records.
    // 同名外部项目会话与空会话必须保留为独立权威记录。
    let mut snapshot = empty_snapshot(17);
    for (project, id) in [(2, "a"), (2, "b"), (2, "empty"), (3, "a")] {
        snapshot
            .memory_sessions
            .push(vulcan_memorymesh_core::StorageMemorySession {
                subject_id: native_vmm_subject_id().expect("subject"),
                scope_id: project_scope(project).expect("scope"),
                session_source_id: SessionSourceId::new(id).expect("session"),
                status: vulcan_memorymesh_core::StorageSessionStatus::Active,
                last_extraction_observed_at_ns: 0,
                last_extraction_completed_at_ns: None,
            });
    }
    for (id, project, session) in [
        (10, 2, "a"),
        (2, 2, "a"),
        (1, 2, "a"),
        (20, 2, "b"),
        (30, 3, "a"),
    ] {
        snapshot
            .dataset
            .turns
            .push(turn(&format!("vmm-turn-{id}"), project, session));
    }
    snapshot.dataset.turns[0].status = TurnStatus::Completed;
    for (id, project, session, source) in [
        ("m1", 2, "a", Some("vmm-turn-1")),
        ("m2", 2, "a", None),
        ("m3", 2, "b", Some("vmm-turn-2")),
        ("m4", 2, "b", Some("vmm-turn-20")),
        ("m5", 3, "a", Some("vmm-turn-1")),
    ] {
        snapshot.dataset.memories.push(memory(
            id,
            project,
            session,
            source,
            MemorySourceKind::Direct,
            MemoryLevel::Persistent,
        ));
    }
    snapshot.dataset.memories[1].state = MemoryState::Deleted;
    snapshot.dataset.memories[0].vector_space_id =
        Some(VectorSpaceId::new("generation").expect("space"));
    snapshot.dataset.memories[0].embedding = Some(vec![1.0]);
    for (id, scope, sources) in [
        (
            "project",
            project_scope(2).expect("scope"),
            vec!["vmm-turn-1"],
        ),
        (
            "user",
            native_vmm_subject_scope().expect("user scope"),
            vec!["vmm-turn-1", "vmm-turn-20"],
        ),
        (
            "foreign",
            project_scope(3).expect("scope"),
            vec!["vmm-turn-1"],
        ),
        ("manual", project_scope(2).expect("scope"), vec![]),
    ] {
        let mut node = profile_node(id, scope, id);
        node.source_turn_ids = sources
            .into_iter()
            .map(|id| TurnId::new(id).expect("source"))
            .collect();
        if id == "project" {
            node.subject_id = None;
        }
        snapshot.dataset.profile_nodes.push(node);
    }
    for id in ["m1", "m4"] {
        snapshot
            .dataset
            .context_evidence
            .push(vulcan_memorymesh_core::MemoryContextEvidence {
                memory_id: MemoryId::new(id).expect("memory"),
                context_key: "branch".to_owned(),
                context_value: "main".to_owned(),
                support_count: 1,
                rebuttal_count: 0,
                last_supported_at_ns: None,
                last_rebutted_at_ns: None,
                version: 1,
            });
    }
    let before = snapshot.clone();
    let query = VmmNativeRemovalQuery {
        vmm_project_id: 2,
        selection: VmmNativeRemovalTarget::Sessions {
            ids: vec!["a".to_owned(), "a".to_owned()],
        },
    };
    let impact = removal_impact::build_impact(&snapshot, query.clone(), 2).expect("session impact");
    assert_eq!(impact.session_source_ids, ["a"]);
    assert_eq!(impact.turn_ids, ["vmm-turn-1", "vmm-turn-2", "vmm-turn-10"]);
    assert_eq!(impact.hot_window_turn_ids, ["vmm-turn-2", "vmm-turn-10"]);
    assert_eq!(impact.pending_turn_ids, ["vmm-turn-1", "vmm-turn-2"]);
    assert_eq!(impact.memory_ids, ["m1", "m2", "m3"]);
    assert_eq!(impact.vector_memory_ids, ["m1"]);
    assert_eq!(impact.project_profile_node_ids, ["project"]);
    assert_eq!(impact.user_profile_node_ids, ["user"]);
    assert_eq!(impact.context_evidence.len(), 1);
    assert_eq!(impact.context_evidence[0].memory_id, "m1");
    assert_eq!(impact.storage_version, 17);
    let turn_impact = removal_impact::build_impact(
        &snapshot,
        VmmNativeRemovalQuery {
            vmm_project_id: 2,
            selection: VmmNativeRemovalTarget::Turns {
                ids: vec!["vmm-turn-2".to_owned()],
            },
        },
        1,
    )
    .expect("single turn");
    assert_eq!(turn_impact.memory_ids, ["m3"]);
    assert!(turn_impact.hot_window_turn_ids.is_empty());
    assert!(turn_impact.user_profile_node_ids.is_empty());
    let empty = removal_impact::build_impact(
        &snapshot,
        VmmNativeRemovalQuery {
            vmm_project_id: 2,
            selection: VmmNativeRemovalTarget::Sessions {
                ids: vec!["empty".to_owned()],
            },
        },
        2,
    )
    .expect("empty session still exists");
    assert_eq!(empty.session_source_ids, ["empty"]);
    assert!(empty.turn_ids.is_empty());
    assert!(empty.memory_ids.is_empty());
    for selection in [
        VmmNativeRemovalTarget::Sessions { ids: vec![] },
        VmmNativeRemovalTarget::Sessions {
            ids: vec!["a".to_owned(); 101],
        },
        VmmNativeRemovalTarget::Sessions {
            ids: vec![" a".to_owned()],
        },
        VmmNativeRemovalTarget::Sessions {
            ids: vec!["missing".to_owned()],
        },
        VmmNativeRemovalTarget::Turns {
            ids: vec!["vmm-turn-30".to_owned()],
        },
        VmmNativeRemovalTarget::Turns {
            ids: vec!["vmm-turn-wrong".to_owned()],
        },
        VmmNativeRemovalTarget::Turns {
            ids: vec!["vmm-turn-2".to_owned(), "vmm-turn-404".to_owned()],
        },
    ] {
        assert!(
            removal_impact::build_impact(
                &snapshot,
                VmmNativeRemovalQuery {
                    vmm_project_id: 2,
                    selection
                },
                2
            )
            .is_err()
        );
    }
    assert_eq!(snapshot, before);
    snapshot.memory_sessions[0].status = vulcan_memorymesh_core::StorageSessionStatus::Archived;
    assert!(
        removal_impact::build_impact(&snapshot, query.clone(), 0)
            .expect("archived allowed")
            .hot_window_turn_ids
            .is_empty()
    );
    for status in [
        vulcan_memorymesh_core::StorageSessionStatus::Recycled,
        vulcan_memorymesh_core::StorageSessionStatus::Forgotten,
    ] {
        snapshot.memory_sessions[0].status = status;
        assert!(removal_impact::build_impact(&snapshot, query.clone(), 2).is_err());
    }
}

/// Verifies restoring a batch moves its original vector-bearing records without regeneration.
/// 验证恢复批次会移动其原始含向量记录，且不重新生成向量。
#[test]
fn recycle_restore_moves_original_records_and_preserves_embedding() {
    let mut snapshot = empty_snapshot(9);
    let mut batch = retention_trash_batch(
        String::from("batch-restore"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        100,
        RetentionReasonCode::TerminalMemoryRecycle,
    );
    let mut restored_memory = memory(
        "vmm-memory-restored",
        2,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Stable,
    );
    restored_memory.embedding = Some(vec![0.25, 0.75]);
    batch.dataset.memories.push(restored_memory);

    ensure_recycle_restore_has_no_conflicts(&snapshot, &batch).expect("conflict-free restore");
    restore_retention_batch(&mut snapshot, batch).expect("restore original records");

    assert_eq!(snapshot.dataset.memories.len(), 1);
    assert_eq!(
        snapshot.dataset.memories[0].embedding.as_deref(),
        Some([0.25_f32, 0.75_f32].as_slice())
    );
}

/// Verifies restore refuses identities that became active after recycling.
/// 验证恢复会拒绝回收后重新变为活动状态的身份。
#[test]
fn recycle_restore_rejects_active_identity_conflicts() {
    let mut snapshot = empty_snapshot(10);
    snapshot
        .dataset
        .turns
        .push(turn("vmm-turn-conflict", 2, "session-a"));
    let mut batch = retention_trash_batch(
        String::from("batch-conflict"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        100,
        RetentionReasonCode::ColdTurnRecycle,
    );
    batch
        .dataset
        .turns
        .push(turn("vmm-turn-conflict", 2, "session-a"));

    let error =
        ensure_recycle_restore_has_no_conflicts(&snapshot, &batch).expect_err("restore conflict");

    assert!(error.contains("turn:vmm-turn-conflict"));
}

/// Verifies every native retention family rejects a mismatched stable reason code.
/// 验证每一个原生保留计划族都会拒绝不匹配的稳定原因码。
#[test]
fn retention_rejects_wrong_reason_codes_for_every_plan_family() {
    let expected_access = MemoryAccessIdentity::from(&memory(
        "vmm-memory-reason",
        2,
        "session-a",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Phase,
    ));
    let mut plans = Vec::new();

    let mut profile_plan = retention_plan(2);
    profile_plan
        .profile_expiry
        .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
            profile_node_id: ProfileNodeId::new("vmm-profile-reason").expect("profile id"),
            expected_subject_id: Some(native_vmm_subject_id().expect("subject")),
            expected_scope_id: project_scope(2).expect("scope"),
            expected_version: 1,
            reason: RetentionReasonCode::ColdTurnRecycle,
        });
    plans.push(profile_plan);

    let mut memory_plan = retention_plan(2);
    memory_plan
        .terminal_memory_recycle
        .push(vulcan_memorymesh_core::PlannedMemoryRef {
            memory_id: String::from("vmm-memory-reason"),
            expected_version: 1,
            expected_access,
            reason: RetentionReasonCode::IdleSessionRecycle,
        });
    plans.push(memory_plan);

    let mut enqueue_plan = retention_plan(2);
    enqueue_plan
        .cold_turn_enqueue
        .push(vulcan_memorymesh_core::PlannedSessionRef {
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::TerminalMemoryRecycle,
        });
    plans.push(enqueue_plan);

    let mut execute_plan = retention_plan(2);
    execute_plan
        .cold_turn_execute
        .push(vulcan_memorymesh_core::PlannedColdTurnJobRef {
            job_id: String::from("cold-job-reason"),
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::TrashExpiryPurge,
        });
    plans.push(execute_plan);

    let mut idle_plan = retention_plan(2);
    idle_plan
        .idle_session_recycle
        .push(vulcan_memorymesh_core::PlannedSessionRef {
            session_source_id: String::from("session-a"),
            reason: RetentionReasonCode::ColdTurnRecycle,
        });
    plans.push(idle_plan);

    let mut purge_plan = retention_plan(2);
    purge_plan
        .trash_purge
        .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
            batch_id: String::from("trash-batch-reason"),
            reason: RetentionReasonCode::ProfileExpiry,
        });
    plans.push(purge_plan);

    for plan in plans {
        let error = apply_retention_plan(
            &mut empty_snapshot(1),
            &plan,
            &RetentionPolicy::v1(),
            100,
            "retention-reason",
        )
        .expect_err("wrong reason must fail");
        assert!(error.contains("原因码"), "unexpected error: {error}");
    }
}

/// Verifies duplicate profile identities fail before any profile state is mutated.
/// 验证重复画像身份会在任何画像状态被修改前失败。
#[test]
fn retention_rejects_duplicate_profile_identity_atomically() {
    let mut snapshot = empty_snapshot(1);
    let mut profile = profile_node(
        "vmm-profile-duplicate",
        project_scope(2).expect("scope"),
        "duplicate profile",
    );
    profile.expires_at_ns = Some(50);
    snapshot.dataset.profile_nodes.push(profile.clone());
    let planned = vulcan_memorymesh_core::PlannedProfileNodeRef {
        profile_node_id: profile.id.clone(),
        expected_subject_id: profile.subject_id.clone(),
        expected_scope_id: profile.scope_id.clone(),
        expected_version: profile.version,
        reason: RetentionReasonCode::ProfileExpiry,
    };
    let mut plan = retention_plan(2);
    plan.profile_expiry = vec![planned.clone(), planned];

    let error = apply_retention_plan(
        &mut snapshot,
        &plan,
        &RetentionPolicy::v1(),
        100,
        "retention-duplicate",
    )
    .expect_err("duplicate profile must fail");

    assert!(error.contains("重复身份"));
    assert_eq!(
        snapshot.dataset.profile_nodes[0].state,
        ProfileNodeState::Active
    );
    assert_eq!(snapshot.dataset.profile_nodes[0].version, 1);
}

/// Verifies automatic purge rechecks ownership and the finite recycle deadline at apply time.
/// 验证自动清除会在应用时重新检查所有权与有限回收期限。
#[test]
fn retention_rejects_premature_or_cross_scope_trash_purge() {
    let policy = RetentionPolicy::v1();
    let batch = retention_trash_batch(
        String::from("trash-batch-pending"),
        native_vmm_subject_id().expect("subject"),
        project_scope(3).expect("scope"),
        50,
        200,
        RetentionReasonCode::ColdTurnRecycle,
    );
    let mut snapshot = empty_snapshot(1);
    snapshot.retention_trash_batches.push(batch);
    let mut plan = retention_plan(2);
    plan.trash_purge
        .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
            batch_id: String::from("trash-batch-pending"),
            reason: RetentionReasonCode::TrashExpiryPurge,
        });

    let error = apply_retention_plan(&mut snapshot, &plan, &policy, 100, "retention-purge")
        .expect_err("cross-scope premature purge must fail");

    assert!(error.contains("所有权或到期状态"));
    assert_eq!(snapshot.retention_trash_batches.len(), 1);
}

/// Verifies a shortened default cannot purge an existing batch before its frozen deadline.
/// 验证缩短默认窗口不能在已有批次冻结截止时间之前清除该批次。
#[test]
fn retention_purge_preserves_deadline_when_default_is_shortened() {
    // The shortened default would have expired this batch, but its frozen deadline has not.
    // 缩短后的默认窗口会认为批次已到期，但批次冻结的截止时间尚未到达。
    let mut policy = RetentionPolicy::v1();
    policy.trash_retention_ms = 1;
    let mut snapshot = empty_snapshot(1);
    snapshot.retention_trash_batches.push(retention_trash_batch(
        String::from("long-lived-batch"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        10_000_000,
        RetentionReasonCode::ColdTurnRecycle,
    ));
    let mut plan = retention_plan(2);
    plan.trash_purge
        .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
            batch_id: String::from("long-lived-batch"),
            reason: RetentionReasonCode::TrashExpiryPurge,
        });
    let error = apply_retention_plan(&mut snapshot, &plan, &policy, 9_999_999, "shortened")
        .expect_err("frozen deadline has not elapsed");
    assert!(error.contains("所有权或到期状态"));
    assert_eq!(
        snapshot.retention_trash_batches[0].expires_at_ns,
        10_000_000
    );
    let result = apply_retention_plan(&mut snapshot, &plan, &policy, 10_000_000, "due")
        .expect("exact frozen deadline permits purge");
    assert_eq!(result.purged_trash_batches, 1);
    assert!(snapshot.retention_trash_batches.is_empty());
}

/// Verifies permanent trash purge also removes idempotency receipts for purged records.
/// 验证永久清除回收站时也会删除引用已清除记录的幂等收据。
#[test]
fn retention_purge_removes_dangling_idempotency_outcomes() {
    let policy = RetentionPolicy::v1();
    let memory_id = "vmm-memory-purged";
    let mut batch = retention_trash_batch(
        String::from("trash-batch-expired"),
        native_vmm_subject_id().expect("subject"),
        project_scope(2).expect("scope"),
        1,
        100,
        RetentionReasonCode::ColdTurnRecycle,
    );
    batch.dataset.memories.push(memory(
        memory_id,
        2,
        "purged-session",
        None,
        MemorySourceKind::Direct,
        MemoryLevel::Session,
    ));
    let mut snapshot = empty_snapshot(1);
    snapshot.retention_trash_batches.push(batch);
    push_memory_ledger_entry(&mut snapshot, memory_id);
    let mut plan = retention_plan(2);
    plan.trash_purge
        .push(vulcan_memorymesh_core::PlannedTrashBatchRef {
            batch_id: String::from("trash-batch-expired"),
            reason: RetentionReasonCode::TrashExpiryPurge,
        });
    // The current default is much longer; only the batch's original deadline governs purge.
    // 当前默认窗口远长于该批次；清除仅由批次原始截止时间决定。
    let now_ns = 100;

    let result = apply_retention_plan(&mut snapshot, &plan, &policy, now_ns, "retention-purge")
        .expect("expired trash purge");

    assert_eq!(result.purged_trash_batches, 1);
    assert!(snapshot.retention_trash_batches.is_empty());
    assert!(snapshot.outcomes.is_empty());
    vulcan_memorymesh_core::InMemoryStorage::from_snapshot(snapshot)
        .expect("purged snapshot remains valid");
}

/// Verifies a Core plan cannot exceed any frozen per-pass policy bound.
/// 验证 Core 计划不能超过任一冻结的单轮策略边界。
#[test]
fn retention_rejects_plan_families_above_policy_bounds() {
    let mut policy = RetentionPolicy::v1();
    policy.profile_expiry_limit = 1;
    let mut plan = retention_plan(2);
    for suffix in ["a", "b"] {
        plan.profile_expiry
            .push(vulcan_memorymesh_core::PlannedProfileNodeRef {
                profile_node_id: ProfileNodeId::new(format!("vmm-profile-{suffix}"))
                    .expect("profile id"),
                expected_subject_id: Some(native_vmm_subject_id().expect("subject")),
                expected_scope_id: project_scope(2).expect("scope"),
                expected_version: 1,
                reason: RetentionReasonCode::ProfileExpiry,
            });
    }

    let error = apply_retention_plan(
        &mut empty_snapshot(1),
        &plan,
        &policy,
        100,
        "retention-bound",
    )
    .expect_err("oversized plan must fail");

    assert!(error.contains("单轮处理上限"));
}

/// Verifies native defaults preserve the exact external VMM P1 and stable protection floors.
/// 验证原生默认值保留外置 VMM 准确的 P1 与 stable 保护阈值。
#[test]
fn retention_policy_v1_matches_external_protection_defaults() {
    let policy = RetentionPolicy::v1();

    assert_eq!(policy.protected_priority_floor, 60);
    assert_eq!(
        policy.protected_memory_level_floor,
        MemoryLevel::Stable as u32
    );
    assert!(policy.skip_protected_shared_memories);
}

/// Verifies the fast profile worker cannot trigger any unrelated retention phase.
/// 验证快速画像工作器不能触发任何无关保留阶段。
#[test]
fn profile_expiry_policy_disables_every_unrelated_retention_family() {
    let policy = native_profile_expiry_policy(&RetentionPolicy::v1());
    assert_eq!(policy.profile_expiry_limit, 256);
    assert_eq!(policy.memory_recycle_limit, 0);
    assert_eq!(policy.cold_turn_scan_limit, 0);
    assert_eq!(policy.cold_turn_job_limit, 0);
    assert_eq!(policy.idle_session_limit, 0);
    assert_eq!(policy.vector_gc_limit, 0);
    assert_eq!(policy.trash_purge_limit, 0);
}

/// Verifies project-local cleanup failures remain individually attributable.
/// 验证项目局部清洗失败保持可逐项归因。
#[test]
fn retention_project_failures_render_exact_project_identity() {
    let result = NativeVmmRetentionApplyResult {
        project_failures: vec![NativeVmmRetentionProjectFailure {
            project_id: 7,
            error: String::from("storage unavailable"),
        }],
        ..NativeVmmRetentionApplyResult::default()
    };

    assert_eq!(
        retention_project_failure_message(&result).as_deref(),
        Some("项目 7：storage unavailable")
    );
}
