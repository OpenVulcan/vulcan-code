//! Atomic storage, idempotency, and scope isolation tests.
//! 原子存储、幂等与作用域隔离测试。

use vulcan_memorymesh_core_contracts::{
    AnalysisAttemptIdentity, CandidateQueryBoundary, ChatCompactBoundary, ConfigSnapshotId,
    ContextEvidenceRelation, DirectWriteDedupeLookup, EntityKind, EntityRef, ErrorCode,
    EvidenceRef, ExportDataset, IdempotencyKey, MemoryAccessClass, MemoryAccessIdentity,
    MemoryCategory, MemoryId, MemoryLevel, MemoryRecord, MemoryScopeClass, MemorySourceKind,
    MemoryState, Mutation, MutationBatch, MutationIdentity, MutationOutcome, MutationSourceKind,
    OperationId, RetentionPolicy, RetentionReasonCode, Role, ScopeAccess, ScopeId,
    ScoreContractRef, SessionRecallBoundary, SessionRecallMode, SessionSourceId, StorageQuery,
    StorageQueryResult, StructuredOutputContractId, SubjectId, TimelineItem, Turn, TurnId,
    TurnStatus, VectorSpaceId, context_evidence_domain_id,
};
use vulcan_memorymesh_core_storage::{
    InMemoryStorage, REFERENCE_CONTRACT_VERSION, REFERENCE_VECTOR_CONTRACT_ID, StoragePort,
    StorageRetentionTrashBatch, StorageSnapshot,
};

/// Verifies canonical attempt ordering survives independently seeded hash maps after reload.
/// 验证规范尝试顺序在重载后独立初始化的哈希表中保持一致。
#[test]
fn snapshot_analysis_attempt_order_is_stable_after_reload() {
    // Persist deliberately interleaved identities and generations through the actual mutation port.
    // 通过实际变更端口持久化刻意交错的身份及代次。
    let mut storage = InMemoryStorage::new();
    let mut mutations = ["attempt-b", "attempt-a"]
        .into_iter()
        .map(|id| Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: id.to_owned(),
                expected_version: 0,
            },
            turn: turn(id, 1),
        })
        .collect::<Vec<_>>();
    for (id, generation) in [
        ("attempt-b", 2),
        ("attempt-a", 3),
        ("attempt-b", 1),
        ("attempt-a", 1),
    ] {
        mutations.push(Mutation::PutAnalysisAttempt(AnalysisAttemptIdentity {
            turn_id: TurnId::new(id).expect("turn"),
            analysis_generation: generation,
            mutation_identity: identity(&format!("{id}-{generation}")),
            config_snapshot_id: ConfigSnapshotId::new("attempt-order").expect("config"),
            structured_output_contracts: vec![StructuredOutputContractId::PostActionAnalysis],
            score_contracts: Vec::new(),
        }));
    }
    storage
        .mutate(&batch("attempt-order", mutations))
        .expect("commit");
    let expected = storage.snapshot();
    assert_eq!(
        expected
            .analysis_attempts
            .iter()
            .map(|attempt| (attempt.turn_id.as_str(), attempt.analysis_generation))
            .collect::<Vec<_>>(),
        vec![
            ("attempt-a", 1),
            ("attempt-a", 3),
            ("attempt-b", 1),
            ("attempt-b", 2)
        ]
    );
    for _ in 0..16 {
        storage = InMemoryStorage::from_snapshot(expected.clone()).expect("reload");
        assert_eq!(storage.snapshot(), expected);
    }
}

/// Builds an atomic successful extraction, including the valid empty-candidate case.
/// 构造一次原子成功提取，包含有效的空候选情况。
/// `id` is the pending turn, and `after`/`through` are the actual analysis observation bounds.
/// `id` 是待处理回合，`after`/`through` 是本次分析的实际观察边界。
fn extraction_batch(key: &str, id: &TurnId, after: i64, through: i64) -> MutationBatch {
    batch(
        key,
        vec![
            Mutation::TransitionTurnStatus {
                turn_id: id.clone(),
                from: TurnStatus::Pending,
                to: TurnStatus::Completed,
                user_input_kind: Some(vulcan_memorymesh_core_contracts::TurnInputKind::Statement),
                analysis_details: Some("valid extraction, no new candidates".to_owned()),
            },
            Mutation::AdvanceSessionExtraction {
                turn_id: id.clone(),
                observed_after_ns: after,
                observed_through_ns: through,
            },
        ],
    )
}

/// Verifies exact direct-write windows across three successes and an independent session.
/// 验证连续三次成功及独立会话的精确直写窗口。
#[test]
fn successful_session_windows_exclude_already_observed_direct_writes() {
    // storage starts without any session metadata; intake must create its zero boundary.
    // storage 初始无会话元数据；登记必须创建零边界。
    let mut storage = InMemoryStorage::new();
    // targets share one owner, except the last target which belongs to another session.
    // targets 共享同一归属，最后一个回合除外，它属于另一个会话。
    let mut targets = (1..=5)
        .map(|index| {
            // target is pending, not a pre-populated success from the general turn fixture.
            // target 是待处理状态，而不是通用回合样本预填充的成功状态。
            let mut target = turn(&format!("window-turn-{index}"), index);
            target.status = TurnStatus::Pending;
            target.user_input_kind = None;
            target.analysis_details = None;
            target
        })
        .collect::<Vec<_>>();
    targets[4].session_source_id = SessionSourceId::new("other-session").expect("session");
    // mutations registers every turn before analysis, reproducing a populated background queue.
    // mutations 在分析前登记全部回合，复现已有积压的后台队列。
    let mut mutations = targets
        .iter()
        .map(|target| Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: target.id.to_string(),
                expected_version: 0,
            },
            turn: target.clone(),
        })
        .collect::<Vec<_>>();
    for (id, created_at, session) in [
        ("direct-first", 10, "session-compact"),
        ("direct-second", 30, "session-compact"),
        ("direct-future", 70, "session-compact"),
        ("direct-other", 15, "other-session"),
    ] {
        // direct has the same subject/project, so only the session and time window isolate it.
        // direct 具有相同主体和项目，因此由会话及时间窗口进行隔离。
        let mut direct = memory(id, "subject-compact", "scope-compact", "a direct fact");
        direct.origin_session_source_id = Some(SessionSourceId::new(session).expect("session"));
        direct.created_at_ns = created_at;
        direct.updated_at_ns = created_at;
        mutations.push(put_memory(direct, 0));
    }
    assert!(matches!(
        storage
            .mutate(&batch("window-seed", mutations))
            .expect("intake"),
        MutationOutcome::Committed { .. }
    ));
    assert_eq!(storage.snapshot().memory_sessions.len(), 2);
    for (index, cutoff, expected) in [
        (0, 20, vec!["direct-first"]),
        (1, 40, vec!["direct-second"]),
        (2, 60, vec![]),
        (4, 20, vec!["direct-other"]),
    ] {
        // target determines the sole canonical writing session for the next execution.
        // target 确定下一次执行的唯一规范写入会话。
        let target = &targets[index];
        // before is read at execution, after the previous successful commit, never at intake.
        // before 在执行时、上一次成功提交后读取，绝非入队时读取。
        let before = storage.snapshot();
        // session selects the complete ownership tuple, not a display name or bare session string.
        // session 选择完整归属元组，而不是显示名称或裸会话字符串。
        let session = before
            .memory_sessions
            .iter()
            .find(|session| {
                session.subject_id == target.subject_id
                    && session.scope_id == target.scope_id
                    && session.session_source_id == target.session_source_id
            })
            .expect("registered session");
        // query is exactly the PostAction context query consumed by the L1 input stage.
        // query 是 L1 输入阶段实际消费的 PostAction 上下文查询。
        let query = StorageQuery::PostActionContext {
            subject_id: target.subject_id.clone(),
            scope_access: ScopeAccess {
                write_scope: target.scope_id.clone(),
                visible_scopes: vec![target.scope_id.clone()],
            },
            session_source_id: target.session_source_id.clone(),
            target_turn_id: target.id.clone(),
            reference_turn_limit: 10,
            direct_write_observed_after_ns: session.last_extraction_observed_at_ns,
            analysis_cutoff_ns: cutoff,
        };
        let StorageQueryResult::PostActionContext {
            recent_direct_writes,
            ..
        } = storage.query(&query).expect("analysis context")
        else {
            panic!("PostAction context expected")
        };
        assert_eq!(
            recent_direct_writes
                .iter()
                .map(|item| item.memory_id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        // success atomically commits even the empty third extraction and its exact cutoff.
        // success 原子提交包括第三次空提取在内的成功结果及精确截止时间。
        let success = extraction_batch(
            &format!("success-{index}"),
            &target.id,
            session.last_extraction_observed_at_ns,
            cutoff,
        );
        assert!(matches!(
            storage.mutate_at(&success, cutoff + 5).expect("extraction"),
            MutationOutcome::Committed { .. }
        ));
    }
    // before_failure captures both the completed turns and the successfully advanced watermark.
    // before_failure 同时保存已完成回合及已成功推进的水位。
    let before_failure = storage.snapshot();
    // stale attempts cannot roll back the window or partially complete their target turn.
    // stale 尝试不能回退窗口，也不能部分完成其目标回合。
    let stale = extraction_batch("stale-window", &targets[3].id, 0, 80);
    assert!(matches!(
        storage.mutate_at(&stale, 85).expect("stale result"),
        MutationOutcome::Rejected { .. }
    ));
    assert_eq!(storage.snapshot(), before_failure);
    // reload proves the successful boundaries are independent durable facts, not queue estimates.
    // reload 证明成功边界是独立耐久事实，而不是队列估算值。
    let reload = InMemoryStorage::from_snapshot(before_failure.clone()).expect("reload");
    assert_eq!(reload.snapshot(), before_failure);
}

/// Verifies an extraction cannot silently invent its commit clock or a missing session record.
/// 验证提取不能静默伪造提交时钟或缺失会话记录。
#[test]
fn extraction_requires_registered_session_and_explicit_host_clock() {
    // storage is seeded through normal intake rather than manually fabricated metadata.
    // storage 通过正常登记初始化，而不是手动伪造元数据。
    let mut storage = InMemoryStorage::new();
    // target remains pending until the whole completion batch succeeds.
    // target 在整个完成批次成功之前保持待处理状态。
    let mut target = turn("clock-turn", 1);
    target.status = TurnStatus::Pending;
    target.user_input_kind = None;
    target.analysis_details = None;
    storage
        .mutate(&batch(
            "clock-seed",
            vec![Mutation::PutTurn {
                entity: EntityRef::Existing {
                    domain_id: target.id.to_string(),
                    expected_version: 0,
                },
                turn: target.clone(),
            }],
        ))
        .expect("seed");
    // before is the exact snapshot that all rejected attempts must preserve.
    // before 是所有被拒绝尝试必须保持的精确快照。
    let before = storage.snapshot();
    // success deliberately has no inserted memories, covering successful empty extraction.
    // success 故意不插入记忆，以覆盖成功空提取。
    let success = extraction_batch("clock-success", &target.id, 0, 20);
    assert!(matches!(
        storage.mutate(&success).expect("missing clock"),
        MutationOutcome::Rejected { .. }
    ));
    assert!(matches!(
        storage.mutate_at(&success, 19).expect("early clock"),
        MutationOutcome::Rejected { .. }
    ));
    assert_eq!(storage.snapshot(), before);
    assert!(matches!(
        storage.mutate_at(&success, 25).expect("valid clock"),
        MutationOutcome::Committed { .. }
    ));
    assert_eq!(
        storage.snapshot().memory_sessions[0].last_extraction_completed_at_ns,
        Some(25)
    );
    // committed must remain unchanged on exact idempotent replay even with a later host clock.
    // committed 在精确幂等重放时必须保持不变，即使宿主时钟已更晚。
    let committed = storage.snapshot();
    storage.mutate_at(&success, 90).expect("replay");
    assert_eq!(storage.snapshot(), committed);
    // corrupt removes required metadata without changing the existing pending turn.
    // corrupt 移除必需元数据，但不改变既有待处理回合。
    let mut corrupt = before;
    corrupt.memory_sessions.clear();
    let mut corrupt_storage = InMemoryStorage::from_snapshot(corrupt).expect("fixture snapshot");
    assert!(matches!(
        corrupt_storage
            .mutate_at(&success, 25)
            .expect("missing session"),
        MutationOutcome::Rejected { .. }
    ));
}

/// Verifies the vector persistence inventory retains recycle-bin embeddings for direct restore.
/// 验证向量持久化清单会保留回收站嵌入，以支持直接恢复。
#[test]
fn snapshot_memory_embeddings_include_recycle_bin_records() {
    let mut storage = InMemoryStorage::new();
    storage
        .mutate(&batch(
            "trash-vector-seed",
            vec![put_memory(
                memory("trash-vector", "subject", "scope", "retained"),
                0,
            )],
        ))
        .expect("seed memory must commit");
    let mut snapshot = storage.snapshot();
    let recycled = snapshot.dataset.memories.remove(0);
    snapshot
        .retention_trash_batches
        .push(StorageRetentionTrashBatch {
            batch_id: "trash-vector-batch".to_owned(),
            subject_id: recycled
                .source_subject_id
                .clone()
                .expect("seed memory must carry its source subject"),
            scope_id: recycled.scope_id.clone(),
            recycled_at_ns: 2,
            expires_at_ns: 100,
            reason: RetentionReasonCode::TerminalMemoryRecycle,
            manual_selection: None,
            dataset: ExportDataset {
                memories: vec![recycled],
                ..ExportDataset::empty()
            },
            analysis_attempts: Vec::new(),
        });

    let embeddings = snapshot.memory_embeddings();
    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].0.as_str(), "trash-vector");
    assert_eq!(embeddings[0].1.as_str(), "vector-1");
    assert_eq!(embeddings[0].2, vec![1.0, 0.0]);
}

/// Verifies a recycled identity remains reserved for future generated entities.
/// 验证已回收身份对后续生成实体仍保持占用。
#[test]
fn recycled_identity_survives_new_slot_allocation() {
    use vulcan_memorymesh_core_contracts::EntitySlotId;

    let mut snapshot = InMemoryStorage::new().snapshot();
    snapshot
        .retention_trash_batches
        .push(StorageRetentionTrashBatch {
            batch_id: "trash-generated-id".to_owned(),
            subject_id: SubjectId::new("subject-retention")
                .expect("subject identity must be valid"),
            scope_id: ScopeId::new("scope-retention").expect("scope identity must be valid"),
            recycled_at_ns: 1,
            expires_at_ns: 100,
            reason: RetentionReasonCode::ColdTurnRecycle,
            manual_selection: None,
            dataset: ExportDataset {
                turns: vec![retention_turn("vmm-turn-1", "trash-session", 1)],
                ..ExportDataset::empty()
            },
            analysis_attempts: Vec::new(),
        });
    let mut storage =
        InMemoryStorage::from_snapshot(snapshot).expect("one isolated trash identity must load");
    let created = retention_turn("placeholder", "active-session", 2);
    let outcome = storage
        .mutate(&batch(
            "trash-generated-id-allocation",
            vec![Mutation::PutTurn {
                entity: EntityRef::New(EntitySlotId::new("new-turn").expect("slot must be valid")),
                turn: created,
            }],
        ))
        .expect("new turn allocation must succeed");

    assert!(matches!(outcome, MutationOutcome::Committed { .. }));
    assert_eq!(storage.dataset().turns[0].id.as_str(), "vmm-turn-2");
}

/// Verifies active and recycled storage cannot claim the same durable identity.
/// 验证活动区与回收站不能声明同一个耐久身份。
#[test]
fn snapshot_rejects_active_and_recycled_identity_collision() {
    let mut snapshot: StorageSnapshot = InMemoryStorage::new().snapshot();
    let record = memory("colliding-memory", "subject", "scope", "active and trash");
    snapshot.dataset.memories.push(record.clone());
    snapshot
        .retention_trash_batches
        .push(StorageRetentionTrashBatch {
            batch_id: "trash-collision".to_owned(),
            subject_id: SubjectId::new("subject").expect("subject identity must be valid"),
            scope_id: ScopeId::new("scope").expect("scope identity must be valid"),
            recycled_at_ns: 1,
            expires_at_ns: 100,
            reason: RetentionReasonCode::TerminalMemoryRecycle,
            manual_selection: None,
            dataset: ExportDataset {
                memories: vec![record],
                ..ExportDataset::empty()
            },
            analysis_attempts: Vec::new(),
        });

    let error = InMemoryStorage::from_snapshot(snapshot)
        .expect_err("active and recycled identities must not collide");
    assert!(error.contains("active-or-trash memory identity collision"));
}

/// Builds one structurally valid mutation identity for direct storage tests.
/// 为直接存储测试构建一个结构有效的变更身份。
fn identity(key: &str) -> MutationIdentity {
    MutationIdentity {
        operation_id: OperationId::new(format!("operation-{key}"))
            .expect("operation identity must be valid"),
        idempotency_key: IdempotencyKey::new(format!("idempotency-{key}"))
            .expect("idempotency identity must be valid"),
        request_fingerprint: format!("fp256:{}", "0".repeat(64)),
        source_kind: MutationSourceKind::HostRpc,
        request_source_id: "storage-contract-test".to_owned(),
        observed_at_ns: 0,
    }
}

/// Returns complete candidate facts from one immutable storage borrow and filters expiration before limiting.
/// 从一次不可变存储借用返回完整候选事实，并在限量前过滤过期记录；覆盖词法开关、并集及记录快照。
#[test]
fn candidate_observation_retains_complete_union() {
    use vulcan_memorymesh_core_contracts::CandidateObservationQuery;
    use vulcan_memorymesh_core_storage::REFERENCE_LEXICAL_CONTRACT_ID;
    let mut storage = InMemoryStorage::new();
    let valid = memory("valid", "subject", "scope", "needle");
    let mut expired = memory("expired", "subject", "scope", "needle");
    expired.expires_at_ns = Some(10);
    let foreign = memory("foreign", "other", "scope", "needle");
    storage
        .mutate(&batch(
            "observation-seed",
            vec![
                put_memory(valid, 0),
                put_memory(expired, 0),
                put_memory(foreign, 0),
            ],
        ))
        .expect("seed");
    let mut query = CandidateObservationQuery {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: ScopeId::new("scope").expect("scope"),
            visible_scopes: vec![ScopeId::new("scope").expect("scope")],
        },
        query: "needle".to_owned(),
        query_embedding: vec![1.0, 0.0],
        vector_space_id: VectorSpaceId::new("vector-1").expect("vector"),
        boundary: CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        limit: 1,
        now_ns: 10,
        vector_contract: ScoreContractRef {
            id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
            version: REFERENCE_CONTRACT_VERSION,
        },
        lexical_contract: None,
    };
    for enabled in [false, true] {
        query.lexical_contract = enabled.then(|| ScoreContractRef {
            id: REFERENCE_LEXICAL_CONTRACT_ID.to_owned(),
            version: REFERENCE_CONTRACT_VERSION,
        });
        let StorageQueryResult::CandidateObservation(observed) = storage
            .query(&StorageQuery::CandidateObservation(query.clone()))
            .expect("complete read")
        else {
            unreachable!("observation result")
        };
        assert_eq!(observed.vector_channel.candidates.len(), 1);
        assert_eq!(observed.vector_channel.candidates[0].memory_id, "valid");
        assert_eq!(observed.lexical_channel.is_some(), enabled);
        if let Some(lexical) = observed.lexical_channel {
            assert_eq!(lexical.candidates[0].memory_id, "valid");
        }
        assert_eq!(observed.memories.len(), 1);
        assert_eq!(observed.memories[0].version, 1);
        assert_eq!(observed.memories[0].content, "needle");
    }
    query.lexical_contract = Some(ScoreContractRef {
        id: "foreign-producer".to_owned(),
        version: 1,
    });
    assert!(
        storage
            .query(&StorageQuery::CandidateObservation(query))
            .is_err(),
        "one failing channel must reject the entire observation"
    );
}

/// Builds one mutation batch with stable operation identities.
/// 使用稳定操作身份构建一个变更批次。
fn batch(key: &str, mutations: Vec<Mutation>) -> MutationBatch {
    MutationBatch {
        identity: identity(key),
        mutations,
    }
}

/// Builds one valid vectorized memory in an exact ownership scope.
/// 在精确所有权作用域中构建一条有效向量化记忆。
fn memory(identity: &str, subject: &str, scope: &str, content: &str) -> MemoryRecord {
    MemoryRecord {
        id: MemoryId::new(identity).expect("memory identity must be valid"),
        access_class: vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(SubjectId::new(subject).expect("subject identity must be valid")),
        scope_id: ScopeId::new(scope).expect("scope identity must be valid"),
        scope_class: vulcan_memorymesh_core_contracts::MemoryScopeClass::Environment,
        category: MemoryCategory::General,
        summary: content.to_owned(),
        content: content.to_owned(),
        source_kind: MemorySourceKind::Direct,
        origin_session_source_id: None,
        priority: 50,
        stability: vulcan_memorymesh_core_contracts::MemoryStability::Stable,
        expires_at_ns: None,
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        tags: Vec::new(),
        evidence: Vec::new(),
        evidence_source: None,
        state: MemoryState::Active,
        importance: 0.5,
        confidence: Some(1.0),
        created_at_ns: 1,
        updated_at_ns: 1,
        version: 0,
        vector_space_id: Some(
            VectorSpaceId::new("vector-1").expect("vector identity must be valid"),
        ),
        embedding: Some(vec![1.0, 0.0]),
        memory_level: MemoryLevel::Stable,
        refresh_weight: 0,
        dedupe_hash: None,
        recalled_count: 0,
        adopted_count: 0,
        cross_session_adopted_count: 0,
        decay_disabled: false,
        status_reason: None,
        last_adopted_at_ns: None,
        last_reinforced_at_ns: None,
        source_turn_id: None,
    }
}

/// Builds one persisted turn for exact compact-boundary selection tests.
/// 为精确压缩边界选择测试构建一个持久化轮次。
fn turn(identity: &str, created_at_ns: i64) -> Turn {
    Turn {
        id: TurnId::new(identity).expect("turn identity must be valid"),
        subject_id: SubjectId::new("subject-compact").expect("subject identity must be valid"),
        scope_id: ScopeId::new("scope-compact").expect("scope identity must be valid"),
        session_source_id: SessionSourceId::new("session-compact")
            .expect("session identity must be valid"),
        timeline: vec![TimelineItem {
            role: Role::User,
            content: format!("turn at {created_at_ns}"),
            occurred_at_ns: created_at_ns,
        }],
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns,
        status: TurnStatus::Completed,
        user_input_kind: Some(vulcan_memorymesh_core_contracts::TurnInputKind::Statement),
        analysis_details: Some("storage fixture analysis".to_owned()),
    }
}

/// Builds one completed retention turn for a chosen session and activity time.
/// 为指定会话与活动时间构建一个已完成的保留策略测试回合。
fn retention_turn(identity: &str, session: &str, created_at_ns: i64) -> Turn {
    let mut value = turn(identity, created_at_ns);
    value.subject_id = SubjectId::new("subject-retention").expect("subject identity must be valid");
    value.scope_id = ScopeId::new("scope-retention").expect("scope identity must be valid");
    value.session_source_id =
        SessionSourceId::new(session).expect("session identity must be valid");
    value
}

/// Verifies persisted deadlines survive policy edits and due filtering precedes pagination.
/// 验证持久化截止时间不受策略修改影响，且先筛选到期批次再分页。
#[test]
fn retention_scan_uses_frozen_trash_deadlines_before_pagination() {
    // An older long-lived batch must not starve a newer batch with a shorter frozen window.
    // 较早创建的长期批次不能阻塞较晚创建但冻结窗口较短的批次。
    let mut snapshot = InMemoryStorage::new().snapshot();
    for (id, recycled_at_ns, expires_at_ns) in [
        ("older-live", 1, 1_000),
        ("newer-due", 2, 100),
        ("next-due", 3, 101),
    ] {
        snapshot
            .retention_trash_batches
            .push(StorageRetentionTrashBatch {
                batch_id: id.to_owned(),
                subject_id: SubjectId::new("subject-retention").expect("subject"),
                scope_id: ScopeId::new("scope-retention").expect("scope"),
                recycled_at_ns,
                expires_at_ns,
                reason: RetentionReasonCode::ColdTurnRecycle,
                manual_selection: None,
                dataset: ExportDataset::empty(),
                analysis_attempts: Vec::new(),
            });
    }
    let storage = InMemoryStorage::from_snapshot(snapshot).expect("frozen batches load");
    for trash_retention_ms in [1, RetentionPolicy::v1().trash_retention_ms, u64::MAX] {
        let mut policy = RetentionPolicy::v1();
        policy.trash_retention_ms = trash_retention_ms;
        policy.trash_purge_limit = 1;
        for (now_ns, expected_count) in [(99, 0), (100, 1), (101, 1)] {
            let StorageQueryResult::RetentionScan(facts) = storage
                .query(&StorageQuery::RetentionScan {
                    subject_id: SubjectId::new("subject-retention").expect("subject"),
                    scope_access: ScopeAccess {
                        write_scope: ScopeId::new("scope-retention").expect("scope"),
                        visible_scopes: vec![ScopeId::new("scope-retention").expect("scope")],
                    },
                    now_ns,
                    policy: policy.clone(),
                })
                .expect("scan")
            else {
                panic!("expected retention facts");
            };
            assert_eq!(facts.trash_batches.len(), expected_count);
            if expected_count == 1 {
                assert_eq!(facts.trash_batches[0].batch_id, "newer-due");
                assert_eq!(facts.trash_batches[0].expires_at_ns, 100);
            }
        }
    }
}

/// Verifies non-forward trash deadlines are rejected when loading a snapshot.
/// 验证加载快照时会拒绝未向前推进的回收截止时间。
#[test]
fn trash_deadline_is_required_and_strictly_after_recycle_time() {
    let mut snapshot = InMemoryStorage::new().snapshot();
    let mut batch = StorageRetentionTrashBatch {
        batch_id: "invalid-deadline".to_owned(),
        subject_id: SubjectId::new("subject-retention").expect("subject"),
        scope_id: ScopeId::new("scope-retention").expect("scope"),
        recycled_at_ns: 10,
        expires_at_ns: 11,
        reason: RetentionReasonCode::ColdTurnRecycle,
        manual_selection: None,
        dataset: ExportDataset::empty(),
        analysis_attempts: Vec::new(),
    };
    for deadline in [-1, 0, 9, 10] {
        batch.expires_at_ns = deadline;
        snapshot.retention_trash_batches = vec![batch.clone()];
        let error = InMemoryStorage::from_snapshot(snapshot.clone()).expect_err("invalid deadline");
        assert!(error.contains("expiry must follow"));
    }
}

/// Verifies cold-turn pagination selects the oldest session activity before lexical identity.
/// 验证冷回合分页按最久会话活动优先，而不是按字典序身份优先。
#[test]
fn retention_scan_pages_cold_sessions_by_oldest_activity() {
    let mut storage = InMemoryStorage::new();
    let mut mutations = Vec::new();
    for index in 0_i64..9 {
        let old = retention_turn(
            format!("old-turn-{index}").as_str(),
            "z-old-session",
            index + 1,
        );
        mutations.push(Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: old.id.as_str().to_owned(),
                expected_version: 0,
            },
            turn: old,
        });
        let recent = retention_turn(
            format!("recent-turn-{index}").as_str(),
            "a-recent-session",
            index + 101,
        );
        mutations.push(Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: recent.id.as_str().to_owned(),
                expected_version: 0,
            },
            turn: recent,
        });
    }
    storage
        .mutate(&batch("retention-oldest-session", mutations))
        .expect("retention turns must commit");

    let mut policy = RetentionPolicy::v1();
    policy.cold_turn_scan_limit = 1;
    let result = storage
        .query(&StorageQuery::RetentionScan {
            subject_id: SubjectId::new("subject-retention")
                .expect("subject identity must be valid"),
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("scope-retention").expect("scope identity must be valid"),
                visible_scopes: vec![
                    ScopeId::new("scope-retention").expect("scope identity must be valid"),
                ],
            },
            now_ns: 1_000,
            policy,
        })
        .expect("retention scan must succeed");
    let StorageQueryResult::RetentionScan(facts) = result else {
        panic!("retention scan must return retention facts");
    };

    assert_eq!(facts.cold_turn_candidates.len(), 1);
    assert_eq!(
        facts.cold_turn_candidates[0].session_source_id.as_str(),
        "z-old-session"
    );
}

/// Verifies extraction and compaction activity update the retention session activity boundary.
/// 验证提取与压缩活动会更新保留策略使用的会话活动边界。
#[test]
fn retention_scan_uses_analysis_and_compaction_as_session_activity() {
    let mut storage = InMemoryStorage::new();
    let mut mutations = Vec::new();
    for index in 0_i64..9 {
        let analyzed = retention_turn(
            format!("analyzed-turn-{index}").as_str(),
            "z-analyzed-session",
            index + 1,
        );
        mutations.push(Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: analyzed.id.as_str().to_owned(),
                expected_version: 0,
            },
            turn: analyzed,
        });
        let untouched = retention_turn(
            format!("untouched-turn-{index}").as_str(),
            "a-untouched-session",
            index + 101,
        );
        mutations.push(Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: untouched.id.as_str().to_owned(),
                expected_version: 0,
            },
            turn: untouched,
        });
    }
    mutations.push(Mutation::PutChatCompactBoundary(ChatCompactBoundary {
        subject_id: SubjectId::new("subject-retention").expect("subject identity must be valid"),
        scope_id: ScopeId::new("scope-retention").expect("scope identity must be valid"),
        session_source_id: SessionSourceId::new("z-analyzed-session")
            .expect("session identity must be valid"),
        compacted_turn_id: TurnId::new("analyzed-turn-7").expect("turn identity must be valid"),
        compacted_at_ns: 800,
    }));
    // attempt_identity records the newer extraction observation that must dominate compaction.
    // attempt_identity 记录必须晚于压缩活动的最新提取观察时间。
    let mut attempt_identity = identity("retention-session-analysis");
    attempt_identity.observed_at_ns = 900;
    mutations.push(Mutation::PutAnalysisAttempt(AnalysisAttemptIdentity {
        turn_id: TurnId::new("analyzed-turn-8").expect("turn identity must be valid"),
        analysis_generation: 1,
        mutation_identity: attempt_identity,
        config_snapshot_id: ConfigSnapshotId::new("retention-session-activity")
            .expect("snapshot identity must be valid"),
        structured_output_contracts: vec![StructuredOutputContractId::PostActionAnalysis],
        score_contracts: Vec::new(),
    }));
    storage
        .mutate(&batch("retention-session-activity", mutations))
        .expect("retention activity fixtures must commit");

    let mut policy = RetentionPolicy::v1();
    policy.cold_turn_scan_limit = 1;
    policy.idle_session_limit = 2;
    let StorageQueryResult::RetentionScan(facts) = storage
        .query(&StorageQuery::RetentionScan {
            subject_id: SubjectId::new("subject-retention")
                .expect("subject identity must be valid"),
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("scope-retention").expect("scope identity must be valid"),
                visible_scopes: vec![
                    ScopeId::new("scope-retention").expect("scope identity must be valid"),
                ],
            },
            now_ns: 1_000,
            policy,
        })
        .expect("retention scan must succeed")
    else {
        panic!("retention scan must return retention facts");
    };

    assert_eq!(facts.cold_turn_candidates.len(), 1);
    assert_eq!(
        facts.cold_turn_candidates[0].session_source_id.as_str(),
        "a-untouched-session",
        "the recently analyzed session must no longer win oldest-first pagination"
    );
    let analyzed = facts
        .idle_sessions
        .iter()
        .find(|session| session.session_source_id.as_str() == "z-analyzed-session")
        .expect("analyzed session must remain visible to idle planning");
    assert_eq!(analyzed.last_activity_ns, 900);
}

/// Verifies idle retention discovers a session established only by one direct session memory.
/// 验证闲置保留能够发现仅由一条主动会话记忆建立的会话。
#[test]
fn retention_scan_includes_direct_memory_only_sessions() {
    let mut storage = InMemoryStorage::new();
    let mut direct = memory(
        "direct-session-memory",
        "subject-retention",
        "scope-retention",
        "direct session fact",
    );
    direct.scope_class = MemoryScopeClass::Session;
    direct.origin_session_source_id =
        Some(SessionSourceId::new("direct-only-session").expect("session identity must be valid"));
    direct.expires_at_ns = Some(2);
    storage
        .mutate(&batch(
            "retention-direct-session",
            vec![put_memory(direct, 0)],
        ))
        .expect("direct session memory must commit");

    let mut policy = RetentionPolicy::v1();
    policy.session_idle_ms = 1;
    let StorageQueryResult::RetentionScan(facts) = storage
        .query(&StorageQuery::RetentionScan {
            subject_id: SubjectId::new("subject-retention")
                .expect("subject identity must be valid"),
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("scope-retention").expect("scope identity must be valid"),
                visible_scopes: vec![
                    ScopeId::new("scope-retention").expect("scope identity must be valid"),
                ],
            },
            now_ns: 2_000_000,
            policy,
        })
        .expect("retention scan must succeed")
    else {
        panic!("retention scan must return retention facts");
    };

    assert_eq!(facts.idle_sessions.len(), 1);
    assert_eq!(
        facts.idle_sessions[0].session_source_id.as_str(),
        "direct-only-session"
    );
    assert_eq!(facts.idle_sessions[0].last_activity_ns, 1);
    assert!(!facts.idle_sessions[0].has_pending_turn);
}

/// Verifies protected shared terminal memories never fill the recycle limit ahead of recyclable rows.
/// 验证受保护的共享终态记忆不会抢在可回收行之前占满回收名额。
#[test]
fn retention_scan_skips_protected_terminal_memories_before_limit() {
    let mut storage = InMemoryStorage::new();
    let mut protected = memory(
        "protected-shared-memory",
        "subject-retention",
        "scope-retention",
        "protected shared fact",
    );
    protected.priority = 100;
    protected.state = MemoryState::Deleted;
    protected.updated_at_ns = 1;
    let mut recyclable = memory(
        "recyclable-session-memory",
        "subject-retention",
        "scope-retention",
        "recyclable session fact",
    );
    recyclable.scope_class = MemoryScopeClass::Session;
    recyclable.origin_session_source_id =
        Some(SessionSourceId::new("retention-session").expect("session identity must be valid"));
    recyclable.priority = 20;
    recyclable.state = MemoryState::Deleted;
    recyclable.created_at_ns = 2;
    recyclable.updated_at_ns = 2;
    storage
        .mutate(&batch(
            "retention-protected-before-limit",
            vec![put_memory(protected, 0), put_memory(recyclable, 0)],
        ))
        .expect("terminal memories must commit");

    let mut policy = RetentionPolicy::v1();
    policy.memory_recycle_limit = 1;
    let StorageQueryResult::RetentionScan(facts) = storage
        .query(&StorageQuery::RetentionScan {
            subject_id: SubjectId::new("subject-retention")
                .expect("subject identity must be valid"),
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("scope-retention").expect("scope identity must be valid"),
                visible_scopes: vec![
                    ScopeId::new("scope-retention").expect("scope identity must be valid"),
                ],
            },
            now_ns: 2_000_000,
            policy,
        })
        .expect("retention scan must succeed")
    else {
        panic!("retention scan must return retention facts");
    };

    assert_eq!(facts.terminal_memories.len(), 1);
    assert_eq!(
        facts.terminal_memories[0].memory.id.as_str(),
        "recyclable-session-memory"
    );
}

/// Verifies completed and legacy details-bearing turns become details-only analysis references while invalid terminal states stay excluded.
/// 验证Completed与保留历史详情的轮次可成为仅详情型分析参考，同时无资格终态仍被排除。
#[test]
fn post_action_references_preserve_legacy_details_without_promoting_invalid_history() {
    let completed = turn("turn-completed", 10);
    let mut unknown = turn("turn-unknown", 20);
    unknown.status = TurnStatus::ProcessedUnknown;
    unknown.user_input_kind = None;
    unknown.analysis_details = Some("known details without a known input kind".to_owned());
    let mut unknown_without_details = turn("turn-unknown-without-details", 25);
    unknown_without_details.status = TurnStatus::ProcessedUnknown;
    unknown_without_details.user_input_kind = None;
    unknown_without_details.analysis_details = None;
    let mut bypassed = turn("turn-bypassed", 30);
    bypassed.status = TurnStatus::AnalysisBypassed;
    bypassed.user_input_kind = None;
    bypassed.analysis_details = None;
    let mut corrupted = turn("turn-corrupted", 40);
    corrupted.status = TurnStatus::Corrupted;
    corrupted.user_input_kind = None;
    corrupted.analysis_details = None;
    let mut target = turn("turn-target", 50);
    target.status = TurnStatus::Pending;
    target.user_input_kind = None;
    target.analysis_details = None;

    let turns = [
        completed.clone(),
        unknown.clone(),
        unknown_without_details,
        bypassed.clone(),
        corrupted.clone(),
        target.clone(),
    ];
    let mutations = turns
        .into_iter()
        .map(|record| Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: record.id.as_str().to_owned(),
                expected_version: 0,
            },
            turn: record,
        })
        .collect();
    let mut storage = InMemoryStorage::new();
    assert!(matches!(
        storage
            .mutate(&batch("turn-history-seed", mutations))
            .expect("seed turn history"),
        MutationOutcome::Committed { .. }
    ));

    let access = ScopeAccess {
        write_scope: ScopeId::new("scope-compact").expect("scope"),
        visible_scopes: vec![ScopeId::new("scope-compact").expect("scope")],
    };
    let StorageQueryResult::PostActionContext {
        reference_turns, ..
    } = storage
        .query(&StorageQuery::PostActionContext {
            subject_id: SubjectId::new("subject-compact").expect("subject"),
            scope_access: access.clone(),
            session_source_id: SessionSourceId::new("session-compact").expect("session"),
            target_turn_id: target.id,
            reference_turn_limit: 10,
            direct_write_observed_after_ns: 0,
            analysis_cutoff_ns: 50,
        })
        .expect("post-action context")
    else {
        unreachable!("post-action context result")
    };
    assert_eq!(reference_turns.len(), 2);
    assert_eq!(reference_turns[0].turn_id, completed.id);
    assert_eq!(reference_turns[1].turn_id, unknown.id);

    let StorageQueryResult::DetailsBatch { turns, .. } = storage
        .query(&StorageQuery::DetailsBatch {
            subject_id: SubjectId::new("subject-compact").expect("subject"),
            scope_access: access,
            memory_ids: Vec::new(),
            turn_ids: vec![unknown.id, bypassed.id, corrupted.id],
        })
        .expect("history details")
    else {
        unreachable!("details result")
    };
    assert_eq!(
        turns
            .into_iter()
            .map(|turn| turn.expect("history turn").status)
            .collect::<Vec<_>>(),
        vec![
            TurnStatus::ProcessedUnknown,
            TurnStatus::AnalysisBypassed,
            TurnStatus::Corrupted,
        ]
    );
}

/// Verifies `PostAction` context keeps every direct-memory lifecycle horizon independently from its physical scope.
/// 验证 `PostAction` 上下文独立于物理作用域保留每条主动记忆的全部生命周期跨度。
#[test]
fn post_action_direct_writes_preserve_scope_classes() {
    let mut target = turn("turn-scope-class-target", 40);
    target.status = TurnStatus::Pending;
    target.user_input_kind = None;
    target.analysis_details = None;
    let session = SessionSourceId::new("session-compact").expect("session");
    let mut mutations = vec![Mutation::PutTurn {
        entity: EntityRef::Existing {
            domain_id: target.id.as_str().to_owned(),
            expected_version: 0,
        },
        turn: target.clone(),
    }];
    for (index, scope_class) in [
        MemoryScopeClass::Session,
        MemoryScopeClass::Environment,
        MemoryScopeClass::Subject,
    ]
    .into_iter()
    .enumerate()
    {
        let mut record = memory(
            &format!("direct-scope-{index}"),
            "subject-compact",
            "scope-compact",
            "direct scope fact",
        );
        record.scope_class = scope_class;
        record.origin_session_source_id = Some(session.clone());
        record.created_at_ns = 10 + i64::try_from(index).expect("small fixture index");
        record.updated_at_ns = record.created_at_ns;
        mutations.push(put_memory(record, 0));
    }
    let mut storage = InMemoryStorage::new();
    assert!(matches!(
        storage
            .mutate(&batch("postaction-scope-class", mutations))
            .expect("seed PostAction scope classes"),
        MutationOutcome::Committed { .. }
    ));
    let StorageQueryResult::PostActionContext {
        recent_direct_writes,
        ..
    } = storage
        .query(&StorageQuery::PostActionContext {
            subject_id: SubjectId::new("subject-compact").expect("subject"),
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("scope-compact").expect("scope"),
                visible_scopes: vec![ScopeId::new("scope-compact").expect("scope")],
            },
            session_source_id: session,
            target_turn_id: target.id,
            reference_turn_limit: 10,
            direct_write_observed_after_ns: 0,
            analysis_cutoff_ns: 50,
        })
        .expect("query PostAction scope classes")
    else {
        unreachable!("PostAction context result")
    };
    assert_eq!(
        recent_direct_writes
            .into_iter()
            .map(|value| value.scope_class)
            .collect::<Vec<_>>(),
        vec![
            MemoryScopeClass::Session,
            MemoryScopeClass::Environment,
            MemoryScopeClass::Subject,
        ]
    );
}

/// Builds one caller-assigned create mutation for one memory record.
/// 为一条记忆记录构建一个调用方分配身份的创建变更。
fn put_memory(record: MemoryRecord, expected_version: u64) -> Mutation {
    Mutation::PutMemory {
        entity: EntityRef::Existing {
            domain_id: record.id.as_str().to_owned(),
            expected_version,
        },
        record,
    }
}

/// Keeps wide adoption counters, refresh progression and explicit origin-session attribution aligned.
/// 保持宽采纳计数、刷新权重递增及显式来源会话归因一致，并验证重放不会重复递增。
#[test]
fn wide_adoption_counters_follow_explicit_record_origin() {
    for initial in [
        0,
        u64::from(u32::MAX) - 1,
        u64::from(u32::MAX),
        (1_u64 << 53) + 1,
        u64::MAX,
    ] {
        for origin in [None, Some("current"), Some("different")] {
            let mut storage = InMemoryStorage::new();
            let mut row = memory("wide-memory", "subject", "scope", "durable counter");
            row.origin_session_source_id =
                origin.map(|value| SessionSourceId::new(value).expect("origin"));
            row.refresh_weight = initial;
            row.recalled_count = initial;
            row.adopted_count = initial;
            row.reinforcement_count = initial;
            row.cross_session_adopted_count = initial;
            let seed = batch("wide-seed", vec![put_memory(row.clone(), 0)]);
            assert!(matches!(
                storage.mutate(&seed).expect("seed"),
                MutationOutcome::Committed { .. }
            ));
            let adoption = batch(
                "wide-adopt",
                vec![Mutation::ReinforceMemory {
                    actor_subject_id: SubjectId::new("subject").expect("subject"),
                    scope_id: row.scope_id.clone(),
                    expected_access: vulcan_memorymesh_core_contracts::MemoryAccessIdentity::from(
                        &row,
                    ),
                    memory_id: row.id.clone(),
                    expected_version: 1,
                    recalled_at_ns: 200,
                    source_session_id: Some(SessionSourceId::new("current").expect("current")),
                }],
            );
            let outcome = storage.mutate(&adoption).expect("adopt");
            assert!(matches!(outcome, MutationOutcome::Committed { .. }));
            let dataset = storage.dataset();
            let updated = &dataset.memories[0];
            assert_eq!(updated.refresh_weight, initial.max(1).saturating_add(1));
            assert_eq!(updated.recalled_count, initial.saturating_add(1));
            assert_eq!(updated.adopted_count, initial.saturating_add(1));
            assert_eq!(updated.reinforcement_count, initial.saturating_add(1));
            assert_eq!(
                updated.cross_session_adopted_count,
                if origin == Some("different") {
                    initial.saturating_add(1)
                } else {
                    initial
                }
            );
            assert_eq!(
                updated.origin_session_source_id,
                row.origin_session_source_id
            );
            assert_eq!(storage.mutate(&adoption).expect("replay"), outcome);
            assert_eq!(storage.dataset(), dataset);
        }
    }
}

/// Imported identities must not be overwritten by a fresh reference host's local sequence.
/// 导入身份不得被新参考宿主的局部序列覆盖。
#[test]
#[allow(clippy::too_many_lines)]
fn imported_identity_survives_new_slot_allocation() {
    use vulcan_memorymesh_core_contracts::{
        EntitySlotId, ProfileLevel, ProfileNode, ProfileNodeId, ProfileNodeState, ProfileSourceKind,
    };
    for kind in [
        EntityKind::Turn,
        EntityKind::Memory,
        EntityKind::ProfileNode,
    ] {
        let mut storage = InMemoryStorage::new();
        let slot = EntityRef::New(EntitySlotId::new("new-slot").expect("slot"));
        let (restore, create) = match kind {
            EntityKind::Turn => {
                let old = turn("vmm-turn-1", 1);
                (
                    Mutation::PutTurn {
                        entity: EntityRef::Existing {
                            domain_id: old.id.to_string(),
                            expected_version: 0,
                        },
                        turn: old,
                    },
                    Mutation::PutTurn {
                        entity: slot,
                        turn: turn("placeholder", 2),
                    },
                )
            }
            EntityKind::Memory => {
                let mut old = memory("vmm-memory-1", "old-subject", "scope", "old memory");
                old.version = 7;
                (
                    Mutation::RestoreImportedMemory(old),
                    Mutation::PutMemory {
                        entity: slot,
                        record: memory("placeholder", "new-subject", "scope", "new memory"),
                    },
                )
            }
            EntityKind::ProfileNode => {
                let old = ProfileNode {
                    id: ProfileNodeId::new("vmm-profile-1").expect("profile"),
                    subject_id: Some(SubjectId::new("old-subject").expect("subject")),
                    scope_id: ScopeId::new("scope").expect("scope"),
                    content: "old profile".to_owned(),
                    confidence: Some(1.0),
                    state: ProfileNodeState::Active,
                    source_turn_ids: Vec::new(),
                    source_kind: ProfileSourceKind::SystemSeed,
                    source_id: None,
                    evidence_source: None,
                    priority: 50,
                    profile_level: ProfileLevel::Stable,
                    level_reason: None,
                    refresh_weight: 0,
                    effective_date_ns: None,
                    expires_at_ns: None,
                    status_reason: None,
                    superseded_by: None,
                    version: 7,
                    created_at_ns: 1,
                    updated_at_ns: 1,
                };
                let mut new = old.clone();
                new.id = ProfileNodeId::new("placeholder").expect("placeholder");
                new.subject_id = Some(SubjectId::new("new-subject").expect("subject"));
                new.content = "new profile".to_owned();
                (
                    Mutation::RestoreImportedProfileNode(old),
                    Mutation::PutProfileNode {
                        entity: slot,
                        node: new,
                    },
                )
            }
            EntityKind::ContextEvidence => unreachable!("explicit test kinds"),
        };
        let mut imported = batch("import-existing", vec![restore]);
        imported.identity.source_kind = MutationSourceKind::Import;
        assert!(matches!(
            storage.mutate(&imported).expect("restore"),
            MutationOutcome::Committed { .. }
        ));
        let before = storage.dataset();
        let created = batch("create-slot", vec![create]);
        let outcome = storage.mutate(&created).expect("create");
        assert!(matches!(outcome, MutationOutcome::Committed { .. }));
        let after = storage.dataset();
        assert!(
            before.turns.iter().all(|row| after.turns.contains(row)),
            "turn was overwritten"
        );
        assert!(
            before
                .memories
                .iter()
                .all(|row| after.memories.contains(row)),
            "memory was overwritten"
        );
        assert!(
            before
                .profile_nodes
                .iter()
                .all(|row| after.profile_nodes.contains(row)),
            "profile was overwritten"
        );
        assert_eq!(
            before.turns.len() + before.memories.len() + before.profile_nodes.len() + 1,
            after.turns.len() + after.memories.len() + after.profile_nodes.len()
        );
        assert_eq!(storage.mutate(&created).expect("replay"), outcome);
        assert_eq!(storage.dataset(), after);
    }
}

/// Ordinary deletion must remove only the deleted memory's dependent context rows atomically.
/// 普通删除必须原子移除且仅移除被删记忆的依赖情境行。
#[test]
fn memory_deletion_cascades_context_evidence() {
    let mut storage = InMemoryStorage::new();
    let records = [
        memory("remove", "subject", "scope", "remove"),
        memory("keep", "subject", "scope", "keep"),
    ];
    let mut mutations = Vec::new();
    for record in &records {
        mutations.push(put_memory(record.clone(), 0));
        mutations.push(Mutation::AggregateContextEvidence {
            memory: EntityRef::Existing {
                domain_id: record.id.to_string(),
                expected_version: 1,
            },
            context_key: "project".to_owned(),
            context_value: "core".to_owned(),
            relation: ContextEvidenceRelation::Support,
            occurred_at_ns: 1,
        });
    }
    assert!(matches!(
        storage
            .mutate(&batch("seed-delete", mutations))
            .expect("seed"),
        MutationOutcome::Committed { .. }
    ));
    let delete = Mutation::DeleteMemory {
        actor_subject_id: records[0]
            .source_subject_id
            .clone()
            .expect("private source subject"),
        scope_id: records[0].scope_id.clone(),
        expected_access: vulcan_memorymesh_core_contracts::MemoryAccessIdentity::from(&records[0]),
        memory_id: records[0].id.clone(),
        expected_version: Some(1),
    };
    assert!(matches!(
        storage
            .mutate(&batch("delete", vec![delete]))
            .expect("delete"),
        MutationOutcome::Committed { .. }
    ));
    let after = storage.dataset();
    assert_eq!(after.memories.len(), 1);
    assert_eq!(after.context_evidence.len(), 1);
    assert_eq!(after.context_evidence[0].memory_id, records[1].id);
}

/// Preserves real lexical score facts while rejecting unauthorized and inactive physical hits.
/// 保留真实词法评分事实，同时拒绝未授权及失效的物理命中。
#[test]
fn external_lexical_materialization_filters_before_limit_and_preserves_scores() {
    use vulcan_memorymesh_core_contracts::{
        PhysicalCandidate, PhysicalCandidateScore, RawScoreKind, ScoreDirection,
    };
    // storage contains higher-ranked invalid records followed by two valid records.
    // storage 包含排名靠前的无效记录，以及其后的两条有效记录。
    let mut storage = InMemoryStorage::new();
    // records share text so this test cannot accidentally pass by recomputing token overlap.
    // records 共用文本，避免本测试因为重新计算词元重合度而意外通过。
    let mut records = (0..9)
        .map(|index| memory(&format!("fts-{index}"), "subject", "scope", "同一摘要"))
        .collect::<Vec<_>>();
    for record in &mut records {
        record.origin_session_source_id = Some(SessionSourceId::new("selected").expect("session"));
    }
    records[0].source_subject_id = Some(SubjectId::new("other-subject").expect("subject"));
    records[1].scope_id = ScopeId::new("hidden-scope").expect("scope");
    records[2].vector_space_id = Some(VectorSpaceId::new("other-vector").expect("vector"));
    records[3].expires_at_ns = Some(10);
    records[4].state = MemoryState::Deleted;
    records[5].origin_session_source_id =
        Some(SessionSourceId::new("other-session").expect("session"));
    records[6].origin_session_source_id = None;
    assert!(matches!(
        storage
            .mutate(&batch(
                "fts-seed",
                records
                    .into_iter()
                    .map(|record| put_memory(record, 0))
                    .collect()
            ))
            .expect("seed"),
        MutationOutcome::Committed { .. }
    ));
    // contract names the test producer without reusing the reference token-overlap identity.
    // contract 命名测试生产者，不复用参考词元重合度身份。
    let contract = ScoreContractRef {
        id: "test-sqlite-bm25".to_owned(),
        version: 1,
    };
    // candidates keep negative SQLite BM25 values and zero-based physical ranks, including gaps after filtering.
    // candidates 保留 SQLite 负 BM25 值及从零开始的物理名次，包括过滤后留下的间隙。
    let candidates = (0..9_u32)
        .map(|index| PhysicalCandidate {
            memory_id: format!("fts-{index}"),
            physical_rank: index,
            score: PhysicalCandidateScore {
                raw_value: -10.0 + f64::from(index),
                raw_score_kind: RawScoreKind::Bm25,
                direction: ScoreDirection::LowerIsBetter,
                decision_score: None,
                contract: contract.clone(),
            },
        })
        .collect::<Vec<_>>();
    // query admits only this subject, project, vector generation, session and lifecycle time.
    // query 只允许当前主体、项目、向量代次、会话和生命周期时刻。
    let mut query = StorageQuery::LexicalCandidates {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: ScopeId::new("scope").expect("scope"),
            visible_scopes: vec![ScopeId::new("scope").expect("scope")],
        },
        queries: vec!["同一摘要".to_owned()],
        vector_space_id: VectorSpaceId::new("vector-1").expect("vector"),
        boundary: CandidateQueryBoundary {
            candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                scope_ids: vec![ScopeId::new("scope").expect("scope")],
                origin_session_source_id: Some(SessionSourceId::new("selected").expect("session")),
            }),
            session_recall_boundary: None,
        },
        limit: 1,
        expected_contract: contract,
        now_ns: 10,
    };
    // result must retain the eighth physical hit, not truncate the unfiltered first hit.
    // result 必须保留第八条物理命中，而不是先截断未过滤的首条命中。
    let result = storage
        .materialize_external_lexical_candidates(&query, candidates.clone())
        .expect("materialize");
    assert_eq!(result.candidates, vec![candidates[7].clone()]);
    assert_eq!(result.memories[0].id.as_str(), "fts-7");
    assert!(
        storage
            .materialize_external_vector_candidates(&query, candidates.clone())
            .is_err()
    );
    // wrong_contract must fail even when its candidate would otherwise be filtered out.
    // wrong_contract 即使原本会被可见性过滤，也必须明确失败。
    let mut wrong_contract = candidates.clone();
    wrong_contract[0].score.contract.version = 2;
    assert_eq!(
        storage
            .materialize_external_lexical_candidates(&query, wrong_contract)
            .expect_err("mixed contract")
            .code,
        ErrorCode::ScoreContractViolation
    );
    if let StorageQuery::LexicalCandidates { limit, .. } = &mut query {
        *limit = 0;
    }
    assert!(
        storage
            .materialize_external_lexical_candidates(&query, candidates)
            .expect("zero limit")
            .candidates
            .is_empty()
    );
}

/// Candidate channels must enforce the requested vector space before ranking and truncation.
/// 候选通道必须在排序及截断前执行请求的向量空间约束。
#[test]
fn candidate_channels_filter_vector_space_before_limit() {
    let mut storage = InMemoryStorage::new();
    let mut wrong = memory("a-wrong-space", "subject", "scope", "shared content");
    wrong.vector_space_id = Some(VectorSpaceId::new("vector-other").expect("space"));
    let correct = memory("b-correct-space", "subject", "scope", "shared content");
    assert!(matches!(
        storage
            .mutate(&batch(
                "vector-space-seed",
                vec![put_memory(wrong, 0), put_memory(correct, 0)]
            ))
            .expect("seed"),
        MutationOutcome::Committed { .. }
    ));
    let access = ScopeAccess {
        write_scope: ScopeId::new("scope").expect("scope"),
        visible_scopes: vec![ScopeId::new("scope").expect("scope")],
    };
    let boundary = CandidateQueryBoundary {
        candidate_scope: None,
        session_recall_boundary: None,
    };
    for query in [
        StorageQuery::VectorCandidates {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_access: access.clone(),
            queries: vec!["shared".to_owned()],
            query_embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector-1").expect("space"),
            boundary: boundary.clone(),
            limit: 1,
            expected_contract: ScoreContractRef {
                id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                version: 1,
            },
            now_ns: 0,
        },
        StorageQuery::LexicalCandidates {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_access: access,
            queries: vec!["shared".to_owned()],
            vector_space_id: VectorSpaceId::new("vector-1").expect("space"),
            boundary,
            limit: 1,
            expected_contract: ScoreContractRef {
                id: vulcan_memorymesh_core_storage::REFERENCE_LEXICAL_CONTRACT_ID.to_owned(),
                version: 1,
            },
            now_ns: 0,
        },
    ] {
        let result = storage.query(&query).expect("query");
        let (StorageQueryResult::VectorCandidates(candidates)
        | StorageQueryResult::LexicalCandidates(candidates)) = result
        else {
            unreachable!("candidate query");
        };
        assert_eq!(candidates.candidates.len(), 1);
        assert_eq!(candidates.memories.len(), 1);
        assert_eq!(candidates.candidates[0].memory_id, "b-correct-space");
        assert_eq!(candidates.memories[0].id.as_str(), "b-correct-space");
    }
}

/// Applies the same frozen business time to split vector and lexical lifecycle filtering.
/// 对分离向量与词法通道使用同一冻结业务时刻执行生命周期过滤。
#[test]
fn split_candidate_channels_share_frozen_lifecycle_time() {
    let mut storage = InMemoryStorage::new();
    let mut expired = memory("a-expired", "subject", "scope", "shared");
    expired.expires_at_ns = Some(10);
    let mut live = memory("b-live", "subject", "scope", "shared");
    live.expires_at_ns = Some(11);
    assert!(matches!(
        storage
            .mutate(&batch(
                "frozen-lifecycle",
                vec![put_memory(expired, 0), put_memory(live, 0)]
            ))
            .expect("seed"),
        MutationOutcome::Committed { .. }
    ));
    let access = ScopeAccess {
        write_scope: ScopeId::new("scope").expect("scope"),
        visible_scopes: vec![ScopeId::new("scope").expect("scope")],
    };
    let boundary = CandidateQueryBoundary {
        candidate_scope: None,
        session_recall_boundary: None,
    };
    for query in [
        StorageQuery::VectorCandidates {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_access: access.clone(),
            queries: vec!["shared".to_owned()],
            query_embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector-1").expect("space"),
            boundary: boundary.clone(),
            limit: 2,
            expected_contract: ScoreContractRef {
                id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                version: 1,
            },
            now_ns: 10,
        },
        StorageQuery::LexicalCandidates {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_access: access,
            queries: vec!["shared".to_owned()],
            vector_space_id: VectorSpaceId::new("vector-1").expect("space"),
            boundary,
            limit: 2,
            expected_contract: ScoreContractRef {
                id: vulcan_memorymesh_core_storage::REFERENCE_LEXICAL_CONTRACT_ID.to_owned(),
                version: 1,
            },
            now_ns: 10,
        },
    ] {
        let result = storage.query(&query).expect("query");
        let (StorageQueryResult::VectorCandidates(channel)
        | StorageQueryResult::LexicalCandidates(channel)) = result
        else {
            unreachable!("candidate channel")
        };
        assert_eq!(channel.candidates.len(), 1);
        assert_eq!(channel.memories.len(), 1);
        assert_eq!(channel.candidates[0].memory_id, "b-live");
        assert_eq!(channel.memories[0].id.as_str(), "b-live");
    }
}

/// Filters scope and origin session before either physical channel can truncate away the sole eligible record.
/// 在两个物理通道可能截断唯一合格记录前，先过滤作用域及来源会话。
#[test]
fn replacement_channels_filter_scope_and_session_before_limit() {
    let mut storage = InMemoryStorage::new();
    let session = SessionSourceId::new("selected-session").expect("session");
    let mut outside = memory("a-outside", "subject", "primary", "shared");
    outside.origin_session_source_id = Some(session.clone());
    let mut other = memory("b-other-session", "subject", "candidate", "shared");
    other.origin_session_source_id = Some(SessionSourceId::new("other-session").expect("session"));
    let absent = memory("c-no-session", "subject", "candidate", "shared");
    let mut valid = memory("z-selected", "subject", "candidate", "shared");
    valid.origin_session_source_id = Some(session.clone());
    assert!(matches!(
        storage
            .mutate(&batch(
                "scope-seed",
                vec![
                    put_memory(outside, 0),
                    put_memory(other, 0),
                    put_memory(absent, 0),
                    put_memory(valid, 0)
                ]
            ))
            .expect("seed"),
        MutationOutcome::Committed { .. }
    ));
    let access = ScopeAccess {
        write_scope: ScopeId::new("primary").expect("scope"),
        visible_scopes: vec![
            ScopeId::new("primary").expect("scope"),
            ScopeId::new("candidate").expect("scope"),
        ],
    };
    let boundary = CandidateQueryBoundary {
        session_recall_boundary: None,
        candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
            scope_ids: vec![ScopeId::new("candidate").expect("scope")],
            origin_session_source_id: Some(session),
        }),
    };
    for query in [
        StorageQuery::VectorCandidates {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_access: access.clone(),
            queries: vec!["shared".to_owned()],
            query_embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector-1").expect("vector"),
            boundary: boundary.clone(),
            limit: 1,
            expected_contract: ScoreContractRef {
                id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                version: 1,
            },
            now_ns: 0,
        },
        StorageQuery::LexicalCandidates {
            subject_id: SubjectId::new("subject").expect("subject"),
            scope_access: access,
            queries: vec!["shared".to_owned()],
            vector_space_id: VectorSpaceId::new("vector-1").expect("vector"),
            boundary,
            limit: 1,
            expected_contract: ScoreContractRef {
                id: vulcan_memorymesh_core_storage::REFERENCE_LEXICAL_CONTRACT_ID.to_owned(),
                version: 1,
            },
            now_ns: 0,
        },
    ] {
        let result = storage.query(&query).expect("query");
        let (StorageQueryResult::VectorCandidates(rows)
        | StorageQueryResult::LexicalCandidates(rows)) = result
        else {
            unreachable!("candidate channel")
        };
        assert_eq!(rows.candidates.len(), 1);
        assert_eq!(rows.memories.len(), 1);
        assert_eq!(rows.candidates[0].memory_id, "z-selected");
        assert_eq!(rows.memories[0].id.as_str(), "z-selected");
    }
}

/// Explicit target scope enables cross-scope replacement while preserving subject and version checks.
/// 显式目标作用域允许跨范围替代，同时保留主体及版本检查。
#[test]
fn supersede_binds_actual_target_scope() {
    let mut storage = InMemoryStorage::new();
    assert!(matches!(
        storage
            .mutate(&batch(
                "supersede-seed",
                vec![
                    put_memory(memory("old", "subject", "old-scope", "old"), 0),
                    put_memory(memory("new", "subject", "new-scope", "new"), 0),
                    put_memory(memory("foreign", "other", "new-scope", "foreign"), 0)
                ]
            ))
            .expect("seed"),
        MutationOutcome::Committed { .. }
    ));
    for (index, scope, replacement) in [
        (0, "wrong-scope", "new"),
        (1, "old-scope", "foreign"),
        (2, "old-scope", "new"),
    ] {
        let result = storage
            .mutate(&batch(
                &format!("supersede-{index}"),
                vec![Mutation::SupersedeMemory {
                    actor_subject_id: SubjectId::new("subject").expect("subject"),
                    scope_id: ScopeId::new(scope).expect("scope"),
                    expected_access: vulcan_memorymesh_core_contracts::MemoryAccessIdentity {
                        access_class:
                            vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate,
                        source_subject_id: Some(SubjectId::new("subject").expect("subject")),
                    },
                    memory_id: MemoryId::new("old").expect("memory"),
                    expected_version: 1,
                    superseded_by: EntityRef::Existing {
                        domain_id: replacement.to_owned(),
                        expected_version: 1,
                    },
                    status_reason: Some("reviewed".to_owned()),
                }],
            ))
            .expect("mutation outcome");
        assert_eq!(
            matches!(result, MutationOutcome::Committed { .. }),
            index == 2
        );
    }
}

/// Finite positive rescaling must preserve the reference vector producer's cosine result.
/// 有限正数缩放必须保留参考向量生产者的余弦结果。
#[test]
fn reference_cosine_preserves_finite_vector_scale() {
    for magnitude in [1.0_f32, 1.0e-30, 1.0e30] {
        let mut storage = InMemoryStorage::new();
        let mut record = memory("scaled", "subject", "scope", "scaled");
        record.embedding = Some(vec![magnitude, magnitude]);
        assert!(matches!(
            storage
                .mutate(&batch("scale", vec![put_memory(record, 0)]))
                .expect("seed"),
            MutationOutcome::Committed { .. }
        ));
        let result = storage
            .query(&StorageQuery::VectorCandidates {
                subject_id: SubjectId::new("subject").expect("subject"),
                scope_access: ScopeAccess {
                    write_scope: ScopeId::new("scope").expect("scope"),
                    visible_scopes: vec![ScopeId::new("scope").expect("scope")],
                },
                queries: vec!["scaled".to_owned()],
                query_embeddings: vec![vec![magnitude, magnitude]],
                vector_space_id: VectorSpaceId::new("vector-1").expect("space"),
                boundary: CandidateQueryBoundary {
                    candidate_scope: None,
                    session_recall_boundary: None,
                },
                limit: 1,
                expected_contract: ScoreContractRef {
                    id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                    version: 1,
                },
                now_ns: 0,
            })
            .expect("query");
        let StorageQueryResult::VectorCandidates(rows) = result else {
            unreachable!("vectors")
        };
        assert_eq!(rows.candidates.len(), 1, "magnitude={magnitude}");
        assert_eq!(rows.memories.len(), 1, "magnitude={magnitude}");
        assert!((0.999..=1.0).contains(&rows.candidates[0].score.raw_value));
    }
}

/// Distinct typed cache identities remain distinct even when their strings contain separators.
/// 即便字符串包含分隔符，不同强类型缓存身份仍必须互相隔离。
#[test]
fn noise_cache_uses_unambiguous_identity_components() {
    use vulcan_memorymesh_core_contracts::{NoisePrototype, NoisePrototypeCacheIdentity};
    let mut storage = InMemoryStorage::new();
    let first = NoisePrototypeCacheIdentity {
        scope_id: ScopeId::new("scope|common").expect("scope"),
        language: "zh-CN".to_owned(),
        model: "model".to_owned(),
        dimension: 2,
        rules_hash: "a".repeat(64),
    };
    let second = NoisePrototypeCacheIdentity {
        scope_id: ScopeId::new("scope").expect("scope"),
        language: "common".to_owned(),
        model: "zh-CN|model".to_owned(),
        dimension: 2,
        rules_hash: "a".repeat(64),
    };
    let rows = vec![NoisePrototype {
        category: "first-category".to_owned(),
        phrase: "first-phrase".to_owned(),
        vector: vec![1.0, 0.0],
    }];
    assert!(matches!(
        storage
            .mutate(&batch(
                "cache-first",
                vec![Mutation::PutNoisePrototypeBundle {
                    identity: first.clone(),
                    prototypes: rows.clone()
                }]
            ))
            .expect("cache"),
        MutationOutcome::Committed { .. }
    ));
    let StorageQueryResult::NoisePrototypes { prototypes, .. } = storage
        .query(&StorageQuery::NoisePrototypeBundle { identity: second })
        .expect("other identity")
    else {
        unreachable!("cache query")
    };
    assert!(
        prototypes.is_empty(),
        "foreign cache rows leaked through a delimiter collision"
    );
    let StorageQueryResult::NoisePrototypes { prototypes, .. } = storage
        .query(&StorageQuery::NoisePrototypeBundle { identity: first })
        .expect("own identity")
    else {
        unreachable!("cache query")
    };
    assert_eq!(prototypes, rows);
}

/// Builds one turn-derived memory whose evidence points to the provided persisted turn.
/// 构建一条证据指向给定持久轮次的轮次派生记忆。
fn turn_memory(identity: &str, content: &str, source_turn: &Turn) -> MemoryRecord {
    let mut record = memory(identity, "subject-compact", "scope-compact", content);
    record.source_kind = MemorySourceKind::Turn;
    record.source_turn_id = Some(source_turn.id.clone());
    record.evidence = vec![EvidenceRef {
        turn_id: source_turn.id.clone(),
        excerpt: format!("evidence from {}", source_turn.id),
        occurred_at_ns: source_turn.created_at_ns,
    }];
    record
}

/// Verifies composite evidence identities and final-version receipt coalescing.
/// 验证复合证据身份与收据最终版本折叠。
#[test]
fn context_evidence_receipt_reports_distinct_composite_final_versions() -> Result<(), String> {
    let mut storage = InMemoryStorage::new();
    let record = memory("memory-evidence", "subject-a", "scope-a", "evidence owner");
    let created = storage
        .mutate(&batch("evidence-owner", vec![put_memory(record, 0)]))
        .expect("memory seed must commit");
    assert!(matches!(created, MutationOutcome::Committed { .. }));

    let memory_ref = || EntityRef::Existing {
        domain_id: "memory-evidence".to_owned(),
        expected_version: 1,
    };
    let outcome = storage
        .mutate(&batch(
            "evidence-aggregate",
            vec![
                Mutation::AggregateContextEvidence {
                    memory: memory_ref(),
                    context_key: "topic".to_owned(),
                    context_value: "rust:go".to_owned(),
                    relation: ContextEvidenceRelation::Support,
                    occurred_at_ns: 10,
                },
                Mutation::AggregateContextEvidence {
                    memory: memory_ref(),
                    context_key: "topic".to_owned(),
                    context_value: "rust:go".to_owned(),
                    relation: ContextEvidenceRelation::Rebuttal,
                    occurred_at_ns: 11,
                },
                Mutation::AggregateContextEvidence {
                    memory: memory_ref(),
                    context_key: "topic:rust".to_owned(),
                    context_value: "go".to_owned(),
                    relation: ContextEvidenceRelation::Support,
                    occurred_at_ns: 12,
                },
            ],
        ))
        .expect("evidence aggregation must commit");
    let receipt = match outcome {
        MutationOutcome::Committed { receipt, .. } => receipt,
        other => return Err(format!("evidence aggregation did not commit: {other:?}")),
    };
    assert_eq!(receipt.committed_versions.len(), 2);
    let owner = MemoryId::new("memory-evidence").expect("memory identity must be valid");
    let first_id = context_evidence_domain_id(&owner, "topic", "rust:go");
    let second_id = context_evidence_domain_id(&owner, "topic:rust", "go");
    assert_ne!(first_id, second_id);
    assert!(receipt.committed_versions.iter().any(|versioned| {
        versioned.entity_kind == EntityKind::ContextEvidence
            && versioned.domain_id == first_id
            && versioned.version == 2
    }));
    assert!(receipt.committed_versions.iter().any(|versioned| {
        versioned.entity_kind == EntityKind::ContextEvidence
            && versioned.domain_id == second_id
            && versioned.version == 1
    }));
    Ok(())
}

/// Seeds current-session and other-session evidence before the first compact boundary.
/// 在首次压缩边界前播种当前会话与其他会话证据。
fn seed_compact_recall_storage() -> (InMemoryStorage, ScopeAccess) {
    let mut storage = InMemoryStorage::new();
    let first_turn = turn("turn-current-1", 100);
    let mut other_turn = turn("turn-other-1", 100);
    other_turn.session_source_id =
        SessionSourceId::new("session-other").expect("other session identity must be valid");
    let first_memory = turn_memory("memory-current-1", "current turn memory", &first_turn);
    let other_memory = turn_memory("memory-other-1", "other turn memory", &other_turn);
    let seeded = storage
        .mutate(&batch(
            "compact-recall-seed",
            vec![
                Mutation::PutTurn {
                    entity: EntityRef::Existing {
                        domain_id: first_turn.id.as_str().to_owned(),
                        expected_version: 0,
                    },
                    turn: first_turn,
                },
                Mutation::PutTurn {
                    entity: EntityRef::Existing {
                        domain_id: other_turn.id.as_str().to_owned(),
                        expected_version: 0,
                    },
                    turn: other_turn,
                },
                put_memory(first_memory, 0),
                put_memory(other_memory, 0),
            ],
        ))
        .expect("boundary seed must be valid");
    assert!(matches!(seeded, MutationOutcome::Committed { .. }));
    let access = ScopeAccess {
        write_scope: ScopeId::new("scope-compact").expect("scope identity must be valid"),
        visible_scopes: vec![ScopeId::new("scope-compact").expect("scope identity must be valid")],
    };
    (storage, access)
}

/// Advances the compact boundary and persists one later current-session memory.
/// 推进压缩边界并持久化一条更晚的当前会话记忆。
fn compact_and_append_later_memory(storage: &mut InMemoryStorage) {
    let compacted = storage
        .mutate(&batch(
            "compact-recall-boundary",
            vec![Mutation::AdvanceChatCompactBoundary {
                subject_id: SubjectId::new("subject-compact")
                    .expect("subject identity must be valid"),
                scope_id: ScopeId::new("scope-compact").expect("scope identity must be valid"),
                session_source_id: SessionSourceId::new("session-compact")
                    .expect("session identity must be valid"),
                compacted_at_ns: 150,
            }],
        ))
        .expect("compact boundary mutation must succeed");
    assert!(matches!(compacted, MutationOutcome::Committed { .. }));
    let second_turn = turn("turn-current-2", 200);
    let second_memory = turn_memory("memory-current-2", "later turn memory", &second_turn);
    let appended = storage
        .mutate(&batch(
            "compact-recall-later",
            vec![
                Mutation::PutTurn {
                    entity: EntityRef::Existing {
                        domain_id: second_turn.id.as_str().to_owned(),
                        expected_version: 0,
                    },
                    turn: second_turn,
                },
                put_memory(second_memory, 0),
            ],
        ))
        .expect("post-boundary seed must succeed");
    assert!(matches!(appended, MutationOutcome::Committed { .. }));
}

/// Runs one exact-session candidate search and returns stable memory identity strings.
/// 执行一次精确会话候选搜索并返回稳定记忆身份字符串。
fn session_search_ids(
    storage: &InMemoryStorage,
    scope_access: ScopeAccess,
    mode: SessionRecallMode,
) -> Vec<String> {
    let result = storage
        .query(&StorageQuery::VectorCandidates {
            subject_id: SubjectId::new("subject-compact").expect("subject identity must be valid"),
            scope_access,
            queries: vec!["turn memory".to_owned()],
            query_embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector-1").expect("vector identity must be valid"),
            boundary: CandidateQueryBoundary {
                candidate_scope: None,
                session_recall_boundary: Some(SessionRecallBoundary {
                    session_source_id: SessionSourceId::new("session-compact")
                        .expect("session identity must be valid"),
                    mode,
                }),
            },
            limit: 10,
            expected_contract: ScoreContractRef {
                id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                version: REFERENCE_CONTRACT_VERSION,
            },
            now_ns: 0,
        })
        .expect("session recall search must succeed");
    assert!(
        matches!(&result, StorageQueryResult::VectorCandidates(_)),
        "session recall search must return candidates"
    );
    if let StorageQueryResult::VectorCandidates(candidates) = result {
        candidates
            .candidates
            .into_iter()
            .map(|item| item.memory_id)
            .collect()
    } else {
        Vec::new()
    }
}

/// Verifies compact-aware recall excludes un-compacted current-session evidence without hiding other sessions.
/// 验证压缩感知召回会排除当前会话未压缩证据，同时不会隐藏其他会话。
#[test]
fn compact_aware_recall_enforces_exact_session_boundary() {
    let (mut storage, access) = seed_compact_recall_storage();
    assert_eq!(
        session_search_ids(&storage, access.clone(), SessionRecallMode::CompactAware),
        vec!["memory-other-1"]
    );
    compact_and_append_later_memory(&mut storage);
    let compact_ids = session_search_ids(&storage, access.clone(), SessionRecallMode::CompactAware);
    assert!(
        compact_ids
            .iter()
            .any(|identity| identity == "memory-current-1")
    );
    assert!(
        compact_ids
            .iter()
            .any(|identity| identity == "memory-other-1")
    );
    assert!(
        !compact_ids
            .iter()
            .any(|identity| identity == "memory-current-2")
    );
    assert_eq!(
        session_search_ids(&storage, access, SessionRecallMode::Full).len(),
        3
    );
}

/// Verifies a conflict in any item rejects the entire detached transaction.
/// 验证任一项目冲突都会拒绝整个独立事务。
#[test]
fn mutation_batch_is_all_or_nothing() {
    // Create an empty reference backend.
    // 创建空参考后端。
    let mut storage = InMemoryStorage::new();
    // Build one valid create followed by an invalid expected-version update.
    // 构建一个有效创建以及其后的无效期望版本更新。
    let mutation_batch = batch(
        "atomic",
        vec![
            put_memory(memory("memory-1", "subject-1", "scope-1", "first"), 0),
            put_memory(memory("memory-2", "subject-1", "scope-1", "second"), 7),
        ],
    );
    // Apply the detached transaction.
    // 应用独立事务。
    let outcome = storage
        .mutate(&mutation_batch)
        .expect("well-shaped batch must return an outcome");
    assert!(matches!(outcome, MutationOutcome::Rejected { .. }));
    assert!(storage.dataset().memories.is_empty());
    assert_eq!(storage.storage_version(), 0);
}

/// Verifies idempotent replay returns the recorded outcome without another version increment.
/// 验证幂等重放返回已记录结果且不会再次增加版本。
#[test]
fn committed_batch_replay_is_idempotent() {
    // Create an empty reference backend.
    // 创建空参考后端。
    let mut storage = InMemoryStorage::new();
    // Build one valid memory creation batch.
    // 构建一个有效记忆创建批次。
    let mutation_batch = batch(
        "replay",
        vec![put_memory(
            memory("memory-1", "subject-1", "scope-1", "remember rust"),
            0,
        )],
    );
    // Commit the batch once.
    // 提交批次一次。
    let first = storage
        .mutate(&mutation_batch)
        .expect("first mutation must return an outcome");
    // Replay the exact same idempotency key.
    // 重放完全相同的幂等键。
    let second = storage
        .mutate(&mutation_batch)
        .expect("replay must return recorded outcome");
    assert_eq!(first, second);
    assert_eq!(storage.storage_version(), 1);
    assert_eq!(storage.dataset().memories.len(), 1);
    assert_eq!(
        storage.reconcile(&mutation_batch.identity.idempotency_key),
        Some(first)
    );

    // Reusing the key under a different operation is not the same durable retry.
    // 在不同操作下复用该键并不是同一次持久重试。
    let mut conflicting_operation = mutation_batch.clone();
    conflicting_operation.identity.operation_id =
        OperationId::new("op-replay-conflict").expect("operation identity must be valid");
    let conflict = storage
        .mutate(&conflicting_operation)
        .expect_err("same key under a different operation must conflict");
    assert_eq!(conflict.code, ErrorCode::IdempotencyConflict);
}

/// Verifies compact-boundary advancement atomically selects the latest turn and is idempotent.
/// 验证压缩边界推进会原子选择最新轮次并保持幂等。
#[test]
fn chat_compact_boundary_is_atomic_latest_and_idempotent() {
    // Resolve all exact ownership keys once so every assertion addresses one session partition.
    // 一次性解析全部精确所有权键，确保每项断言都针对同一会话分区。
    let subject_id = SubjectId::new("subject-compact").expect("subject identity must be valid");
    let scope_id = ScopeId::new("scope-compact").expect("scope identity must be valid");
    let session_source_id =
        SessionSourceId::new("session-compact").expect("session identity must be valid");
    let advance = |compacted_at_ns| Mutation::AdvanceChatCompactBoundary {
        subject_id: subject_id.clone(),
        scope_id: scope_id.clone(),
        session_source_id: session_source_id.clone(),
        compacted_at_ns,
    };
    let put_turn = |identity: &str, created_at_ns: i64| {
        let value = turn(identity, created_at_ns);
        Mutation::PutTurn {
            entity: EntityRef::Existing {
                domain_id: value.id.as_str().to_owned(),
                expected_version: 0,
            },
            turn: value,
        }
    };
    let mut storage = InMemoryStorage::new();

    // Advancing an empty session is accepted without fabricating a turn or changing storage.
    // 推进空会话时应接受请求，但不得伪造轮次或更改存储。
    let empty_outcome = storage
        .mutate(&batch("compact-empty", vec![advance(10)]))
        .expect("empty compact mutation must return an outcome");
    assert!(matches!(
        empty_outcome,
        MutationOutcome::Committed {
            storage_version: 0,
            chat_compact: Some(ref compact),
            ..
        } if compact.accepted && !compact.updated && compact.compacted_turn_id.is_none()
    ));

    // Persist the first turn and advance the boundary to that exact identity.
    // 持久化第一个轮次，并将边界推进至该精确标识。
    storage
        .mutate(&batch("compact-turn-one", vec![put_turn("turn-1", 1)]))
        .expect("first turn mutation must return an outcome");
    let first_batch = batch("compact-first", vec![advance(11)]);
    let first_outcome = storage
        .mutate(&first_batch)
        .expect("first compact mutation must return an outcome");
    assert!(matches!(
        first_outcome,
        MutationOutcome::Committed {
            storage_version: 2,
            chat_compact: Some(ref compact),
            ..
        } if compact.accepted
            && compact.updated
            && compact.compacted_turn_id.as_ref().is_some_and(|id| id.as_str() == "turn-1")
    ));

    // Replaying the exact idempotency key returns the recorded result without another commit.
    // 重放完全相同的幂等键时应返回已记录结果，不得再次提交。
    let replay_outcome = storage
        .mutate(&first_batch)
        .expect("compact replay must return its recorded outcome");
    assert_eq!(replay_outcome, first_outcome);
    assert_eq!(storage.storage_version(), 2);

    // A new request against the same latest turn reports no update and preserves the version.
    // 针对同一最新轮次的新请求应报告未更新，并保持存储版本不变。
    let repeated_outcome = storage
        .mutate(&batch("compact-same-latest", vec![advance(12)]))
        .expect("repeated compact mutation must return an outcome");
    assert!(matches!(
        repeated_outcome,
        MutationOutcome::Committed {
            storage_version: 2,
            chat_compact: Some(ref compact),
            ..
        } if compact.accepted
            && !compact.updated
            && compact.compacted_turn_id.as_ref().is_some_and(|id| id.as_str() == "turn-1")
    ));

    // A later persisted turn becomes the next atomic boundary target.
    // 后续持久化轮次会成为下一次原子边界推进目标。
    storage
        .mutate(&batch("compact-turn-two", vec![put_turn("turn-2", 2)]))
        .expect("second turn mutation must return an outcome");
    let latest_outcome = storage
        .mutate(&batch("compact-latest", vec![advance(13)]))
        .expect("latest compact mutation must return an outcome");
    assert!(matches!(
        latest_outcome,
        MutationOutcome::Committed {
            storage_version: 4,
            chat_compact: Some(ref compact),
            ..
        } if compact.accepted
            && compact.updated
            && compact.compacted_turn_id.as_ref().is_some_and(|id| id.as_str() == "turn-2")
    ));
    let dataset = storage.dataset();
    assert_eq!(dataset.chat_compact_boundaries.len(), 1);
    assert_eq!(
        dataset.chat_compact_boundaries[0]
            .compacted_turn_id
            .as_str(),
        "turn-2"
    );
}

/// Verifies subject and explicit visible scopes gate search and details.
/// 验证主体与显式可见作用域限制搜索和详情访问。
#[test]
fn reads_never_cross_subject_or_scope_boundaries() {
    // Create a backend and seed two ownership partitions atomically.
    // 创建后端并原子写入两个所有权分区。
    let mut storage = InMemoryStorage::new();
    storage
        .mutate(&batch(
            "scope-seed",
            vec![
                put_memory(
                    memory("memory-a", "subject-a", "scope-a", "rust preference"),
                    0,
                ),
                put_memory(memory("memory-b", "subject-b", "scope-b", "rust secret"), 0),
            ],
        ))
        .expect("seed mutation must return an outcome");
    // Define the only authorized subject and scope.
    // 定义唯一授权主体与作用域。
    let access = ScopeAccess {
        write_scope: ScopeId::new("scope-a").expect("scope identity must be valid"),
        visible_scopes: vec![ScopeId::new("scope-a").expect("scope identity must be valid")],
    };
    // Search with an exact compatible vector.
    // 使用精确兼容向量搜索。
    let result = storage
        .query(&StorageQuery::VectorCandidates {
            subject_id: SubjectId::new("subject-a").expect("subject identity must be valid"),
            scope_access: access.clone(),
            queries: vec!["rust".to_owned()],
            query_embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector-1").expect("vector identity must be valid"),
            boundary: CandidateQueryBoundary {
                candidate_scope: None,
                session_recall_boundary: None,
            },
            limit: 10,
            expected_contract: ScoreContractRef {
                id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                version: REFERENCE_CONTRACT_VERSION,
            },
            now_ns: 0,
        })
        .expect("authorized search must succeed");
    // Extract the exact search result variant.
    // 提取精确搜索结果变体。
    assert!(matches!(result, StorageQueryResult::VectorCandidates(_)));
    if let StorageQueryResult::VectorCandidates(candidates) = result {
        assert_eq!(candidates.candidates.len(), 1);
        assert_eq!(candidates.memories.len(), 1);
        assert_eq!(candidates.candidates[0].memory_id, "memory-a");
        assert_eq!(candidates.memories[0].id.as_str(), "memory-a");
    }
    // Attempt to read another subject's known identity through the authorized scope.
    // 尝试通过授权作用域读取另一主体的已知标识。
    let detail = storage
        .query(&StorageQuery::MemoryDetails {
            subject_id: SubjectId::new("subject-a").expect("subject identity must be valid"),
            scope_access: access,
            memory_id: MemoryId::new("memory-b").expect("memory identity must be valid"),
        })
        .expect("scoped details query must succeed");
    assert!(matches!(detail, StorageQueryResult::MemoryDetails(value) if value.is_none()));
}

/// Builds one shared-memory fixture with either a known or explicitly unknown source subject.
/// 构建一条来源主体已知或显式未知的共享记忆夹具。
fn shared_memory(
    identity: &str,
    source_subject: Option<&str>,
    scope: &str,
    content: &str,
) -> MemoryRecord {
    let mut record = memory(
        identity,
        source_subject.unwrap_or("unknown-shared-source"),
        scope,
        content,
    );
    record.access_class = MemoryAccessClass::ScopeShared;
    record.source_subject_id =
        source_subject.map(|source| SubjectId::new(source).expect("source identity must be valid"));
    record
}

/// Verifies authorized shared memories remain readable without weakening private or scope isolation.
/// 验证已授权共享记忆保持可读，同时不削弱私有主体或作用域隔离。
#[test]
fn shared_reads_use_scope_authority_without_weakening_private_isolation() {
    let mut storage = InMemoryStorage::new();
    let shared_unknown = shared_memory("shared-unknown", None, "scope-a", "shared unknown");
    let shared_known = shared_memory("shared-known", Some("creator"), "scope-a", "shared known");
    let foreign_private = memory("private-foreign", "creator", "scope-a", "private secret");
    let hidden_shared = shared_memory("shared-hidden", Some("creator"), "scope-b", "hidden shared");
    storage
        .mutate(&batch(
            "shared-seed",
            vec![
                put_memory(shared_unknown.clone(), 0),
                put_memory(shared_known.clone(), 0),
                put_memory(foreign_private, 0),
                put_memory(hidden_shared, 0),
            ],
        ))
        .expect("shared seed must succeed");

    let actor = SubjectId::new("reader").expect("actor identity must be valid");
    let scope = ScopeId::new("scope-a").expect("scope identity must be valid");
    let access = ScopeAccess {
        write_scope: scope.clone(),
        visible_scopes: vec![scope],
    };
    let result = storage
        .query(&StorageQuery::VectorCandidates {
            subject_id: actor.clone(),
            scope_access: access.clone(),
            queries: vec!["shared".to_owned()],
            query_embeddings: vec![vec![1.0, 0.0]],
            vector_space_id: VectorSpaceId::new("vector-1").expect("vector identity must be valid"),
            boundary: CandidateQueryBoundary {
                candidate_scope: None,
                session_recall_boundary: None,
            },
            limit: 10,
            expected_contract: ScoreContractRef {
                id: REFERENCE_VECTOR_CONTRACT_ID.to_owned(),
                version: REFERENCE_CONTRACT_VERSION,
            },
            now_ns: 1,
        })
        .expect("authorized shared search must succeed");
    assert!(matches!(result, StorageQueryResult::VectorCandidates(_)));
    let StorageQueryResult::VectorCandidates(result) = result else {
        return;
    };
    let visible_ids = result
        .memories
        .iter()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(visible_ids, vec!["shared-known", "shared-unknown"]);

    for record in [&shared_unknown, &shared_known] {
        let details = storage
            .query(&StorageQuery::MemoryDetails {
                subject_id: actor.clone(),
                scope_access: access.clone(),
                memory_id: record.id.clone(),
            })
            .expect("shared details must succeed");
        assert!(
            matches!(details, StorageQueryResult::MemoryDetails(value) if value.as_ref().as_ref().is_some_and(|value| value.id == record.id))
        );
    }
    let private_details = storage
        .query(&StorageQuery::MemoryDetails {
            subject_id: actor,
            scope_access: access,
            memory_id: MemoryId::new("private-foreign").expect("memory identity must be valid"),
        })
        .expect("private details query must succeed");
    assert!(matches!(private_details, StorageQueryResult::MemoryDetails(value) if value.is_none()));
}

/// Verifies a readable shared record cannot impersonate the current subject's direct-write idempotency fact.
/// 验证可读共享记录不能冒充当前主体的主动写入幂等事实。
#[test]
fn shared_memory_never_matches_private_direct_write_dedupe() {
    let mut storage = InMemoryStorage::new();
    let mut shared = shared_memory("shared-direct", None, "scope-a", "shared direct");
    shared.source_kind = MemorySourceKind::Direct;
    shared.origin_session_source_id = Some(SessionSourceId::new("session-a").expect("session"));
    shared.dedupe_hash = Some("v1:shared-direct".to_owned());
    shared.created_at_ns = 10;
    shared.updated_at_ns = 10;
    storage
        .mutate(&batch("shared-direct-seed", vec![put_memory(shared, 0)]))
        .expect("shared direct seed must succeed");
    let scope = ScopeId::new("scope-a").expect("scope identity must be valid");
    let result = storage
        .query(&StorageQuery::RecentDirectWriteDedupe {
            subject_id: SubjectId::new("reader").expect("actor identity must be valid"),
            scope_access: ScopeAccess {
                write_scope: scope.clone(),
                visible_scopes: vec![scope.clone()],
            },
            session_source_id: SessionSourceId::new("session-a")
                .expect("session identity must be valid"),
            lookups: vec![DirectWriteDedupeLookup {
                target_scope_id: scope,
                scope_class: vulcan_memorymesh_core_contracts::MemoryScopeClass::Environment,
                dedupe_hash: "v1:shared-direct".to_owned(),
            }],
            created_not_before_ns: 0,
            evaluated_at_ns: 20,
        })
        .expect("direct-write dedupe query must succeed");
    assert!(matches!(
        result,
        StorageQueryResult::RecentDirectWriteDedupe(records)
            if records.len() == 1 && records[0].is_none()
    ));
}

/// Verifies shared mutations require the exact access identity captured by the authorized read.
/// 验证共享记忆变更必须携带授权读取时捕获的精确访问身份。
#[test]
fn shared_mutations_preserve_the_observed_access_identity() {
    let mut storage = InMemoryStorage::new();
    let shared_unknown = shared_memory("shared-unknown", None, "scope-a", "shared unknown");
    let replacement = memory(
        "private-replacement",
        "reader",
        "scope-a",
        "private replacement",
    );
    storage
        .mutate(&batch(
            "shared-mutation-seed",
            vec![
                put_memory(shared_unknown.clone(), 0),
                put_memory(replacement.clone(), 0),
            ],
        ))
        .expect("shared mutation seed must succeed");
    let actor = SubjectId::new("reader").expect("actor identity must be valid");
    let scope = ScopeId::new("scope-a").expect("scope identity must be valid");
    let reinforce = Mutation::ReinforceMemory {
        actor_subject_id: actor.clone(),
        scope_id: scope.clone(),
        expected_access: MemoryAccessIdentity::from(&shared_unknown),
        memory_id: shared_unknown.id.clone(),
        expected_version: 1,
        recalled_at_ns: 2,
        source_session_id: None,
    };
    assert!(matches!(
        storage
            .mutate(&batch("shared-reinforce", vec![reinforce]))
            .expect("shared reinforcement must return an outcome"),
        MutationOutcome::Committed { .. }
    ));
    let wrong_identity = Mutation::DeleteMemory {
        actor_subject_id: actor.clone(),
        scope_id: scope.clone(),
        expected_access: MemoryAccessIdentity {
            access_class: MemoryAccessClass::ScopeShared,
            source_subject_id: Some(SubjectId::new("other").expect("source identity")),
        },
        memory_id: shared_unknown.id.clone(),
        expected_version: Some(2),
    };
    assert!(matches!(
        storage
            .mutate(&batch("shared-wrong-identity", vec![wrong_identity]))
            .expect("identity conflict must return an outcome"),
        MutationOutcome::Rejected { .. }
    ));
    let supersede = Mutation::SupersedeMemory {
        actor_subject_id: actor.clone(),
        scope_id: scope.clone(),
        expected_access: MemoryAccessIdentity::from(&shared_unknown),
        memory_id: shared_unknown.id.clone(),
        expected_version: 2,
        superseded_by: EntityRef::Existing {
            domain_id: replacement.id.as_str().to_owned(),
            expected_version: 1,
        },
        status_reason: Some("shared replacement".to_owned()),
    };
    assert!(matches!(
        storage
            .mutate(&batch("shared-supersede", vec![supersede]))
            .expect("shared supersede must return an outcome"),
        MutationOutcome::Committed { .. }
    ));
    let delete = Mutation::DeleteMemory {
        actor_subject_id: actor,
        scope_id: scope,
        expected_access: MemoryAccessIdentity::from(&shared_unknown),
        memory_id: shared_unknown.id.clone(),
        expected_version: Some(3),
    };
    assert!(matches!(
        storage
            .mutate(&batch("shared-delete", vec![delete]))
            .expect("shared deletion must return an outcome"),
        MutationOutcome::Committed { .. }
    ));
    assert!(
        storage
            .dataset()
            .memories
            .iter()
            .all(|record| record.id != shared_unknown.id)
    );
}

/// Verifies invalid scope registration fails before storage access.
/// 验证无效作用域注册在存储访问前失败。
#[test]
fn invalid_scope_access_is_rejected() {
    // Create an empty reference backend.
    // 创建空参考后端。
    let storage = InMemoryStorage::new();
    // Build a scope access that omits its write scope.
    // 构建一个遗漏写入作用域的作用域访问。
    let access = ScopeAccess {
        write_scope: ScopeId::new("scope-a").expect("scope identity must be valid"),
        visible_scopes: vec![ScopeId::new("scope-b").expect("scope identity must be valid")],
    };
    // Execute a details query that must fail validation.
    // 执行一个必须校验失败的详情查询。
    let error = storage
        .query(&StorageQuery::MemoryDetails {
            subject_id: SubjectId::new("subject-a").expect("subject identity must be valid"),
            scope_access: access,
            memory_id: MemoryId::new("memory-a").expect("memory identity must be valid"),
        })
        .expect_err("invalid scope access must fail");
    assert_eq!(
        error.code,
        vulcan_memorymesh_core_contracts::ErrorCode::ScopeViolation
    );
}

/// Verifies exact-identity retirement preserves an auditable link to a same-batch replacement.
/// 验证精确身份退役保留指向同批次替代节点的可审计关联。
#[test]
#[allow(clippy::too_many_lines)]
fn retire_profile_node_by_exact_identity_binds_same_batch_replacement() {
    use vulcan_memorymesh_core_contracts::{
        EntityRef, EntitySlotId, Mutation, ProfileNode, ProfileNodeId, ProfileNodeState,
        ProfileSourceKind, ProfileViewSection, SectionRole,
    };
    let mut storage = InMemoryStorage::new();
    let subject_id = SubjectId::new("subject-profile").expect("subject identity must be valid");
    let scope_id = ScopeId::new("scope-profile").expect("scope identity must be valid");
    let scope_access = ScopeAccess {
        write_scope: scope_id.clone(),
        visible_scopes: vec![scope_id.clone()],
    };
    let slot = EntitySlotId::new("profile-slot").expect("slot identity must be valid");
    let node = ProfileNode {
        id: ProfileNodeId::new("profile-node-1").expect("profile node identity must be valid"),
        subject_id: Some(subject_id.clone()),
        scope_id: scope_id.clone(),
        // Keep one atomic statement while retaining the complete ownership fields.
        // 保留一条原子陈述，同时携带完整所有权字段。
        content: "language: zh-CN".to_owned(),
        confidence: Some(0.9),
        state: ProfileNodeState::Active,
        source_turn_ids: Vec::new(),
        source_kind: ProfileSourceKind::SystemSeed,
        source_id: None,
        evidence_source: None,
        priority: 60,
        profile_level: vulcan_memorymesh_core_contracts::ProfileLevel::Stable,
        level_reason: None,
        refresh_weight: 0,
        effective_date_ns: Some(1),
        expires_at_ns: None,
        status_reason: None,
        superseded_by: None,
        version: 0,
        created_at_ns: 1,
        updated_at_ns: 1,
    };
    let put_outcome = storage
        .mutate(&batch(
            "put-profile-by-key",
            vec![Mutation::PutProfileNode {
                entity: EntityRef::New(slot),
                node: node.clone(),
            }],
        ))
        .expect("profile node write must succeed");
    assert!(
        matches!(&put_outcome, MutationOutcome::Committed { .. }),
        "profile node write must commit: {put_outcome:?}"
    );

    let MutationOutcome::Committed { receipt, .. } = &put_outcome else {
        unreachable!("profile node write must commit")
    };
    let stored_id = ProfileNodeId::new(
        receipt
            .entity_bindings
            .first()
            .expect("profile binding must exist")
            .domain_id
            .clone(),
    )
    .expect("stored profile identity must be valid");
    // Create the replacement first and bind exact retirement to its same-batch slot.
    // 先创建替代节点，并把精确退役绑定到其同批次槽位。
    let replacement_slot =
        EntitySlotId::new("profile-replacement").expect("slot identity must be valid");
    let mut replacement = node;
    replacement.content = "language: en-US".to_owned();
    let retired = storage
        .mutate(&batch(
            "retire-profile-exact",
            vec![
                Mutation::PutProfileNode {
                    entity: EntityRef::New(replacement_slot.clone()),
                    node: replacement,
                },
                Mutation::RetireProfileNode {
                    subject_id: Some(subject_id.clone()),
                    scope_id: scope_id.clone(),
                    profile_node_id: stored_id.clone(),
                    expected_version: 1,
                    superseded_by: Some(EntityRef::New(replacement_slot)),
                    status_reason: Some("superseded-by-review".to_owned()),
                },
            ],
        ))
        .expect("exact retirement must succeed");
    assert!(
        matches!(retired, MutationOutcome::Committed { .. }),
        "retirement must commit"
    );
    // The node must now be inactive with the version and supersede trail intact.
    // 该节点现在必须处于非活跃状态，且替代轨迹完整。
    let nodes = storage
        .query(&StorageQuery::ProfileNodes {
            subject_id,
            scope_access,
            sections: vec![ProfileViewSection {
                scope_id,
                role: SectionRole::Subject,
                display_label: "Subject".to_owned(),
            }],
            evaluated_at_ns: 1,
        })
        .expect("profile query must succeed");
    let StorageQueryResult::ProfileNodes(nodes) = nodes else {
        unreachable!("profile query must return nodes");
    };
    assert_eq!(nodes.len(), 1, "only the replacement must remain active");
    let replacement_id = nodes[0].id.clone();
    let retired_node = storage
        .dataset()
        .profile_nodes
        .into_iter()
        .find(|candidate| candidate.id == stored_id)
        .expect("retired profile node must remain auditable");
    assert_eq!(retired_node.state, ProfileNodeState::Inactive);
    assert_eq!(retired_node.superseded_by, Some(replacement_id));
}

/// Verifies environment profile nodes are shared while subject nodes remain isolated.
/// 验证环境画像节点跨主体共享，同时主体画像节点保持严格隔离。
#[test]
#[allow(clippy::too_many_lines)]
fn profile_sections_share_environment_and_isolate_subject_nodes() {
    use vulcan_memorymesh_core_contracts::{
        EntityRef, EntitySlotId, ProfileLevel, ProfileNode, ProfileNodeId, ProfileNodeState,
        ProfileSourceKind, ProfileViewSection, SectionRole,
    };

    /// Builds one active atomic profile node for the requested source and scope.
    /// 为指定来源主体与作用域构建一个活跃原子画像节点。
    fn profile_node(subject: &str, scope: &str, content: &str) -> ProfileNode {
        ProfileNode {
            id: ProfileNodeId::new(format!("placeholder-{subject}-{scope}"))
                .expect("profile identity must be valid"),
            subject_id: Some(SubjectId::new(subject).expect("subject identity must be valid")),
            scope_id: ScopeId::new(scope).expect("scope identity must be valid"),
            content: content.to_owned(),
            confidence: Some(1.0),
            state: ProfileNodeState::Active,
            source_turn_ids: Vec::new(),
            source_kind: ProfileSourceKind::SystemSeed,
            source_id: None,
            evidence_source: None,
            priority: 50,
            profile_level: ProfileLevel::Stable,
            level_reason: None,
            refresh_weight: 0,
            effective_date_ns: Some(1),
            expires_at_ns: None,
            status_reason: None,
            superseded_by: None,
            version: 0,
            created_at_ns: 1,
            updated_at_ns: 1,
        }
    }

    let mut storage = InMemoryStorage::new();
    let mut subjectless_environment =
        profile_node("historical-unknown", "scope-environment", "project: legacy");
    subjectless_environment.subject_id = None;
    storage
        .mutate(&batch(
            "profile-section-visibility",
            vec![
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("environment-node-slot")
                            .expect("slot identity must be valid"),
                    ),
                    node: profile_node("subject-a", "scope-environment", "project: Vulcan"),
                },
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("subjectless-environment-node-slot")
                            .expect("slot identity must be valid"),
                    ),
                    node: subjectless_environment,
                },
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("subject-a-node-slot")
                            .expect("slot identity must be valid"),
                    ),
                    node: profile_node("subject-a", "scope-subject", "language: zh-CN"),
                },
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("subject-b-node-slot")
                            .expect("slot identity must be valid"),
                    ),
                    node: profile_node("subject-b", "scope-subject", "language: en-US"),
                },
            ],
        ))
        .expect("profile seed mutation must succeed");

    let environment_scope = ScopeId::new("scope-environment").expect("scope must be valid");
    let subject_scope = ScopeId::new("scope-subject").expect("scope must be valid");
    let result = storage
        .query(&StorageQuery::ProfileNodes {
            subject_id: SubjectId::new("subject-b").expect("subject identity must be valid"),
            scope_access: ScopeAccess {
                write_scope: subject_scope.clone(),
                visible_scopes: vec![environment_scope.clone(), subject_scope.clone()],
            },
            sections: vec![
                ProfileViewSection {
                    scope_id: environment_scope,
                    role: SectionRole::Environment,
                    display_label: "Project".to_owned(),
                },
                ProfileViewSection {
                    scope_id: subject_scope,
                    role: SectionRole::Subject,
                    display_label: "User".to_owned(),
                },
            ],
            evaluated_at_ns: 1,
        })
        .expect("profile query must succeed");
    let StorageQueryResult::ProfileNodes(nodes) = result else {
        unreachable!("profile query must return profile nodes")
    };
    let contents = nodes
        .iter()
        .map(|node| node.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        contents,
        vec!["project: Vulcan", "project: legacy", "language: en-US"]
    );
}

/// Verifies profile lifecycle filtering uses the explicit business-time cutoff and exclusive expiry.
/// 验证画像生命周期过滤使用显式业务时间截止点与排他过期边界。
#[test]
fn profile_nodes_filter_future_and_expired_nodes_at_business_time() {
    use vulcan_memorymesh_core_contracts::{
        EntityRef, EntitySlotId, Mutation, ProfileLevel, ProfileNode, ProfileNodeId,
        ProfileNodeState, ProfileSourceKind, ProfileViewSection, SectionRole,
    };
    let subject_id =
        SubjectId::new("subject-profile-time").expect("subject identity must be valid");
    let scope_id = ScopeId::new("scope-profile-time").expect("scope identity must be valid");
    let mut storage = InMemoryStorage::new();
    let node =
        |label: &str, effective_date_ns: Option<i64>, expires_at_ns: Option<i64>| ProfileNode {
            id: ProfileNodeId::new(format!("placeholder-{label}"))
                .expect("profile identity must be valid"),
            subject_id: Some(subject_id.clone()),
            scope_id: scope_id.clone(),
            content: label.to_owned(),
            confidence: Some(1.0),
            state: ProfileNodeState::Active,
            source_turn_ids: Vec::new(),
            source_kind: ProfileSourceKind::SystemSeed,
            source_id: None,
            evidence_source: None,
            priority: 50,
            profile_level: ProfileLevel::Stable,
            level_reason: None,
            refresh_weight: 0,
            effective_date_ns,
            expires_at_ns,
            status_reason: None,
            superseded_by: None,
            version: 0,
            created_at_ns: 1,
            updated_at_ns: 1,
        };
    // A pending node is durable evidence but never part of the active view, even with a current effective date.
    // 待评审节点是持久证据，但即使生效日期已到，也不属于活跃画像视图。
    let mut pending = node("pending", Some(1), None);
    pending.state = ProfileNodeState::Pending;
    pending.source_kind = ProfileSourceKind::TurnExtract;
    pending.source_turn_ids =
        vec![TurnId::new("pending-source-turn").expect("source turn must be valid")];
    storage
        .mutate(&batch(
            "profile-lifecycle-seed",
            vec![
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("profile-pending").expect("slot must be valid"),
                    ),
                    node: pending,
                },
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("profile-current").expect("slot must be valid"),
                    ),
                    node: node("current", Some(10), Some(30)),
                },
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("profile-future").expect("slot must be valid"),
                    ),
                    node: node("future", Some(21), None),
                },
                Mutation::PutProfileNode {
                    entity: EntityRef::New(
                        EntitySlotId::new("profile-expired").expect("slot must be valid"),
                    ),
                    node: node("expired", Some(1), Some(20)),
                },
            ],
        ))
        .expect("profile lifecycle seed must commit");
    let result = storage
        .query(&StorageQuery::ProfileNodes {
            subject_id,
            scope_access: ScopeAccess {
                write_scope: scope_id.clone(),
                visible_scopes: vec![scope_id.clone()],
            },
            sections: vec![ProfileViewSection {
                scope_id,
                role: SectionRole::Subject,
                display_label: "Subject".to_owned(),
            }],
            evaluated_at_ns: 20,
        })
        .expect("profile lifecycle query must succeed");
    let StorageQueryResult::ProfileNodes(nodes) = result else {
        unreachable!("profile lifecycle query must return nodes")
    };
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.content.as_str())
            .collect::<Vec<_>>(),
        vec!["current"]
    );
}
