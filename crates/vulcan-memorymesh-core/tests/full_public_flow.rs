//! Full public Rust facade call-chain tests with a deterministic typed host.
//! 使用确定性强类型宿主的完整公共 Rust 门面调用链测试。

#![allow(clippy::assertions_on_constants)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::cast_possible_truncation)]

use vulcan_memorymesh_core::{
    AcceptedCandidate, AcceptedProfileCandidate, AcceptedProfileNode, Admission,
    CandidateChannelResult, ChatCompactCommand, ChatCompactMutationResult, ChatCompactResult,
    CommandResult, CommandStatus, CommandStep, CommitReceipt, ContextEvidenceCandidate,
    ContextEvidenceRelation, CoreCommand, CoreError, EffectFailure, EffectFailureKind, EffectId,
    EffectRequest, EffectResult, EmbeddingEffect, EntityBinding, EntityKind, EntityRef,
    EntitySlotId, ErrorCode, ErrorStage, FailureClass, HostDriver, HostEffect, HostEffectResult,
    IdempotencyKey, InMemoryStorage, LlmEffect, LlmInput, LlmOutput, LlmPurpose, MaintenanceAction,
    MaintenanceCommand, MemoryAccessClass, MemoryCandidate, MemoryCategory, MemoryContextEvidence,
    MemoryDeleteBatchCommand, MemoryDeleteBatchItem, MemoryDeleteCommand,
    MemoryDetailsBatchCommand, MemoryDetailsCommand, MemoryId, MemoryLevel, MemoryMeshCore,
    MemoryRecord, MemoryScopeClass, MemorySearchBatchCommand, MemorySearchCommand,
    MemorySourceKind, MemoryStability, MemoryState, MemoryWriteBatchCommand, MemoryWriteBatchItem,
    MemoryWriteCommand, Mutation, MutationBatch, MutationIdentity, MutationOutcome,
    MutationSourceKind, OperationId, PostActionAnalyzePendingTurnCommand, PostActionIntakeCommand,
    PostActionReferenceTurn, PreCheckCommand, ProfileCandidate, ProfileInstructionCommand,
    ProfileLevel, ProfileNode, ProfileNodeId, ProfileNodeState, ProfileQueryCommand,
    ProfileSourceKind, ProfileView, ProfileViewMode, ProfileViewSection, RecallMode,
    RetentionColdTurnCandidateFact, RetentionColdTurnJobFact, RetentionCommand,
    RetentionIdleSessionFact, RetentionMemoryFact, RetentionPlanSet, RetentionPolicy,
    RetentionReasonCode, RetentionScanFacts, RetentionTrashBatchFact, RetentionVectorGcJobFact,
    Role, ScopeAccess, ScopeId, SectionRole, SessionRecallMode, SessionSourceId, StoragePort,
    StorageQuery, StorageQueryResult, SubjectId, TimelineItem, TurnId, TurnInputKind,
    VectorSpaceId, VersionedEntity, VersionedMemoryRef, request_fingerprint,
};

// Keeps rehashed L2 checkpoint counterexamples next to the real public-flow fixture.
// 将重算摘要后的L2检查点反例与真实公共流程夹具关联。
#[path = "support/review_context.rs"]
mod review_context;

// Runs external-generated profile timeline evidence through the existing public query fixture.
// 通过既有公共查询夹具执行外置生成的画像时间线证据。
#[path = "support/profile_timeline_parity.rs"]
mod profile_timeline_parity;

/// Preserves the existing fixture's readable scopes while explicitly allowing replacement only in its primary write scope.
/// 保留既有用例的可读范围，同时仅显式允许替代其主要写作用域。
fn fixture_replacement_scope(
    access: &ScopeAccess,
) -> vulcan_memorymesh_core::MemoryReplacementScope {
    vulcan_memorymesh_core::MemoryReplacementScope {
        candidates: vulcan_memorymesh_core::MemoryCandidateScope {
            scope_ids: access.visible_scopes.clone(),
            origin_session_source_id: None,
        },
        authorized_supersede_scopes: vec![access.write_scope.clone()],
    }
}

#[test]
fn retention_zero_limits_disable_individual_plan_families() {
    const NOW_NS: i64 = 10_000_000;
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut policy = retention_policy();
    policy.memory_recycle_limit = 0;
    policy.cold_turn_scan_limit = 0;
    policy.cold_turn_job_limit = 0;
    policy.idle_session_limit = 0;
    policy.vector_gc_limit = 1;
    policy.trash_purge_limit = 0;
    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::Retention(RetentionCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                now_ns: NOW_NS,
                policy,
                mutation_identity: identity,
            })
        },
        "retention:vector-only",
        NOW_NS,
    );
    let (handle, step) = core
        .start(command)
        .expect("family-disabled Retention must start");
    let CommandStep::Effects(mut effects) = step else {
        panic!("family-disabled Retention must request storage facts");
    };
    let effect = effects
        .pop()
        .expect("family-disabled Retention query must exist");
    let completed = core
        .resume(
            &handle,
            EffectResult {
                effect_id: effect.effect_id,
                result: HostEffectResult::StorageQuery(StorageQueryResult::RetentionScan(
                    RetentionScanFacts {
                        subject_id,
                        scope_id: scope_access.write_scope,
                        expired_profile_nodes: Vec::new(),
                        terminal_memories: Vec::new(),
                        cold_turn_candidates: Vec::new(),
                        cold_turn_jobs: Vec::new(),
                        idle_sessions: Vec::new(),
                        vector_gc_jobs: vec![RetentionVectorGcJobFact {
                            job_id: "vector-only-due".to_owned(),
                            vector_id: "vector-only-due".to_owned(),
                            next_run_at_ns: NOW_NS,
                        }],
                        trash_batches: Vec::new(),
                    },
                )),
            },
        )
        .expect("enabled vector-only Retention facts must complete");
    let CommandStep::Complete(result) = completed else {
        panic!("family-disabled Retention facts must complete synchronously");
    };
    let CommandResult::Retention(plan) = result.as_ref() else {
        panic!("family-disabled Retention must return one typed plan set");
    };
    assert!(plan.terminal_memory_recycle.is_empty());
    assert!(plan.cold_turn_enqueue.is_empty());
    assert!(plan.cold_turn_execute.is_empty());
    assert!(plan.idle_session_recycle.is_empty());
    assert_eq!(plan.vector_gc_retry.len(), 1);
    assert!(plan.trash_purge.is_empty());
}

/// Verifies one newly enqueued cold-turn session is not simultaneously planned as idle cleanup.
/// 验证一个新入队冷回合作业的会话不会同时被规划为闲置清洗。
#[test]
fn retention_sequences_new_cold_turn_work_before_idle_cleanup() {
    const NOW_NS: i64 = 10_000_000;
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut policy = retention_policy();
    policy.profile_expiry_limit = 0;
    policy.memory_recycle_limit = 0;
    policy.cold_turn_scan_limit = 1;
    policy.cold_turn_job_limit = 0;
    policy.idle_session_limit = 1;
    policy.vector_gc_limit = 0;
    policy.trash_purge_limit = 0;
    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::Retention(RetentionCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                now_ns: NOW_NS,
                policy,
                mutation_identity: identity,
            })
        },
        "retention:cold-before-idle",
        NOW_NS,
    );
    let (handle, step) = core.start(command).expect("Retention must start");
    let CommandStep::Effects(mut effects) = step else {
        panic!("Retention must request storage facts");
    };
    let effect = effects.pop().expect("Retention query must exist");
    let session_source_id = SessionSourceId::new("session-a").expect("session id");
    let completed = core
        .resume(
            &handle,
            EffectResult {
                effect_id: effect.effect_id,
                result: HostEffectResult::StorageQuery(StorageQueryResult::RetentionScan(
                    RetentionScanFacts {
                        subject_id,
                        scope_id: scope_access.write_scope,
                        expired_profile_nodes: Vec::new(),
                        terminal_memories: Vec::new(),
                        cold_turn_candidates: vec![RetentionColdTurnCandidateFact {
                            session_source_id: session_source_id.clone(),
                            recyclable_turn_count: 2,
                        }],
                        cold_turn_jobs: Vec::new(),
                        idle_sessions: vec![RetentionIdleSessionFact {
                            session_source_id,
                            last_activity_ns: 1,
                            has_pending_turn: false,
                        }],
                        vector_gc_jobs: Vec::new(),
                        trash_batches: Vec::new(),
                    },
                )),
            },
        )
        .expect("Retention facts must complete");
    let CommandStep::Complete(result) = completed else {
        panic!("Retention facts must complete synchronously");
    };
    let CommandResult::Retention(plan) = result.as_ref() else {
        panic!("Retention must return one typed plan set");
    };
    assert_eq!(plan.cold_turn_enqueue.len(), 1);
    assert!(plan.idle_session_recycle.is_empty());
}

/// Verifies every retention family, including profile expiry, obeys the public maximum bound.
/// 验证包括画像过期在内的每个保留计划族都遵守公共最大上限。
#[test]
fn retention_rejects_oversized_profile_expiry_limit() {
    const NOW_NS: i64 = 10_000_000;
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut policy = retention_policy();
    policy.profile_expiry_limit = 10_001;
    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::Retention(RetentionCommand {
                subject_id,
                scope_access,
                now_ns: NOW_NS,
                policy,
                mutation_identity: identity,
            })
        },
        "retention:oversized-profile-expiry",
        NOW_NS,
    );

    let error = core
        .start(command)
        .expect_err("oversized profile expiry limit must fail");

    assert_eq!(error.code, ErrorCode::InvalidArgument);
}

/// Restricts physical candidates to every scope explicitly visible to the fixture command.
/// 将物理候选限制在夹具命令显式可见的全部作用域内。
fn fixture_candidate_scope(access: &ScopeAccess) -> vulcan_memorymesh_core::MemoryCandidateScope {
    vulcan_memorymesh_core::MemoryCandidateScope {
        scope_ids: access.visible_scopes.clone(),
        origin_session_source_id: None,
    }
}

/// Builds an explicit public-search boundary that preserves the fixture's prior visible-scope semantics.
/// 构建显式公共搜索边界，保留夹具原有的可见作用域语义。
fn fixture_search_boundary(access: &ScopeAccess) -> vulcan_memorymesh_core::CandidateQueryBoundary {
    vulcan_memorymesh_core::CandidateQueryBoundary {
        candidate_scope: Some(fixture_candidate_scope(access)),
        session_recall_boundary: None,
    }
}

/// Builds one complete shared-memory record for public preflight and lifecycle tests.
/// 为公共预检与生命周期测试构建一条完整共享记忆记录。
fn shared_memory_record(identity: &str, scope_id: &ScopeId, state: MemoryState) -> MemoryRecord {
    MemoryRecord {
        id: MemoryId::new(identity).expect("memory identity must be valid"),
        access_class: MemoryAccessClass::ScopeShared,
        source_subject_id: None,
        scope_id: scope_id.clone(),
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::General,
        summary: "shared memory".to_owned(),
        content: "shared memory details".to_owned(),
        source_kind: MemorySourceKind::Direct,
        origin_session_source_id: None,
        priority: 50,
        stability: MemoryStability::Stable,
        expires_at_ns: None,
        last_recalled_at_ns: None,
        reinforcement_count: 0,
        tags: Vec::new(),
        evidence: Vec::new(),
        evidence_source: None,
        state,
        importance: 0.5,
        confidence: Some(1.0),
        created_at_ns: 1,
        updated_at_ns: 1,
        version: 0,
        vector_space_id: None,
        embedding: None,
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

/// Seeds one shared-memory record directly through the typed reference storage transaction.
/// 通过强类型参考存储事务直接播种一条共享记忆记录。
fn seed_shared_memory(host: &mut DeterministicHost, record: MemoryRecord, key: &str) {
    let memory_id = record.id.clone();
    let outcome = host
        .storage
        .mutate(&MutationBatch {
            identity: MutationIdentity {
                operation_id: OperationId::new(format!("seed-operation-{key}"))
                    .expect("operation identity must be valid"),
                idempotency_key: IdempotencyKey::new(format!("seed-idempotency-{key}"))
                    .expect("idempotency identity must be valid"),
                request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                source_kind: MutationSourceKind::HostRpc,
                request_source_id: "shared-memory-public-test".to_owned(),
                observed_at_ns: 1,
            },
            mutations: vec![Mutation::PutMemory {
                entity: EntityRef::Existing {
                    domain_id: memory_id.as_str().to_owned(),
                    expected_version: 0,
                },
                record,
            }],
        })
        .expect("shared memory seed must return an outcome");
    assert!(matches!(outcome, MutationOutcome::Committed { .. }));
}

/// Executes every host capability deterministically while preserving typed boundaries.
/// 在保持强类型边界的同时确定性执行每项宿主能力。
#[allow(clippy::struct_excessive_bools)]
struct DeterministicHost {
    /// Stores all semantic data through the reference storage port.
    /// 通过参考存储端口存储全部语义数据。
    storage: InMemoryStorage,
    /// Forces the next storage mutation outcome to be uncertain.
    /// 强制下一次存储变更结果不确定。
    uncertain_next_mutation: bool,
    /// Commits the next storage mutation before returning a Host execution error.
    /// 提交下一次存储变更后返回宿主执行错误。
    fail_after_committed_mutation: bool,
    /// Counts every `PostAction` L2 review LLM invocation.
    /// 统计每次 `PostAction` L2 审核 LLM 调用。
    review_llm_calls: u32,
    /// Tracks the largest similar-memory count the review ever received.
    /// 记录审核曾经收到的最大相似记忆数量。
    max_review_similar_count: usize,
    /// Forces every lexical candidate query to fail with a structured host failure.
    /// 强制每次词法候选查询以结构化宿主失败失败。
    fail_lexical: bool,
    /// Forces every `PreCheck` L2 review effect to fail with a structured host failure.
    /// 强制每次 `PreCheck` L2 审核 Effect 以结构化宿主失败失败。
    fail_precheck_review: bool,
    /// Appends one foreign identity after valid `PreCheck` review candidates.
    /// 在有效的 `PreCheck` 审核候选之后追加一个外来标识。
    append_foreign_precheck_identity: bool,
    /// Injects one cross-subject historical reference into `PostAction` context.
    /// 向 `PostAction` 上下文注入一条跨主体历史参考。
    inject_foreign_postaction_reference: bool,
    /// Corrupts the pending target into an already-completed turn before analysis.
    /// 在分析前将待处理目标篡改为已完成轮次。
    complete_postaction_target_before_analysis: bool,
    /// Injects NUL-bearing metadata into one Host memory-details result.
    /// 向一份 Host 记忆详情结果注入带 NUL 的元数据。
    inject_invalid_memory_metadata: bool,
    /// Injects future-dated context evidence into one retrieval result.
    /// 向一份检索结果注入未来时间的情境证据。
    inject_future_context_evidence: bool,
    /// Appends one expired active-state profile fact that a Host failed to lifecycle-filter.
    /// 追加一条宿主未执行生命周期预过滤的已过期 active 状态画像事实。
    inject_expired_profile_fact: bool,
    /// Corrupts one returned profile source kind without changing its incompatible source turns.
    /// 篡改一条返回画像的来源类别，同时保留与之不兼容的来源轮次。
    inject_invalid_profile_source: bool,
    /// Forces physical candidate scores below final thresholds without changing stored embeddings.
    /// 在不改变已存嵌入的前提下强制物理候选分低于最终阈值。
    depress_candidate_scores: bool,
    /// Forces physical vector and lexical candidate queries to return empty channels.
    /// 强制物理向量与词法候选查询返回空通道。
    suppress_candidate_queries: bool,
    /// Forces finite Provider rerank results above the public Core score interval.
    /// 强制有限 Provider 重排结果高于 Core 公共评分区间。
    out_of_range_rerank_scores: bool,
    /// Selects the deterministic LLM scenario driving analysis and review.
    /// 选择驱动分析与审核的确定性 LLM 场景。
    scenario: HostScenario,
}

/// Observes the exact L1 inputs while returning a valid empty extraction through the real Core flow.
/// 观察精确 L1 输入，同时通过真实 Core 流程返回有效空提取。
struct SessionWindowHost {
    /// Executes the existing deterministic embedding, query and atomic storage ports.
    /// 执行既有确定性嵌入、查询及原子存储端口。
    inner: DeterministicHost,
    /// Records every inclusive cutoff and direct-write ID list actually delivered to L1.
    /// 记录实际送到 L1 的每个包含式截止时间及直写 ID 列表。
    l1_windows: Vec<(i64, Vec<MemoryId>)>,
    /// Fails exactly one L1 call without producing any successful derived mutation.
    /// 令恰好一次 L1 调用失败，不产生任何成功派生变更。
    fail_next_l1: bool,
}

impl HostDriver for SessionWindowHost {
    /// Executes `request`, observing L1 or delegating unchanged effects to the test host.
    /// 执行 `request`，观察 L1 或将其他 Effect 原样委托给测试宿主。
    /// Returns a valid empty structured extraction or the explicitly requested one-shot failure.
    /// 返回有效空结构化提取，或明确要求的单次失败。
    fn execute_effect(&mut self, request: EffectRequest) -> Result<EffectResult, CoreError> {
        if let HostEffect::Llm(effect) = &request.effect
            && let LlmInput::PostActionAnalysis {
                target_turn,
                recent_direct_writes,
                current_timestamp_ns,
                ..
            } = &effect.input
        {
            self.l1_windows.push((
                *current_timestamp_ns,
                recent_direct_writes
                    .iter()
                    .map(|item| item.memory_id.clone())
                    .collect(),
            ));
            if self.fail_next_l1 {
                self.fail_next_l1 = false;
                return Err(CoreError::new(
                    ErrorCode::Internal,
                    ErrorStage::Effect,
                    "one-shot test analysis failure",
                ));
            }
            return Ok(EffectResult {
                effect_id: request.effect_id,
                result: HostEffectResult::Llm(LlmOutput::PostActionAnalysis {
                    user_input_kind: TurnInputKind::Statement,
                    turn_id: target_turn.id.clone(),
                    details: "Existing direct writes already capture all durable facts.".to_owned(),
                    memories: Vec::new(),
                    profiles: Vec::new(),
                    context_evidence: Vec::new(),
                }),
            });
        }
        self.inner.execute_effect(request)
    }
}

/// Writes one direct fact through the real command path and returns its committed memory ID.
/// 通过真实命令路径写入一条直接事实，并返回已提交记忆 ID。
/// `key` is both unique operation text and content; `session` and `at` fix ownership and observation time.
/// `key` 同时用作唯一操作文本和内容；`session` 与 `at` 固定归属及观察时间。
fn write_window_fact(
    core: &MemoryMeshCore,
    host: &mut SessionWindowHost,
    key: &str,
    session: &SessionSourceId,
    at: i64,
) -> MemoryId {
    // identities are the same canonical fixture identities used by intake and analysis below.
    // identities 与下方登记及分析使用同一组规范样本身份。
    let (subject_id, scope_access, vector_space_id) = identities();
    // written includes actual embedding and transactional storage, not a manually inserted record.
    // written 包含实际嵌入及事务存储，而不是手动插入记录。
    let written = core
        .run_to_completion(
            with_identity(
                core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id,
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::ProjectContext,
                            key,
                            Vec::new(),
                            20,
                        ),
                        scope_access,
                        session_source_id: session.clone(),
                        vector_space_id,
                        occurred_at_ns: at,
                        mutation_identity: identity,
                    })
                },
                key,
                at,
            ),
            host,
        )
        .expect("direct fact");
    match written {
        CommandResult::MemoryWrite(result) => result.memory.id,
        _ => panic!("memory write result"),
    }
}

/// Exercises queued turns, a failed analysis, three successes and independent-session L1 windows.
/// 验证积压回合、一次失败分析、三次成功及独立会话的 L1 窗口。
#[test]
fn l1_observes_only_direct_writes_after_the_last_successful_session_window() {
    // core and host drive the complete intake/query/LLM/derived-commit sequence.
    // core 与 host 驱动完整登记、查询、LLM 和派生提交序列。
    let core = MemoryMeshCore::new().expect("core");
    let mut host = SessionWindowHost {
        inner: DeterministicHost::new(),
        l1_windows: Vec::new(),
        fail_next_l1: false,
    };
    host.inner.suppress_candidate_queries = true;
    // owner identifies the same subject and writing project for both isolated sessions.
    // owner 为两个隔离会话指定相同主体及写入项目。
    let (subject_id, scope_access, vector_space_id) = identities();
    // sessions differ only in their explicit host session identity.
    // sessions 仅在明确的宿主会话身份上不同。
    let sessions = [
        SessionSourceId::new("window-session-a").expect("session A"),
        SessionSourceId::new("window-session-b").expect("session B"),
    ];
    // first is written before every queued turn.
    // first 在全部积压回合之前写入。
    let first = write_window_fact(&core, &mut host, "window-fact-first", &sessions[0], 10);
    // turns are all registered before any analysis, so intake cannot supply the later watermarks.
    // turns 全部在任何分析之前登记，因此登记时无法提供后续水位。
    let mut turns = Vec::new();
    for index in 0..4 {
        // session chooses the independent session only for the last turn.
        // session 仅为最后一个回合选择独立会话。
        let session = &sessions[usize::from(index == 3)];
        // at is the earlier user-turn time, not the later extraction cutoff.
        // at 是较早的用户回合时间，而不是较晚的提取截止时间。
        let at = 21 + index;
        // accepted runs normal noise/PII intake and persists the authoritative session record.
        // accepted 执行正常 Noise/PII 登记并持久化权威会话记录。
        let accepted = core
            .run_to_completion(
                with_identity(
                    &core,
                    |identity| {
                        CoreCommand::PostActionIntake(PostActionIntakeCommand {
                            input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                            subject_id: subject_id.clone(),
                            scope_access: scope_access.clone(),
                            session_source_id: session.clone(),
                            vector_space_id: vector_space_id.clone(),
                            timeline: vec![TimelineItem {
                                role: Role::User,
                                content:
                                    "我下周要出差去北京，我喜欢提前订票，请记录我的长期出行偏好"
                                        .to_owned(),
                                occurred_at_ns: at,
                            }],
                            completed_at_ns: at,
                            mutation_identity: identity,
                        })
                    },
                    &format!("window-intake-{index}"),
                    at,
                ),
                &mut host,
            )
            .expect("intake");
        let CommandResult::PostActionAccepted(accepted) = accepted else {
            panic!("intake result")
        };
        turns.push(accepted.turn_id.expect("pending turn"));
    }
    // other shares subject/project but must never enter session A's L1 direct-write input.
    // other 共享主体和项目，但绝不能进入会话 A 的 L1 直写输入。
    let other = write_window_fact(&core, &mut host, "window-fact-other", &sessions[1], 50);
    // expected stores only IDs actually returned by successful direct-write commands.
    // expected 仅保存成功直写命令实际返回的 ID。
    let mut expected = vec![
        (100, vec![first]),
        (200, Vec::new()),
        (200, Vec::new()),
        (300, Vec::new()),
        (100, vec![other]),
    ];
    for (index, cutoff) in [(0, 100), (1, 200), (2, 300), (3, 100)] {
        if index == 1 {
            // second arrives after the first success and must appear in the retry but not later rounds.
            // second 在首次成功后到达，必须出现在重试中，但不能出现在更晚的轮次。
            let second =
                write_window_fact(&core, &mut host, "window-fact-second", &sessions[0], 150);
            expected[1].1 = vec![second.clone()];
            expected[2].1 = vec![second];
        }
        // snapshot supplies the latest committed boundary at this command's execution boundary.
        // snapshot 在本命令执行边界提供最新已提交边界。
        let snapshot = host.inner.storage.snapshot();
        // session_record is selected by complete ownership, including the writing project.
        // session_record 按包含写入项目的完整归属选择。
        let session_record = snapshot
            .memory_sessions
            .iter()
            .find(|record| {
                record.subject_id == subject_id
                    && record.scope_id == scope_access.write_scope
                    && record.session_source_id == sessions[usize::from(index == 3)]
            })
            .expect("session record");
        // command freezes this execution's observation interval through every Core effect.
        // command 在每个 Core Effect 中冻结本次执行的观察区间。
        let command = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    profile_sections: vec![subject_profile_section(&scope_access)],
                    session_source_id: session_record.session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    turn_id: turns[index].clone(),
                    analysis_generation: 1,
                    analysis_cutoff_ns: cutoff,
                    direct_write_observed_after_ns: session_record.last_extraction_observed_at_ns,
                    mutation_identity: identity,
                })
            },
            &format!("window-analysis-{index}"),
            cutoff,
        );
        if index == 1 {
            host.fail_next_l1 = true;
            assert!(core.run_to_completion(command.clone(), &mut host).is_err());
            assert_eq!(
                host.inner.storage.snapshot().memory_sessions,
                snapshot.memory_sessions
            );
        }
        assert!(matches!(
            core.run_to_completion(command, &mut host)
                .expect("analysis"),
            CommandResult::PostActionAnalysis(_)
        ));
    }
    assert_eq!(host.l1_windows, expected);
}

/// Selects one deterministic LLM scenario for the analysis and review chain.
/// 为分析与审核链路选择一种确定性 LLM 场景。
#[derive(Clone, Copy, PartialEq, Eq)]
enum HostScenario {
    /// One admitted candidate and full acceptance.
    /// 单已准入候选与全量接受。
    Default,
    /// Three candidates with the first dropped, plus an L2 review accepting only the
    /// second admitted candidate.
    /// 三个候选且首个被丢弃，L2 审核仅接受第二个已准入候选。
    SparseAcceptance,
    /// One admitted candidate declaring a full lifecycle: priority, expiry, supersede,
    /// and an evidence source.
    /// 一个声明完整生命周期的已准入候选：优先级、过期、替代与证据来源。
    PrioritySupersede,
}

/// Writes one mutation identity into whichever mutating command variant is present.
/// 将一个变更身份写入当前存在的变更命令变体。
fn apply_identity(command: &mut CoreCommand, identity: MutationIdentity) {
    match command {
        CoreCommand::PreCheck(payload) => payload.mutation_identity = identity,
        CoreCommand::PostActionIntake(payload) => payload.mutation_identity = identity,
        CoreCommand::PostActionAnalyzePendingTurn(payload) => payload.mutation_identity = identity,
        CoreCommand::ChatCompact(payload) => payload.mutation_identity = identity,
        CoreCommand::MemoryWrite(payload) => payload.mutation_identity = identity,
        CoreCommand::MemoryDelete(payload) => payload.mutation_identity = identity,
        CoreCommand::ProfileInstruction(payload) => payload.mutation_identity = identity,
        CoreCommand::Retention(payload) => payload.mutation_identity = identity,
        CoreCommand::MemoryWriteBatch(payload) => payload.mutation_identity = identity,
        CoreCommand::MemoryDeleteBatch(payload) => payload.mutation_identity = identity,
        CoreCommand::Maintenance(payload) => payload.mutation_identity = identity,
        CoreCommand::MemorySearch(_)
        | CoreCommand::MemoryDetails(_)
        | CoreCommand::ProfileQuery(_)
        | CoreCommand::MemorySearchBatch(_)
        | CoreCommand::MemoryDetailsBatch(_) => {}
    }
}

/// Builds one mutating command bound to the active snapshot's canonical fingerprint.
/// 构建一个绑定活跃快照规范指纹的变更命令。
fn with_identity(
    core: &MemoryMeshCore,
    build: impl FnOnce(MutationIdentity) -> CoreCommand,
    idempotency_key: &str,
    observed_at_ns: i64,
) -> CoreCommand {
    // Seed with an empty fingerprint; the canonical material excludes identity fields.
    // 以空指纹播种；规范材料排除身份字段。
    let seed = MutationIdentity {
        operation_id: OperationId::new(format!("op-{idempotency_key}"))
            .expect("operation identity must be valid"),
        idempotency_key: IdempotencyKey::new(idempotency_key)
            .expect("idempotency identity must be valid"),
        request_fingerprint: String::new(),
        source_kind: MutationSourceKind::HostRpc,
        request_source_id: "full-public-flow-test".to_owned(),
        observed_at_ns,
    };
    let snapshot_id = core
        .active_snapshot()
        .expect("active snapshot must exist")
        .id
        .as_str()
        .to_owned();
    let mut command = build(seed.clone());
    let identity = MutationIdentity {
        request_fingerprint: request_fingerprint(&command, &snapshot_id),
        ..seed
    };
    apply_identity(&mut command, identity);
    command
}

/// Builds the stable direct-write session used by tests that do not exercise session exclusion.
/// 构建不验证会话排除语义的测试所共用的稳定主动写入会话。
fn test_direct_session() -> SessionSourceId {
    SessionSourceId::new("session-direct-test").expect("session identity must be valid")
}

/// Builds one environment-scoped direct-write item with explicit VMM-compatible priority.
/// 构建一条环境作用域主动写入项，并显式携带与 VMM 兼容的优先级。
fn direct_write_item(
    target_scope_id: ScopeId,
    category: MemoryCategory,
    content: impl Into<String>,
    tags: Vec<String>,
    priority: u32,
) -> MemoryWriteBatchItem {
    let content = content.into();
    MemoryWriteBatchItem {
        target_scope_id,
        scope_class: MemoryScopeClass::Environment,
        category,
        summary: content.clone(),
        details: content,
        priority: Some(priority),
        memory_level: Some(MemoryLevel::Stable),
        expires_at_ns: None,
        tags,
    }
}

/// Drives retryable read-only effects until the command exposes its storage mutation.
/// 驱动可重试只读 Effect，直到命令暴露其存储变更。
fn drive_to_storage_mutation(
    core: &MemoryMeshCore,
    handle: &vulcan_memorymesh_core::CommandHandle,
    host: &mut DeterministicHost,
    mut step: CommandStep,
) -> EffectRequest {
    loop {
        let CommandStep::Effects(mut effects) = step else {
            panic!("command completed before exposing its storage mutation");
        };
        assert_eq!(effects.len(), 1, "test host expects one sequential effect");
        let request = effects.pop().expect("effect must exist");
        if matches!(request.effect, HostEffect::StorageMutation(_)) {
            return request;
        }
        assert!(request.retryable);
        assert!(!request.side_effect_possible);
        let result = host
            .execute_effect(request)
            .expect("read-only effect must succeed");
        step = core
            .resume(handle, result)
            .expect("read-only effect resume must succeed");
    }
}

/// Builds the one subject profile section bound to a command's write scope.
/// 构建绑定到命令写入作用域的唯一主体画像分区。
fn subject_profile_section(scope_access: &ScopeAccess) -> ProfileViewSection {
    ProfileViewSection {
        scope_id: scope_access.write_scope.clone(),
        role: SectionRole::Subject,
        display_label: "Subject".to_owned(),
    }
}

/// Builds one structurally valid committed outcome for lifecycle tests.
/// 为生命周期测试构建一个结构有效的已提交结果。
fn committed_outcome(
    request: &EffectRequest,
    storage_version: u64,
    chat_compact: Option<ChatCompactMutationResult>,
) -> MutationOutcome {
    MutationOutcome::Committed {
        storage_version,
        chat_compact,
        receipt: receipt_for_request(request),
    }
}

/// Builds the exact durable receipt identity expected for one mutation request.
/// 为一个变更请求构建预期的精确持久收据身份。
fn receipt_for_request(request: &EffectRequest) -> CommitReceipt {
    let HostEffect::StorageMutation(batch) = &request.effect else {
        panic!("commit receipt requires a storage mutation request");
    };
    CommitReceipt {
        operation_id: batch.identity.operation_id.clone(),
        idempotency_key: batch.identity.idempotency_key.clone(),
        request_fingerprint: batch.identity.request_fingerprint.clone(),
        entity_bindings: Vec::new(),
        committed_versions: Vec::new(),
        side_effect_summary: String::new(),
    }
}

/// Builds one committed receipt for the fixed single direct-memory creation slot.
/// 为固定的单条主动记忆创建槽位构建一份已提交收据。
fn committed_direct_memory_outcome(
    identity: &MutationIdentity,
    storage_version: u64,
    domain_id: &str,
) -> MutationOutcome {
    MutationOutcome::Committed {
        storage_version,
        chat_compact: None,
        receipt: CommitReceipt {
            operation_id: identity.operation_id.clone(),
            idempotency_key: identity.idempotency_key.clone(),
            request_fingerprint: identity.request_fingerprint.clone(),
            entity_bindings: vec![EntityBinding {
                entity_slot_id: EntitySlotId::new("direct-memory-0")
                    .expect("direct memory slot must be valid"),
                entity_kind: EntityKind::Memory,
                domain_id: domain_id.to_owned(),
            }],
            committed_versions: vec![VersionedEntity {
                entity_kind: EntityKind::Memory,
                domain_id: domain_id.to_owned(),
                version: 1,
            }],
            side_effect_summary: "put-memory=1".to_owned(),
        },
    }
}

/// Builds the frozen-style retention policy used by lifecycle assertions.
/// 构建生命周期断言使用的冻结式保留策略。
const fn retention_policy() -> RetentionPolicy {
    RetentionPolicy {
        schema_version: 1,
        turn_hot_window_size: 1,
        session_idle_ms: 1,
        trash_retention_ms: 1,
        protected_priority_floor: 100,
        protected_memory_level_floor: MemoryLevel::Persistent as u32,
        skip_protected_shared_memories: false,
        profile_expiry_limit: 100,
        memory_recycle_limit: 100,
        cold_turn_scan_limit: 100,
        cold_turn_job_limit: 100,
        idle_session_limit: 100,
        vector_gc_limit: 100,
        trash_purge_limit: 100,
    }
}

/// Applies the memory-recycle portions of one Core-owned retention plan through the Host port.
/// 通过宿主端口应用一个 Core 自有保留计划中的记忆回收部分。
fn apply_retention_recycle_plan(host: &mut DeterministicHost, plan: &RetentionPlanSet) {
    let mutations = plan
        .terminal_memory_recycle
        .iter()
        .map(|item| Mutation::RecycleMemory {
            actor_subject_id: plan.subject_id.clone(),
            scope_id: plan.scope_id.clone(),
            expected_access: item.expected_access.clone(),
            memory_id: MemoryId::new(&item.memory_id)
                .expect("planned memory identity must be valid"),
            expected_version: item.expected_version,
            status_reason: Some(
                match item.reason {
                    RetentionReasonCode::TerminalMemoryRecycle => "retention-terminal-memory",
                    RetentionReasonCode::ColdTurnRecycle => "retention-cold-turn",
                    RetentionReasonCode::IdleSessionRecycle => "retention-idle-session",
                    RetentionReasonCode::TrashExpiryPurge => "retention-trash-expiry",
                    RetentionReasonCode::VectorGcRetry => "retention-vector-gc",
                    RetentionReasonCode::ProfileExpiry => "retention-profile-expiry",
                    RetentionReasonCode::ManualSessionRecycle => "manual-session-recycle",
                    RetentionReasonCode::ManualTurnRecycle => "manual-turn-recycle",
                }
                .to_owned(),
            ),
        })
        .collect();
    let outcome = host
        .storage
        .mutate(&MutationBatch {
            identity: plan.identity.clone(),
            mutations,
        })
        .expect("host retention plan execution must succeed");
    assert!(matches!(outcome, MutationOutcome::Committed { .. }));
}

/// Transitions one reference-host memory into the terminal superseded state for retention tests.
/// 在保留测试中把一条参考 Host 记忆迁移到终态 Superseded 状态。
fn terminalize_memory(host: &mut DeterministicHost, memory_id: &str, sequence: u64) {
    let mut record = host
        .storage
        .dataset()
        .memories
        .into_iter()
        .find(|memory| memory.id.as_str() == memory_id)
        .expect("terminalized memory must exist");
    let expected_version = record.version;
    record.state = MemoryState::Superseded;
    record.status_reason = Some("superseded-before-retention".to_owned());
    let outcome = host
        .storage
        .mutate(&MutationBatch {
            identity: MutationIdentity {
                operation_id: OperationId::new(format!("op-terminalize-{sequence}"))
                    .expect("operation identity must be valid"),
                idempotency_key: IdempotencyKey::new(format!("terminalize-{sequence}"))
                    .expect("idempotency identity must be valid"),
                request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                source_kind: MutationSourceKind::BackgroundWorker,
                request_source_id: "retention-test".to_owned(),
                observed_at_ns: i64::try_from(sequence)
                    .expect("bounded test sequence must fit signed time"),
            },
            mutations: vec![Mutation::PutMemory {
                entity: EntityRef::Existing {
                    domain_id: memory_id.to_owned(),
                    expected_version,
                },
                record,
            }],
        })
        .expect("terminal state transition must succeed");
    assert!(matches!(outcome, MutationOutcome::Committed { .. }));
}

/// Completes shared-environment profile replacement and retirement across different creators.
/// 跨不同创建主体完成共享环境画像替代及仅退役，并验证恢复与可移植数据链。
#[test]
fn shared_environment_profiles_preserve_owner_through_analysis() {
    for retire_only in [false, true] {
        let mut core = MemoryMeshCore::new().expect("Core");
        // Distinct project/user durations prove environment profiles do not inherit the user rule.
        // 不同项目/用户期限证明环境画像不会继承用户规则。
        let mut policy = vulcan_memorymesh_core::ConfigBundle {
            name: "profile-lifetime".to_owned(),
            ..Default::default()
        };
        policy.memory_policy.profiles.user.stable.base_ms = 17;
        policy.memory_policy.profiles.project.stable.base_ms = 31;
        policy.memory_policy.profiles.project.stable.step_ms = 0;
        core.register_config(policy.clone())
            .expect("profile policy");
        core.activate_config("profile-lifetime")
            .expect("activate profile policy");
        let (subject_id, scope_access, vector_space_id) = identities();
        let owner = SubjectId::new("other-profile-creator").expect("owner");
        let mut host = DeterministicHost::new();
        let previous_id = seed_profile_node(&mut host, &owner, &scope_access.write_scope, 90);
        assert_eq!(
            query_profile_facts(
                &core,
                subject_id.clone(),
                scope_access.clone(),
                ProfileView {
                    mode: ProfileViewMode::Split,
                    sections: vec![subject_profile_section(&scope_access)],
                    include_explanations: false,
                    display_timezone: "UTC".to_owned(),
                },
                host.storage.dataset().profile_nodes,
            )
            .expect_err("private Subject section must still reject another creator")
            .code,
            ErrorCode::EffectMismatch
        );
        let session = SessionSourceId::new("shared-profile-session").expect("session");
        let intake = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionIntake(PostActionIntakeCommand {
                    input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: session.clone(),
                    vector_space_id: vector_space_id.clone(),
                    timeline: vec![TimelineItem {
                        role: Role::User,
                        content: "The project uses Rust for durable storage".to_owned(),
                        occurred_at_ns: 100,
                    }],
                    completed_at_ns: 100,
                    mutation_identity: identity,
                })
            },
            "shared-profile-intake",
            100,
        );
        let CommandResult::PostActionAccepted(intake) =
            core.run_to_completion(intake, &mut host).expect("intake")
        else {
            panic!("intake result")
        };
        let command = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    profile_sections: vec![ProfileViewSection {
                        scope_id: scope_access.write_scope.clone(),
                        role: SectionRole::Environment,
                        display_label: "Environment".to_owned(),
                    }],
                    session_source_id: session.clone(),
                    vector_space_id: vector_space_id.clone(),
                    turn_id: intake.turn_id.clone().expect("pending turn"),
                    analysis_generation: 1,
                    analysis_cutoff_ns: 101,
                    direct_write_observed_after_ns: 0,
                    mutation_identity: identity,
                })
            },
            "shared-profile-analyze",
            101,
        );
        let (mut handle, mut step) = core.start(command).expect("analyze");
        let mut outcome = None;
        for _ in 0..20 {
            let CommandStep::Effects(mut effects) = step else {
                if let CommandStep::Complete(result) = step {
                    outcome = Some(*result);
                    break;
                }
                panic!("expected effect");
            };
            let request = effects.pop().expect("one effect");
            if let HostEffect::StorageMutation(batch) = &request.effect {
                for mutation in &batch.mutations {
                    if let Mutation::RetireProfileNode {
                        subject_id: retired_owner,
                        profile_node_id,
                        expected_version,
                        ..
                    } = mutation
                    {
                        assert_eq!(
                            retired_owner.as_ref(),
                            Some(&owner),
                            "retirement must preserve the existing creator"
                        );
                        assert_eq!(profile_node_id, &previous_id);
                        assert_eq!(*expected_version, 1);
                        let checkpoint =
                            core.export_checkpoint(&handle).expect("derived checkpoint");
                        core = MemoryMeshCore::new().expect("fresh Core");
                        // Restore requires the exact admitted snapshot, not the fresh instance's defaults.
                        // 恢复需要准确的准入快照，而不是新实例的默认值。
                        core.register_config(policy.clone())
                            .expect("restore profile policy");
                        let (restored, restored_step) = core
                            .restore_checkpoint(&checkpoint)
                            .expect("restore derived mutation");
                        assert_eq!(restored_step, CommandStep::Effects(vec![request.clone()]));
                        handle = restored;
                    }
                }
            }
            let mut result = host.execute_effect(request).expect("Host effect");
            if let HostEffectResult::Llm(LlmOutput::PostActionAnalysis {
                memories,
                profiles,
                context_evidence,
                ..
            }) = &mut result.result
            {
                memories.clear();
                context_evidence.clear();
                profiles[0].section_role = SectionRole::Environment;
            }
            if let HostEffectResult::Llm(LlmOutput::PostActionReview {
                accepted_profile_candidates,
                invalid_profile_candidate_indexes,
                retired_profile_nodes,
                ..
            }) = &mut result.result
            {
                if retire_only {
                    accepted_profile_candidates.clear();
                    *invalid_profile_candidate_indexes = vec![0];
                    retired_profile_nodes.push(vulcan_memorymesh_core::RetiredProfileNode {
                        node_id: previous_id.clone(),
                        reason: "obsolete environment rule".to_owned(),
                    });
                } else {
                    accepted_profile_candidates[0].supersede_nodes.push(
                        vulcan_memorymesh_core::SupersedeNodeRef {
                            node_id: previous_id.clone(),
                            reason: "updated environment rule".to_owned(),
                        },
                    );
                }
            }
            step = core
                .resume(&handle, result)
                .expect("shared environment transition");
        }
        let CommandResult::PostActionAnalysis(analysis) = outcome.expect("bounded completion")
        else {
            panic!("analysis result")
        };
        assert_eq!(analysis.retired_profile_node_ids, vec![previous_id.clone()]);
        assert_eq!(
            analysis.accepted_profile_nodes.len(),
            usize::from(!retire_only)
        );
        let dataset = host.storage.dataset();
        let previous = dataset
            .profile_nodes
            .iter()
            .find(|node| node.id == previous_id)
            .expect("prior profile");
        assert_eq!(previous.subject_id, Some(owner));
        assert_eq!(previous.state, ProfileNodeState::Inactive);
        assert_eq!(previous.version, 2);
        if retire_only {
            assert!(previous.superseded_by.is_none());
        } else {
            assert_eq!(
                previous.superseded_by.as_ref(),
                Some(&analysis.accepted_profile_nodes[0].id)
            );
            // Receipt projection and committed storage must share the admitted project formula.
            // 回执投影与已提交存储必须共用准入的项目公式。
            let node = &analysis.accepted_profile_nodes[0];
            assert_eq!(node.expires_at_ns, Some(100 + 31 * 1_000_000));
            assert_eq!(
                dataset
                    .profile_nodes
                    .iter()
                    .find(|stored| stored.id == node.id),
                Some(node)
            );
            assert_eq!(
                analysis.accepted_profile_nodes[0].subject_id,
                Some(subject_id)
            );
        }
    }
}

/// Manual profile instructions retain the admitted role-specific rule even after the active policy changes.
/// 手动画像指令在活动策略变化后仍保留准入时的角色专属规则。
#[test]
fn manual_profile_lifetime_is_role_specific_and_command_frozen() {
    for role in [SectionRole::Subject, SectionRole::Environment] {
        let core = MemoryMeshCore::new().expect("core");
        // Name the initial role policy before registering its immutable command snapshot.
        // 注册不可变命令快照之前，为初始角色策略确定名称。
        let mut policy = vulcan_memorymesh_core::ConfigBundle {
            name: "manual-profile-lifetime".to_owned(),
            ..Default::default()
        };
        policy.memory_policy.profiles.user.stable.base_ms = 17;
        policy.memory_policy.profiles.project.stable.base_ms = 31;
        core.register_config(policy.clone()).expect("initial rules");
        core.activate_config(&policy.name).expect("activate");
        let (subject_id, scope_access, _) = identities();
        let mut host = DeterministicHost::new();
        let command = with_identity(
            &core,
            |identity| {
                CoreCommand::ProfileInstruction(ProfileInstructionCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    target_section: ProfileViewSection {
                        scope_id: scope_access.write_scope.clone(),
                        role,
                        display_label: "Lifetime fixture".to_owned(),
                    },
                    instruction: "Use Rust for durable storage".to_owned(),
                    occurred_at_ns: 30,
                    mutation_identity: identity,
                })
            },
            "manual-lifetime",
            30,
        );
        let (handle, mut step) = core.start(command).expect("admit command");
        // Replacing the registered default cannot change an already admitted command's expiry formula.
        // 替换已注册默认配置不能改变已准入命令的过期公式。
        policy.memory_policy.profiles.user.stable.base_ms = 999;
        policy.memory_policy.profiles.project.stable.base_ms = 999;
        core.register_config(policy).expect("replacement rules");
        core.activate_config("manual-profile-lifetime")
            .expect("activate replacement");
        loop {
            match step {
                CommandStep::Effects(effects) => {
                    assert_eq!(effects.len(), 1);
                    let result = host
                        .execute_effect(effects.into_iter().next().expect("effect"))
                        .expect("host");
                    step = core.resume(&handle, result).expect("resume");
                }
                CommandStep::Complete(result) => {
                    let CommandResult::ProfileInstruction(result) = *result else {
                        panic!("instruction result")
                    };
                    let node = &result.accepted_nodes[0];
                    let expected_ms = match role {
                        SectionRole::Subject => 17,
                        SectionRole::Environment => 31,
                    };
                    assert_eq!(node.expires_at_ns, Some(30 + expected_ms * 1_000_000));
                    assert_eq!(
                        host.storage
                            .dataset()
                            .profile_nodes
                            .iter()
                            .find(|stored| stored.id == node.id),
                        Some(node)
                    );
                    break;
                }
            }
        }
    }
}

/// Rejects a subject-section profile fact whose durable source subject is absent.
/// 拒绝持久来源主体缺失却被宿主放入主体分区的画像事实。
#[test]
fn subject_profile_section_requires_present_matching_source_subject() {
    let core = MemoryMeshCore::new().expect("Core");
    let (subject_id, scope_access, _) = identities();
    let owner = subject_id.clone();
    let mut host = DeterministicHost::new();
    seed_profile_node(&mut host, &owner, &scope_access.write_scope, 90);
    let mut nodes = host.storage.dataset().profile_nodes;
    nodes[0].subject_id = None;
    let error = query_profile_facts(
        &core,
        subject_id,
        scope_access.clone(),
        ProfileView {
            mode: ProfileViewMode::Split,
            sections: vec![subject_profile_section(&scope_access)],
            include_explanations: false,
            display_timezone: "UTC".to_owned(),
        },
        nodes,
    )
    .expect_err("subject section must reject an absent source subject");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Seeds one active system profile node through the reference Host transaction boundary.
/// 通过参考宿主事务边界播种一条活跃系统画像节点。
fn seed_profile_node(
    host: &mut DeterministicHost,
    subject_id: &SubjectId,
    scope_id: &ScopeId,
    sequence: u64,
) -> ProfileNodeId {
    let slot = EntitySlotId::new(format!("profile-seed-slot-{sequence}"))
        .expect("profile slot identity must be valid");
    let outcome = host
        .storage
        .mutate(&MutationBatch {
            identity: MutationIdentity {
                operation_id: OperationId::new(format!("op-profile-seed-{sequence}"))
                    .expect("operation identity must be valid"),
                idempotency_key: IdempotencyKey::new(format!("profile-seed-{sequence}"))
                    .expect("idempotency identity must be valid"),
                request_fingerprint: format!("fp256:{}", "0".repeat(64)),
                source_kind: MutationSourceKind::BackgroundWorker,
                request_source_id: "profile-test".to_owned(),
                observed_at_ns: i64::try_from(sequence)
                    .expect("bounded test sequence must fit signed time"),
            },
            mutations: vec![Mutation::PutProfileNode {
                entity: EntityRef::New(slot),
                node: ProfileNode {
                    id: ProfileNodeId::new(format!("profile-placeholder-{sequence}"))
                        .expect("profile identity must be valid"),
                    subject_id: Some(subject_id.clone()),
                    scope_id: scope_id.clone(),
                    content: "stable profile fact".to_owned(),
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
                },
            }],
        })
        .expect("profile seed mutation must succeed");
    let MutationOutcome::Committed { receipt, .. } = outcome else {
        unreachable!("profile seed mutation must commit")
    };
    ProfileNodeId::new(
        receipt
            .entity_bindings
            .first()
            .expect("profile seed binding must exist")
            .domain_id
            .clone(),
    )
    .expect("bound profile identity must be valid")
}

/// Verifies all upstream batch entry points and host execution-policy metadata end to end.
/// 端到端验证全部上游批量入口与宿主执行策略元数据。
#[test]
fn upstream_batch_contracts_preserve_order_atomicity_and_effect_policy() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let mut host = DeterministicHost::new();
    let (subject_id, scope_access, vector_space_id) = identities();
    let write_command = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: SessionSourceId::new("session-batch")
                    .expect("session identity must be valid"),
                vector_space_id: vector_space_id.clone(),
                occurred_at_ns: 100,
                authorized_write_scopes: vec![scope_access.write_scope.clone()],
                items: vec![
                    MemoryWriteBatchItem {
                        target_scope_id: scope_access.write_scope.clone(),
                        scope_class: MemoryScopeClass::Environment,
                        category: MemoryCategory::ProjectContext,
                        summary: "Prefers Rust".to_owned(),
                        details: "I prefer Rust for systems work".to_owned(),
                        priority: Some(90),
                        memory_level: Some(MemoryLevel::Stable),
                        expires_at_ns: None,
                        tags: vec!["rust".to_owned()],
                    },
                    MemoryWriteBatchItem {
                        target_scope_id: scope_access.write_scope.clone(),
                        scope_class: MemoryScopeClass::Environment,
                        category: MemoryCategory::General,
                        summary: "Uses Go".to_owned(),
                        details: "I use Go for network services".to_owned(),
                        priority: Some(60),
                        memory_level: Some(MemoryLevel::Stable),
                        expires_at_ns: Some(1_000),
                        tags: vec!["go".to_owned()],
                    },
                ],
                mutation_identity: identity,
            })
        },
        "write-batch-1",
        100,
    );
    // Inspect the public first effect before driving the remainder to completion.
    // 在驱动剩余流程完成前检查公共首个 Effect。
    let (handle, step) = core.start(write_command).expect("batch write must start");
    let CommandStep::Effects(mut effects) = step else {
        assert!(false, "batch write must first request recent dedupe facts");
        return;
    };
    let dedupe_request = effects.pop().expect("dedupe request must exist");
    assert!(matches!(
        dedupe_request.effect,
        HostEffect::StorageQuery(StorageQuery::RecentDirectWriteDedupe { .. })
    ));
    assert_eq!(dedupe_request.command_id, handle.command_id);
    assert_eq!(dedupe_request.config_snapshot_id, handle.config_snapshot_id);
    assert!(dedupe_request.retryable);
    assert!(!dedupe_request.side_effect_possible);
    let dedupe_result = host
        .execute_effect(dedupe_request)
        .expect("batch dedupe query must succeed");
    let dedupe_step = core
        .resume(&handle, dedupe_result)
        .expect("batch dedupe resume must succeed");
    let mutation_request = drive_to_storage_mutation(&core, &handle, &mut host, dedupe_step);
    assert!(!mutation_request.retryable);
    assert!(mutation_request.side_effect_possible);
    let write_result = host
        .execute_effect(mutation_request)
        .and_then(|result| core.resume(&handle, result))
        .expect("batch write mutation must complete");
    let CommandStep::Complete(write_result) = write_result else {
        assert!(false, "batch write must return a final result");
        return;
    };
    let CommandResult::MemoryWriteBatch(write_result) = *write_result else {
        assert!(false, "batch write must return ordered memories");
        return;
    };
    assert_eq!(write_result.memories.len(), 2);
    assert_eq!(write_result.deduplicated, vec![false, false]);
    assert_eq!(write_result.memories[0].summary, "Prefers Rust");
    assert_eq!(write_result.memories[0].priority, 90);
    assert!(write_result.receipt.is_some());
    let written_ids = write_result
        .memories
        .iter()
        .map(|memory| memory.id.clone())
        .collect::<Vec<_>>();
    core.release_command(&handle)
        .expect("completed manual batch command must release");

    // Repeat the same normalized content to verify reuse of the host-assigned identities.
    // 重复相同规范化内容，以验证对宿主分配身份的复用。
    let reused = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: SessionSourceId::new("session-batch")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        occurred_at_ns: 101,
                        authorized_write_scopes: vec![scope_access.write_scope.clone()],
                        items: vec![
                            MemoryWriteBatchItem {
                                target_scope_id: scope_access.write_scope.clone(),
                                scope_class: MemoryScopeClass::Environment,
                                category: MemoryCategory::ProjectContext,
                                summary: "Prefers Rust".to_owned(),
                                details: "I prefer Rust for systems work".to_owned(),
                                priority: Some(90),
                                memory_level: Some(MemoryLevel::Stable),
                                expires_at_ns: None,
                                tags: vec!["rust".to_owned()],
                            },
                            MemoryWriteBatchItem {
                                target_scope_id: scope_access.write_scope.clone(),
                                scope_class: MemoryScopeClass::Environment,
                                category: MemoryCategory::General,
                                summary: "Uses Go".to_owned(),
                                details: "I use Go for network services".to_owned(),
                                priority: Some(60),
                                memory_level: Some(MemoryLevel::Stable),
                                expires_at_ns: Some(1_000),
                                tags: vec!["go".to_owned()],
                            },
                        ],
                        mutation_identity: identity,
                    })
                },
                "batch-write-dedupe",
                101,
            ),
            &mut host,
        )
        .expect("duplicate batch write must complete");
    let CommandResult::MemoryWriteBatch(reused) = reused else {
        assert!(false, "duplicate batch write must return ordered memories");
        return;
    };
    assert_eq!(reused.deduplicated, vec![true, true]);
    assert_eq!(reused.memories[0].id, written_ids[0]);
    assert_eq!(reused.memories[1].id, written_ids[1]);

    let searched = core
        .run_to_completion(
            CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                queries: vec!["Rust systems".to_owned(), "Go services".to_owned()],
                top_k: 1,
                vector_space_id: vector_space_id.clone(),
                boundary: fixture_search_boundary(&scope_access),
                now_ns: 120,
            }),
            &mut host,
        )
        .expect("grouped search must complete");
    let CommandResult::MemorySearchBatch(groups) = searched else {
        assert!(false, "grouped search must return grouped results");
        return;
    };
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].query_index, 0);
    assert_eq!(groups[1].query_index, 1);
    assert!(groups.iter().all(|group| group.hits.len() <= 1));

    let details = core
        .run_to_completion(
            CoreCommand::MemoryDetailsBatch(MemoryDetailsBatchCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                memory_ids: vec![
                    written_ids[0].clone(),
                    MemoryId::new("memory-missing").expect("memory identity must be valid"),
                ],
                turn_ids: vec![TurnId::new("turn-missing").expect("turn identity must be valid")],
            }),
            &mut host,
        )
        .expect("batch details must complete");
    let CommandResult::MemoryDetailsBatch(details) = details else {
        assert!(false, "batch details must return mixed slots");
        return;
    };
    assert!(details.memories[0].is_some());
    assert!(details.memories[1].is_none());
    assert!(details.turns[0].is_none());

    let rebuilt = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::Maintenance(MaintenanceCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        occurred_at_ns: 150,
                        action: MaintenanceAction::RebuildVectors {
                            memory_ids: written_ids.clone(),
                            vector_space_id: VectorSpaceId::new("vector-demo-v2")
                                .expect("vector identity must be valid"),
                        },
                        mutation_identity: identity,
                    })
                },
                "rebuild-batch-1",
                150,
            ),
            &mut host,
        )
        .expect("vector rebuild must complete");
    let CommandResult::Maintenance(rebuilt) = rebuilt else {
        panic!("maintenance rebuild must return its typed result");
    };
    assert_eq!(rebuilt.rebuilt_vector_count, 2);
    assert_eq!(
        rebuilt.receipt.operation_id.as_str(),
        "vmm-op-v1:e7cc7b5729581189d112ead19faed100ad9c6e4b6e3d7e82f7f69965af3d4774"
    );

    let deleted = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryDeleteBatch(MemoryDeleteBatchCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        reason: "test cleanup".to_owned(),
                        items: vec![
                            MemoryDeleteBatchItem {
                                memory_id: written_ids[0].clone(),
                                expected_version: None,
                            },
                            MemoryDeleteBatchItem {
                                memory_id: written_ids[1].clone(),
                                expected_version: None,
                            },
                            MemoryDeleteBatchItem {
                                memory_id: MemoryId::new("memory-batch-missing")
                                    .expect("memory identity must be valid"),
                                expected_version: None,
                            },
                        ],
                        mutation_identity: identity,
                    })
                },
                "delete-batch-1",
                160,
            ),
            &mut host,
        )
        .expect("batch deletion must complete");
    let CommandResult::MemoryDeleteBatch(deleted) = deleted else {
        assert!(false, "batch deletion must return ordered identities");
        return;
    };
    assert_eq!(deleted.deleted_memory_ids.len(), 2);
    assert_eq!(
        deleted.not_found_memory_ids,
        vec![MemoryId::new("memory-batch-missing").expect("memory identity must be valid")]
    );
    assert_eq!(
        deleted
            .receipt
            .as_ref()
            .expect("committed deletion batch must expose its receipt")
            .operation_id
            .as_str(),
        "vmm-op-v1:20e08009d79f7780f7d2dfadc4c49be54fa286e7bde2c7cdebaad49b223298fc"
    );
    assert!(host.storage.dataset().memories.is_empty());

    let no_op_delete = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryDeleteBatch(MemoryDeleteBatchCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        reason: "verify missing batch".to_owned(),
                        items: vec![MemoryDeleteBatchItem {
                            memory_id: MemoryId::new("memory-still-missing")
                                .expect("memory identity must be valid"),
                            expected_version: None,
                        }],
                        mutation_identity: identity,
                    })
                },
                "delete-batch-no-op",
                161,
            ),
            &mut host,
        )
        .expect("all-missing deletion batch must complete without a mutation");
    let CommandResult::MemoryDeleteBatch(no_op_delete) = no_op_delete else {
        panic!("all-missing deletion batch must return its typed result");
    };
    assert!(no_op_delete.deleted_memory_ids.is_empty());
    assert!(no_op_delete.receipt.is_none());

    let lifecycle_write = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: SubjectId::new("subject-demo")
                            .expect("subject identity must be valid"),
                        scope_access: ScopeAccess {
                            write_scope: ScopeId::new("scope-demo")
                                .expect("scope identity must be valid"),
                            visible_scopes: vec![
                                ScopeId::new("scope-demo").expect("scope identity must be valid"),
                            ],
                        },
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        item: direct_write_item(
                            ScopeId::new("scope-demo").expect("scope identity must be valid"),
                            MemoryCategory::General,
                            "maintenance lifecycle",
                            Vec::new(),
                            20,
                        ),
                        occurred_at_ns: 200,
                        mutation_identity: identity,
                    })
                },
                "write-maintenance-1",
                200,
            ),
            &mut host,
        )
        .expect("maintenance fixture write must complete");
    let CommandResult::MemoryWrite(lifecycle_write) = lifecycle_write else {
        panic!("maintenance fixture write must return its bound memory");
    };
    let lifecycle_memory_id = lifecycle_write.memory.id;
    let lifecycle_subject = SubjectId::new("subject-demo").expect("subject identity must be valid");
    let lifecycle_scope = ScopeId::new("scope-demo").expect("scope identity must be valid");
    let lifecycle_access = ScopeAccess {
        write_scope: lifecycle_scope.clone(),
        visible_scopes: vec![lifecycle_scope],
    };
    terminalize_memory(&mut host, lifecycle_memory_id.as_str(), 1);
    let first_retention = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::Retention(RetentionCommand {
                        subject_id: lifecycle_subject.clone(),
                        scope_access: lifecycle_access.clone(),
                        now_ns: 10_000,
                        policy: retention_policy(),
                        mutation_identity: identity,
                    })
                },
                "retention-recycle-1",
                10_000,
            ),
            &mut host,
        )
        .expect("maintenance fixture retention must complete");
    let CommandResult::Retention(first_plan) = first_retention else {
        assert!(false, "retention must return a typed plan set");
        return;
    };
    assert_eq!(first_plan.terminal_memory_recycle.len(), 1);
    apply_retention_recycle_plan(&mut host, &first_plan);
    let recycled = host
        .storage
        .dataset()
        .memories
        .into_iter()
        .find(|memory| memory.id == lifecycle_memory_id)
        .expect("recycled maintenance memory must exist");
    core.run_to_completion(
        with_identity(
            &core,
            |identity| {
                CoreCommand::Maintenance(MaintenanceCommand {
                    subject_id: lifecycle_subject.clone(),
                    scope_access: lifecycle_access.clone(),
                    occurred_at_ns: 10_001,
                    action: MaintenanceAction::RestoreRecycled(vec![VersionedMemoryRef {
                        memory_id: recycled.id,
                        expected_version: recycled.version,
                    }]),
                    mutation_identity: identity,
                })
            },
            "restore-1",
            10_001,
        ),
        &mut host,
    )
    .expect("memory restore must complete");
    terminalize_memory(&mut host, lifecycle_memory_id.as_str(), 2);
    let second_retention = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::Retention(RetentionCommand {
                        subject_id: lifecycle_subject.clone(),
                        scope_access: lifecycle_access.clone(),
                        now_ns: 20_000,
                        policy: retention_policy(),
                        mutation_identity: identity,
                    })
                },
                "retention-recycle-2",
                20_000,
            ),
            &mut host,
        )
        .expect("restored memory retention must complete");
    let CommandResult::Retention(second_plan) = second_retention else {
        assert!(false, "retention must return a typed plan set");
        return;
    };
    assert_eq!(second_plan.terminal_memory_recycle.len(), 1);
    apply_retention_recycle_plan(&mut host, &second_plan);
    let recycled_again = host
        .storage
        .dataset()
        .memories
        .into_iter()
        .find(|memory| memory.id == lifecycle_memory_id)
        .expect("recycled maintenance memory must exist again");
    core.run_to_completion(
        with_identity(
            &core,
            |identity| {
                CoreCommand::Maintenance(MaintenanceCommand {
                    subject_id: lifecycle_subject,
                    scope_access: lifecycle_access,
                    occurred_at_ns: 20_001,
                    action: MaintenanceAction::PurgeRecycled(vec![VersionedMemoryRef {
                        memory_id: recycled_again.id,
                        expected_version: recycled_again.version,
                    }]),
                    mutation_identity: identity,
                })
            },
            "purge-1",
            20_001,
        ),
        &mut host,
    )
    .expect("memory purge must complete");
    assert!(host.storage.dataset().memories.is_empty());
}

impl DeterministicHost {
    /// Creates an empty deterministic host.
    /// 创建空确定性宿主。
    #[must_use]
    fn new() -> Self {
        Self {
            storage: InMemoryStorage::new(),
            uncertain_next_mutation: false,
            fail_after_committed_mutation: false,
            fail_lexical: false,
            fail_precheck_review: false,
            append_foreign_precheck_identity: false,
            inject_foreign_postaction_reference: false,
            complete_postaction_target_before_analysis: false,
            inject_invalid_memory_metadata: false,
            inject_future_context_evidence: false,
            inject_expired_profile_fact: false,
            inject_invalid_profile_source: false,
            depress_candidate_scores: false,
            suppress_candidate_queries: false,
            out_of_range_rerank_scores: false,
            scenario: HostScenario::Default,
            review_llm_calls: 0,
            max_review_similar_count: 0,
        }
    }

    /// Executes one provider-agnostic LLM request using fixed scenario semantics.
    /// 使用固定场景语义执行一次与 Provider 无关的 LLM 请求。
    fn execute_llm(&mut self, effect: LlmEffect) -> Result<LlmOutput, CoreError> {
        match (effect.purpose, effect.input) {
            (LlmPurpose::PreCheckIntent, LlmInput::PreCheckIntent { user_content, .. }) => {
                // Classify retrospective or preference questions as requiring memory.
                // 将回顾性问题或偏好问题分类为需要记忆。
                let lowercase = user_content.to_lowercase();
                // Preserve one complete natural-language query.
                // 保留一条完整自然语言查询。
                let need_memory = lowercase.contains("remember")
                    || lowercase.contains("prefer")
                    || user_content.contains("记得");
                Ok(LlmOutput::PreCheckIntent {
                    need_memory,
                    queries: need_memory.then_some(user_content).into_iter().collect(),
                    reason: "deterministic intent classification".to_owned(),
                })
            }
            (LlmPurpose::PreCheckReview, LlmInput::PreCheckReview { candidates, .. }) => {
                let mut accepted_memory_ids = candidates
                    .into_iter()
                    .map(|hit| hit.memory_id)
                    .collect::<Vec<_>>();
                if self.append_foreign_precheck_identity {
                    accepted_memory_ids.push(
                        MemoryId::new("memory-foreign-review")
                            .expect("foreign memory identity must be valid"),
                    );
                }
                Ok(LlmOutput::PreCheckReview {
                    accepted_memory_ids,
                })
            }
            (LlmPurpose::PostActionAnalysis, LlmInput::PostActionAnalysis { target_turn, .. }) => {
                // Find the first exact user item as the evidence-bearing source.
                // 查找第一个精确用户项目作为证据来源。
                let user_content = target_turn
                    .timeline
                    .iter()
                    .find(|item| item.role == Role::User)
                    .map(|item| item.content.clone())
                    .unwrap_or_default();
                let turn_id = target_turn.id.clone();
                if self.scenario == HostScenario::PrioritySupersede {
                    return Ok(LlmOutput::PostActionAnalysis {
                        user_input_kind: TurnInputKind::Statement,
                        turn_id,
                        details: "deterministic priority analysis".to_owned(),
                        memories: vec![MemoryCandidate {
                            category: MemoryCategory::ProjectContext,
                            summary: "Prefers Rust".to_owned(),
                            content: format!("User preference: {user_content}"),
                            evidence_source: "user_asserted".to_owned(),
                            admission: Admission::Accept,
                            admission_reason: None,
                        }],
                        profiles: vec![ProfileCandidate {
                            section_role: SectionRole::Subject,
                            content: "Preferred language: Rust".to_owned(),
                            evidence_source: "user_asserted".to_owned(),
                            admission: Admission::Accept,
                            admission_reason: None,
                        }],
                        context_evidence: Vec::new(),
                    });
                }
                if self.scenario == HostScenario::SparseAcceptance {
                    // Emit a dropped first candidate plus two admitted candidates, with one
                    // evidence row per candidate so sparse acceptance is observable.
                    // 产出首个被丢弃候选与两个已准入候选，每个候选一条证据行，
                    // 使稀疏接受可被观测。
                    return Ok(LlmOutput::PostActionAnalysis {
                        user_input_kind: TurnInputKind::Statement,
                        turn_id,
                        details: "deterministic sparse analysis".to_owned(),
                        memories: vec![
                            MemoryCandidate {
                                category: MemoryCategory::General,
                                summary: "dropped".to_owned(),
                                content: "first candidate is dropped".to_owned(),
                                evidence_source: "user_asserted".to_owned(),
                                admission: Admission::Drop,
                                admission_reason: Some("non_durable".to_owned()),
                            },
                            MemoryCandidate {
                                category: MemoryCategory::ProjectContext,
                                summary: "Prefers Rust".to_owned(),
                                content: format!("User preference: {user_content}"),
                                evidence_source: "user_asserted".to_owned(),
                                admission: Admission::Accept,
                                admission_reason: None,
                            },
                            MemoryCandidate {
                                category: MemoryCategory::General,
                                summary: "Lives in Beijing".to_owned(),
                                content: format!("User fact: {user_content}"),
                                evidence_source: "user_asserted".to_owned(),
                                admission: Admission::Accept,
                                admission_reason: None,
                            },
                        ],
                        profiles: Vec::new(),
                        context_evidence: vec![
                            ContextEvidenceCandidate {
                                candidate_index: 0,
                                context_key: "dropped".to_owned(),
                                context_value: "filtered".to_owned(),
                                relation: ContextEvidenceRelation::Support,
                            },
                            ContextEvidenceCandidate {
                                candidate_index: 1,
                                context_key: "language".to_owned(),
                                context_value: "Rust".to_owned(),
                                relation: ContextEvidenceRelation::Support,
                            },
                            ContextEvidenceCandidate {
                                candidate_index: 2,
                                context_key: "city".to_owned(),
                                context_value: "Beijing".to_owned(),
                                relation: ContextEvidenceRelation::Support,
                            },
                        ],
                    });
                }
                Ok(LlmOutput::PostActionAnalysis {
                    user_input_kind: TurnInputKind::Statement,
                    turn_id,
                    details: "deterministic default analysis".to_owned(),
                    memories: vec![MemoryCandidate {
                        category: MemoryCategory::ProjectContext,
                        summary: "Prefers Rust".to_owned(),
                        content: format!("User preference: {user_content}"),
                        evidence_source: "user_asserted".to_owned(),
                        admission: Admission::Accept,
                        admission_reason: None,
                    }],
                    profiles: vec![ProfileCandidate {
                        section_role: SectionRole::Subject,
                        content: "Preferred language: Rust".to_owned(),
                        evidence_source: "user_asserted".to_owned(),
                        admission: Admission::Accept,
                        admission_reason: None,
                    }],
                    context_evidence: Vec::new(),
                })
            }
            (
                LlmPurpose::PostActionReview,
                LlmInput::PostActionReview {
                    memory_candidates,
                    profile_candidates,
                    ..
                },
            ) => {
                self.review_llm_calls += 1;
                self.max_review_similar_count = self.max_review_similar_count.max(
                    memory_candidates
                        .iter()
                        .map(|candidate| candidate.similar_memories.len())
                        .max()
                        .unwrap_or(0),
                );
                if self.scenario == HostScenario::SparseAcceptance {
                    return Ok(LlmOutput::PostActionReview {
                        accepted_candidates: memory_candidates
                            .iter()
                            .enumerate()
                            .filter(|(_, candidate)| {
                                candidate.candidate.category == MemoryCategory::General
                            })
                            .map(|(index, _)| AcceptedCandidate {
                                candidate_index: u32::try_from(index).unwrap_or(u32::MAX),
                                supersede_memory_ids: Vec::new(),
                            })
                            .collect(),
                        dropped_candidates: memory_candidates
                            .iter()
                            .enumerate()
                            .filter(|(_, candidate)| {
                                candidate.candidate.category == MemoryCategory::ProjectContext
                            })
                            .map(
                                |(index, candidate)| vulcan_memorymesh_core::DroppedCandidate {
                                    candidate_index: u32::try_from(index).unwrap_or(u32::MAX),
                                    dedupe_memory_id: Some(
                                        candidate
                                            .similar_memories
                                            .first()
                                            .map(|hit| hit.memory.id.to_string())
                                            .unwrap_or_default(),
                                    ),
                                    reason: Some("sparse deterministic drop".to_owned()),
                                },
                            )
                            .collect(),
                        accepted_profile_candidates: profile_candidates
                            .iter()
                            .enumerate()
                            .map(|(index, candidate)| AcceptedProfileCandidate {
                                candidate_index: u32::try_from(index).unwrap_or(u32::MAX),
                                normalized_content: candidate.content.clone(),
                                priority: 60,
                                profile_level: ProfileLevel::Stable,
                                level_reason: None,
                                supersede_nodes: Vec::new(),
                            })
                            .collect(),
                        invalid_profile_candidate_indexes: Vec::new(),
                        retired_profile_nodes: Vec::new(),
                        reason: Some("sparse deterministic review".to_owned()),
                    });
                }
                Ok(LlmOutput::PostActionReview {
                    accepted_candidates: memory_candidates
                        .iter()
                        .enumerate()
                        .map(|(index, candidate)| AcceptedCandidate {
                            candidate_index: u32::try_from(index).unwrap_or(u32::MAX),
                            supersede_memory_ids: if self.scenario
                                == HostScenario::PrioritySupersede
                                && index == 0
                            {
                                candidate
                                    .similar_memories
                                    .first()
                                    .map(|hit| hit.memory.id.to_string())
                                    .into_iter()
                                    .collect()
                            } else {
                                Vec::new()
                            },
                        })
                        .collect(),
                    dropped_candidates: Vec::new(),
                    accepted_profile_candidates: profile_candidates
                        .iter()
                        .enumerate()
                        .map(|(index, candidate)| AcceptedProfileCandidate {
                            candidate_index: u32::try_from(index).unwrap_or(u32::MAX),
                            normalized_content: candidate.content.clone(),
                            priority: 60,
                            profile_level: ProfileLevel::Stable,
                            level_reason: None,
                            supersede_nodes: Vec::new(),
                        })
                        .collect(),
                    invalid_profile_candidate_indexes: Vec::new(),
                    retired_profile_nodes: Vec::new(),
                    reason: Some("deterministic test review".to_owned()),
                })
            }
            (
                LlmPurpose::ProfileInstruction,
                LlmInput::ProfileInstruction {
                    instruction,
                    current_nodes,
                    ..
                },
            ) => Ok(LlmOutput::ProfileInstruction {
                accepted_nodes: vec![AcceptedProfileNode {
                    content: instruction,
                    priority: 60,
                    profile_level: ProfileLevel::Stable,
                    level_reason: None,
                    supersede_nodes: Vec::new(),
                }],
                retired_nodes: current_nodes
                    .first()
                    .map(|node| vulcan_memorymesh_core::RetiredProfileNode {
                        node_id: node.id.clone(),
                        reason: "replaced-by-instruction".to_owned(),
                    })
                    .into_iter()
                    .collect(),
                review_reason: "deterministic test review".to_owned(),
            }),
            _ => Err(CoreError::new(
                ErrorCode::EffectMismatch,
                ErrorStage::Effect,
                "deterministic host received mismatched LLM purpose and input",
            )),
        }
    }

    /// Generates one stable finite embedding with the exact requested dimension per input.
    /// 为每个输入生成一个维度与请求严格一致的稳定有限嵌入。
    fn execute_embedding(effect: &EmbeddingEffect) -> Vec<Vec<f32>> {
        let expected_dimension = usize::try_from(effect.expected_dimension)
            .expect("validated embedding dimension must fit usize");
        effect
            .inputs
            .iter()
            .map(|input| {
                // Normalize ASCII case once for semantic feature extraction.
                // 为语义特征提取仅规范化一次 ASCII 大小写。
                let lowercase = input.to_lowercase();
                let semantic_features = [
                    f32::from(lowercase.contains("rust")),
                    f32::from(lowercase.contains("prefer") || input.contains("喜欢")),
                    f32::from(
                        u8::try_from(input.chars().count().min(100))
                            .expect("bounded character count must fit u8"),
                    ) / 100.0,
                    // Give this fixture's acknowledgement its own direction instead of
                    // making every non-Rust/non-preference sentence collinear with denials.
                    // 为此夹具的确认回复保留独立方向，避免所有无 Rust/偏好词的句子
                    // 都与拒绝原型共线；真实语义边界由 noise_intake 专项验证。
                    f32::from(input == "已记录偏好"),
                ];
                // Preserve the original semantic features and zero-pad the host result to the contract dimension.
                // 保留原始语义特征，并用零填充宿主结果直至契约维度。
                let mut vector = vec![0.0; expected_dimension];
                let feature_count = semantic_features.len().min(expected_dimension);
                vector[..feature_count].copy_from_slice(&semantic_features[..feature_count]);
                vector
            })
            .collect()
    }
}

impl HostDriver for DeterministicHost {
    /// Executes exactly one typed effect and returns its matching identity.
    /// 执行恰好一个强类型 Effect 并返回其匹配标识。
    fn execute_effect(&mut self, request: EffectRequest) -> Result<EffectResult, CoreError> {
        // Preserve the request identity before consuming its payload.
        // 在消耗请求载荷前保留请求标识。
        let effect_id = request.effect_id;
        // Execute the exact typed host capability.
        // 执行精确强类型宿主能力。
        let result = match request.effect {
            HostEffect::Llm(effect) => {
                if self.fail_precheck_review && effect.purpose == LlmPurpose::PreCheckReview {
                    HostEffectResult::Failure(EffectFailure {
                        kind: EffectFailureKind::Llm,
                        class: FailureClass::Provider,
                        retryable: false,
                        allow_degrade: true,
                        diagnostic: "injected PreCheck review failure".to_owned(),
                    })
                } else {
                    HostEffectResult::Llm(self.execute_llm(effect)?)
                }
            }
            HostEffect::Embedding(effect) => {
                HostEffectResult::Embedding(Self::execute_embedding(&effect).into())
            }
            HostEffect::Rerank(effect) => {
                if self.out_of_range_rerank_scores {
                    return Ok(EffectResult {
                        effect_id,
                        result: HostEffectResult::Rerank(
                            (0..effect.documents.len())
                                .map(|index| vulcan_memorymesh_core::RerankEntry {
                                    input_index: index as u32,
                                    score: if index == 0 { 1.1 } else { -0.1 },
                                })
                                .collect(),
                        ),
                    });
                }
                // Score every document by normalized token overlap with the exact query.
                // 按与精确查询的规范化词元重合度为每个文档评分。
                let query = effect.query.to_lowercase();
                // Generate one score in exact document order.
                // 按精确文档顺序生成一个评分。
                let scores = effect
                    .documents
                    .iter()
                    .enumerate()
                    .map(|(index, document)| {
                        let document = document.to_lowercase();
                        let score = if query
                            .split_whitespace()
                            .any(|token| !token.is_empty() && document.contains(token))
                        {
                            1.0
                        } else {
                            0.5
                        };
                        vulcan_memorymesh_core::RerankEntry {
                            input_index: index as u32,
                            score,
                        }
                    })
                    .collect();
                HostEffectResult::Rerank(scores)
            }
            HostEffect::StorageQuery(query) => {
                if self.suppress_candidate_queries
                    && matches!(
                        &query,
                        StorageQuery::VectorCandidates { .. }
                            | StorageQuery::LexicalCandidates { .. }
                    )
                {
                    HostEffectResult::StorageQuery(match query {
                        StorageQuery::VectorCandidates { .. } => {
                            StorageQueryResult::VectorCandidates(CandidateChannelResult {
                                candidates: Vec::new(),
                                memories: Vec::new(),
                            })
                        }
                        StorageQuery::LexicalCandidates { .. } => {
                            StorageQueryResult::LexicalCandidates(CandidateChannelResult {
                                candidates: Vec::new(),
                                memories: Vec::new(),
                            })
                        }
                        _ => {
                            return Err(CoreError::new(
                                ErrorCode::EffectMismatch,
                                ErrorStage::Effect,
                                "candidate suppression received a non-candidate storage query",
                            ));
                        }
                    })
                } else if self.fail_lexical
                    && matches!(&query, StorageQuery::LexicalCandidates { .. })
                {
                    HostEffectResult::Failure(EffectFailure {
                        kind: EffectFailureKind::StorageQuery,
                        class: FailureClass::Provider,
                        retryable: false,
                        allow_degrade: true,
                        diagnostic: "injected lexical failure".to_owned(),
                    })
                } else {
                    let depress_candidate_scores = self.depress_candidate_scores
                        && matches!(
                            &query,
                            StorageQuery::VectorCandidates { .. }
                                | StorageQuery::LexicalCandidates { .. }
                        );
                    let mut query_result = self.storage.query(&query)?;
                    if self.inject_foreign_postaction_reference
                        && let StorageQueryResult::PostActionContext {
                            target_turn,
                            reference_turns,
                            ..
                        } = &mut query_result
                    {
                        reference_turns.push(PostActionReferenceTurn {
                            turn_id: TurnId::new("turn-foreign-reference")
                                .expect("foreign reference identity must be valid"),
                            subject_id: SubjectId::new("subject-foreign-reference")
                                .expect("foreign subject identity must be valid"),
                            scope_id: target_turn.scope_id.clone(),
                            session_source_id: target_turn.session_source_id.clone(),
                            details: "foreign refined details".to_owned(),
                            created_at_ns: target_turn.created_at_ns.saturating_sub(1),
                        });
                    }
                    if self.complete_postaction_target_before_analysis
                        && let StorageQueryResult::PostActionContext { target_turn, .. } =
                            &mut query_result
                    {
                        target_turn.status = vulcan_memorymesh_core::TurnStatus::Completed;
                        target_turn.user_input_kind = Some(TurnInputKind::Statement);
                        target_turn.analysis_details = Some("corrupted prior analysis".to_owned());
                    }
                    if self.inject_invalid_memory_metadata
                        && let StorageQueryResult::MemoryDetails(memory) = &mut query_result
                        && let Some(memory) = memory.as_mut()
                    {
                        memory.status_reason = Some("invalid\0hidden".to_owned());
                    }
                    if self.inject_future_context_evidence
                        && let StorageQueryResult::ContextEvidenceBatch(groups) = &mut query_result
                        && let Some(group) = groups.first_mut()
                    {
                        group.evidence.push(MemoryContextEvidence {
                            memory_id: group.memory_id.clone(),
                            context_key: "environment".to_owned(),
                            context_value: "future".to_owned(),
                            support_count: 1,
                            rebuttal_count: 0,
                            last_supported_at_ns: Some(i64::MAX),
                            last_rebutted_at_ns: None,
                            version: 1,
                        });
                    }
                    if self.inject_expired_profile_fact {
                        let evaluated_at_ns = match &query {
                            StorageQuery::ProfileNodes {
                                evaluated_at_ns, ..
                            } => Some(*evaluated_at_ns),
                            _ => None,
                        };
                        if let (Some(evaluated_at_ns), StorageQueryResult::ProfileNodes(nodes)) =
                            (evaluated_at_ns, &mut query_result)
                            && let Some(template) = nodes.first().cloned()
                        {
                            let mut expired = template;
                            expired.id = ProfileNodeId::new("profile-expired-host-fact")
                                .expect("profile identity must be valid");
                            expired.effective_date_ns = Some(evaluated_at_ns.saturating_sub(1));
                            expired.expires_at_ns = Some(evaluated_at_ns);
                            nodes.push(expired);
                        }
                    }
                    if self.inject_invalid_profile_source
                        && let StorageQueryResult::ProfileNodes(nodes) = &mut query_result
                        && let Some(node) = nodes.first_mut()
                    {
                        node.source_kind = ProfileSourceKind::ManualInstruction;
                        node.source_id = None;
                    }
                    if depress_candidate_scores {
                        match &mut query_result {
                            StorageQueryResult::VectorCandidates(candidates)
                            | StorageQueryResult::LexicalCandidates(candidates) => {
                                for candidate in &mut candidates.candidates {
                                    candidate.score.raw_value = 0.0;
                                    candidate.score.decision_score = Some(0.0);
                                }
                            }
                            _ => {}
                        }
                    }
                    HostEffectResult::StorageQuery(query_result)
                }
            }
            HostEffect::StorageMutation(batch) => {
                if self.uncertain_next_mutation {
                    self.uncertain_next_mutation = false;
                    HostEffectResult::StorageMutation(MutationOutcome::Uncertain)
                } else if self.fail_after_committed_mutation {
                    self.fail_after_committed_mutation = false;
                    let outcome = self.storage.mutate_at(&batch, i64::MAX)?;
                    assert!(matches!(outcome, MutationOutcome::Committed { .. }));
                    return Err(CoreError::new(
                        ErrorCode::Internal,
                        ErrorStage::Effect,
                        "test Host lost the committed mutation acknowledgement",
                    ));
                } else {
                    // This deterministic test host commits after every fixture's observation cutoff.
                    // 此确定性测试宿主在所有样本观察截止之后提交。
                    HostEffectResult::StorageMutation(self.storage.mutate_at(&batch, i64::MAX)?)
                }
            }
        };
        Ok(EffectResult { effect_id, result })
    }
}

/// Creates the shared subject, scope, and vector identities used by public call tests.
/// 创建公共调用测试使用的共享主体、作用域与向量标识。
fn identities() -> (SubjectId, ScopeAccess, VectorSpaceId) {
    // Create the exact subject identity.
    // 创建精确主体标识。
    let subject_id = SubjectId::new("subject-demo").expect("subject identity must be valid");
    // Create the exact write scope identity.
    // 创建精确写入作用域标识。
    let scope_id = ScopeId::new("scope-demo").expect("scope identity must be valid");
    // Build explicit read and write scope access.
    // 构建显式读写作用域访问。
    let scope_access = ScopeAccess {
        write_scope: scope_id.clone(),
        visible_scopes: vec![scope_id],
    };
    // Create the exact embedding vector space identity.
    // 创建精确嵌入向量空间标识。
    let vector_space_id =
        VectorSpaceId::new("vector-demo-v1").expect("vector identity must be valid");
    (subject_id, scope_access, vector_space_id)
}

/// Verifies direct writes reject unbounded tag collections before requesting Host work.
/// 验证主动写入会在请求宿主工作前拒绝无界标签集合。
#[test]
fn direct_write_rejects_oversized_tag_collections() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let tags = (0..=vulcan_memorymesh_core_contracts::MAX_MEMORY_TAGS)
        .map(|index| format!("tag-{index}"))
        .collect();
    let error = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id,
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id,
                    item: direct_write_item(
                        scope_access.write_scope,
                        MemoryCategory::ProjectContext,
                        "bounded memory",
                        tags,
                        20,
                    ),
                    occurred_at_ns: 10,
                    mutation_identity: identity,
                })
            },
            "write:oversized-tags",
            10,
        ))
        .expect_err("oversized direct-write tags must fail validation");
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert_eq!(error.stage, ErrorStage::Validation);
}

/// Verifies every public functional command through complete typed host calls.
/// 通过完整强类型宿主调用验证每个公共功能命令。
#[test]
fn public_facade_runs_the_complete_function_set() {
    // Create Core and validate all embedded resources.
    // 创建 Core 并校验全部嵌入资源。
    let core = MemoryMeshCore::new().expect("Core must initialize");
    // Custom user duration exercises automatic and manual profile persistence through public commands.
    // 自定义用户期限通过公开命令验证自动与手动画像持久化。
    let mut policy = vulcan_memorymesh_core::ConfigBundle {
        name: "user-profile-lifetime".to_owned(),
        ..Default::default()
    };
    policy.memory_policy.profiles.user.stable.base_ms = 17;
    policy.memory_policy.profiles.user.stable.step_ms = 0;
    policy.memory_policy.profiles.project.stable.base_ms = 31;
    core.register_config(policy).expect("profile policy");
    core.activate_config("user-profile-lifetime")
        .expect("activate profile policy");
    // Create an empty deterministic host.
    // 创建空确定性宿主。
    let mut host = DeterministicHost::new();
    // Create shared domain identities.
    // 创建共享领域身份。
    let (subject_id, scope_access, vector_space_id) = identities();
    // Write one direct memory through embedding and atomic storage effects.
    // 通过嵌入与原子存储 Effect 写入一条直接记忆。
    let written = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::ProjectContext,
                            "I prefer Rust for systems work",
                            vec!["Rust".to_owned(), "rust".to_owned()],
                            20,
                        ),
                        occurred_at_ns: 10,
                        mutation_identity: identity,
                    })
                },
                "write-direct-1",
                10,
            ),
            &mut host,
        )
        .expect("direct memory write must complete");
    let direct_memory_id = match written {
        CommandResult::MemoryWrite(result) => result.memory.id,
        _ => panic!("direct memory write must return its bound record"),
    };
    // Search through embedding, semantic storage, and rerank effects.
    // 通过嵌入、语义存储与重排 Effect 搜索。
    let searched = core
        .run_to_completion(
            CoreCommand::MemorySearch(MemorySearchCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                query: "What do I prefer for systems work?".to_owned(),
                limit: 1,
                vector_space_id: vector_space_id.clone(),
                boundary: fixture_search_boundary(&scope_access),
                now_ns: 15,
            }),
            &mut host,
        )
        .expect("memory search must complete");
    // Verify the public limit survives candidate expansion and reranking.
    // 验证公共限制在候选扩展与重排后仍然生效。
    let CommandResult::MemorySearch(hits) = searched else {
        assert!(false, "memory search must return search hits");
        return;
    };
    assert_eq!(hits.len(), 1);
    // Read exact memory details through subject and scope isolation.
    // 通过主体与作用域隔离读取精确记忆详情。
    let details = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                memory_id: direct_memory_id.clone(),
            }),
            &mut host,
        )
        .expect("memory details must complete");
    assert!(matches!(details, CommandResult::MemoryDetails(Some(_))));
    // Run the full PreCheck L1, embedding, search, rerank, and L2 chain.
    // 运行完整 PreCheck L1、嵌入、搜索、重排与 L2 链路。
    let precheck = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PreCheck(PreCheckCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-demo")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        candidate_scope: fixture_candidate_scope(&scope_access),
                        user_content: "What do I prefer for systems work?".to_owned(),
                        recent_turns: Vec::new(),
                        recall_mode: RecallMode::Auto,
                        session_recall_mode: SessionRecallMode::CompactAware,
                        max_search_queries: 3,
                        max_memories: 3,
                        now_ns: 16,
                        mutation_identity: identity,
                    })
                },
                "precheck-1",
                16,
            ),
            &mut host,
        )
        .expect("PreCheck must complete");
    let CommandResult::PreCheck(precheck) = precheck else {
        assert!(false, "PreCheck must return its typed result");
        return;
    };
    assert!(precheck.should_inject);
    assert!(precheck.context_text.contains("Rust"));
    assert_eq!(precheck.queries, vec!["What do I prefer for systems work?"]);
    assert_eq!(
        precheck
            .reinforcement
            .as_ref()
            .and_then(|value| value.receipt.as_ref())
            .expect("successful reinforcement must expose its receipt")
            .operation_id
            .as_str(),
        "vmm-op-v1:c7de2045de97d963296825068492cb8a35d6910dc7c19a047cf9c5b58ccf8b26"
    );
    // Run the durable PostAction intake and the asynchronous analysis chain.
    // 运行持久 PostAction 登记与异步分析链路。
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-demo")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![
                            TimelineItem {
                                role: Role::User,
                                content: "我下周要出差去北京，我喜欢提前订票，手机号 13800138000"
                                    .to_owned(),
                                occurred_at_ns: 20,
                            },
                            TimelineItem {
                                role: Role::Assistant,
                                content: "已记录偏好".to_owned(),
                                occurred_at_ns: 21,
                            },
                        ],
                        completed_at_ns: 21,
                        mutation_identity: identity,
                    })
                },
                "intake-1",
                21,
            ),
            &mut host,
        )
        .expect("PostAction intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        assert!(false, "PostAction intake must return its typed result");
        return;
    };
    assert!(!intake.ignored_as_noise);
    assert!(intake.receipt.is_some());
    let pending_turn = intake.turn_id.expect("intake must persist a pending turn");
    let analysis = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: SessionSourceId::new("session-demo")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        turn_id: pending_turn.clone(),
                        analysis_generation: 1,
                        analysis_cutoff_ns: 22,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "analyze-1",
                22,
            ),
            &mut host,
        )
        .expect("PostAction analysis must complete");
    let CommandResult::PostActionAnalysis(analysis) = analysis else {
        assert!(false, "PostAction analysis must return its typed result");
        return;
    };
    assert_eq!(analysis.accepted_memory_ids.len(), 1);
    assert_eq!(analysis.accepted_profile_nodes.len(), 1);
    assert_eq!(
        analysis.accepted_profile_nodes[0].effective_date_ns,
        Some(21),
        "the profile business date must remain anchored to the source turn"
    );
    assert_eq!(
        analysis.accepted_profile_nodes[0].expires_at_ns,
        Some(21 + 17 * 1_000_000)
    );
    assert_eq!(
        analysis.accepted_profile_nodes[0].updated_at_ns, 22,
        "the analysis cutoff must deterministically represent the VMM profile commit time"
    );
    assert_eq!(analysis.accepted_profile_nodes.len(), 1);
    assert!(analysis.receipt.is_some());
    // Query the profile bundle created by PostAction.
    // 查询 PostAction 创建的画像包。
    let profile = core
        .run_to_completion(
            CoreCommand::ProfileQuery(ProfileQueryCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                view: ProfileView {
                    mode: ProfileViewMode::Full,
                    sections: vec![ProfileViewSection {
                        scope_id: ScopeId::new("scope-demo").expect("scope identity must be valid"),
                        role: SectionRole::Subject,
                        display_label: "Subject".to_owned(),
                    }],
                    include_explanations: false,
                    display_timezone: "UTC".to_owned(),
                },
                evaluated_at_ns: 22,
            }),
            &mut host,
        )
        .expect("profile query must complete");
    assert!(matches!(profile, CommandResult::ProfileQuery(_)));
    // Apply one manual profile instruction through current-node query, LLM review, and mutation.
    // 通过当前节点查询、LLM 审核与变更应用一条手动画像指令。
    let instruction = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::ProfileInstruction(ProfileInstructionCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        target_section: subject_profile_section(&scope_access),
                        instruction: "优先使用中文回答".to_owned(),
                        occurred_at_ns: 30,
                        mutation_identity: identity,
                    })
                },
                "instruction-1",
                30,
            ),
            &mut host,
        )
        .expect("profile instruction must complete");
    let CommandResult::ProfileInstruction(instruction) = instruction else {
        assert!(
            false,
            "profile instruction must return its reviewed outcome"
        );
        return;
    };
    assert_eq!(instruction.accepted_nodes.len(), 1);
    assert_eq!(
        instruction.accepted_nodes[0].expires_at_ns,
        Some(30 + 17 * 1_000_000)
    );
    assert_eq!(
        host.storage
            .dataset()
            .profile_nodes
            .iter()
            .find(|node| node.id == instruction.accepted_nodes[0].id),
        Some(&instruction.accepted_nodes[0])
    );
    assert_eq!(instruction.retired_nodes.len(), 1);
    assert!(!instruction.review_reason.is_empty());
    assert!(instruction.receipt.is_some());
    // Advance the latest persisted turn as the durable session compact boundary.
    // 将最新持久化轮次推进为持久会话压缩边界。
    let compacted = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::ChatCompact(ChatCompactCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-demo")
                            .expect("session identity must be valid"),
                        compacted_at_ns: 40,
                        mutation_identity: identity,
                    })
                },
                "compact-1",
                40,
            ),
            &mut host,
        )
        .expect("ChatCompact must complete");
    let CommandResult::ChatCompact(compacted) = compacted else {
        assert!(false, "ChatCompact must return its typed result");
        return;
    };
    assert!(compacted.accepted);
    assert!(compacted.updated);
    assert_eq!(compacted.compacted_turn_id, Some(pending_turn));
    assert_eq!(
        compacted.receipt.operation_id.as_str(),
        "vmm-op-v1:1d5601045c75c14bc8c2224d11c5d422612452b87040d598ad38502f4dbce3f7"
    );
    // Apply retention only inside the exact write scope.
    // 仅在精确写入作用域内应用保留策略。
    let retained = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::Retention(RetentionCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        now_ns: 10_000,
                        policy: retention_policy(),
                        mutation_identity: identity,
                    })
                },
                "retention-1",
                10_000,
            ),
            &mut host,
        )
        .expect("Retention must complete");
    assert!(matches!(
        retained,
        CommandResult::Retention(plans) if plans.terminal_memory_recycle.is_empty()
    ));
    // Active memories must survive retention and remain available for explicit deletion.
    // 活跃记忆必须在保留治理后继续存在，并可供显式删除。
    let current = host
        .storage
        .dataset()
        .memories
        .into_iter()
        .find(|memory| memory.id == direct_memory_id)
        .expect("active direct memory must still exist");
    // Execute scoped deletion with optimistic concurrency.
    // 使用乐观并发执行有作用域删除。
    let deleted = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryDelete(MemoryDeleteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        memory_id: current.id,
                        expected_version: Some(current.version),
                        mutation_identity: identity,
                    })
                },
                "delete-1",
                10_001,
            ),
            &mut host,
        )
        .expect("memory deletion must complete");
    let CommandResult::MemoryDelete(deleted) = deleted else {
        panic!("memory deletion must return its typed result");
    };
    assert!(deleted.deleted);
    assert_eq!(
        deleted.receipt.operation_id.as_str(),
        "vmm-op-v1:22eb9951b2bdf967ee0cc34a2fa9704bf604873d4396d0ebc7e20f3b7a54456b"
    );
    // Verify public local PII and Noise helpers use the same active snapshot.
    // 验证公共本地 PII 与 Noise 辅助方法使用同一活跃快照。
    let scrubbed = core
        .scrub_pii("电话 13800138000")
        .expect("PII scrub must complete");
    assert_eq!(scrubbed.text, "电话 [MOBILE_MASKED]");
    // Detect one authoritative Chinese meta-question category.
    // 检测一个权威中文元问题类别。
    let noise = core
        .detect_noise(Role::User, "你还记得我上次说的吗？")
        .expect("Noise detection must complete");
    assert!(noise.is_noise);
}

/// Verifies cancellation, effect mismatch recovery, and uncertain-write replay protection.
/// 验证取消、Effect 不匹配恢复与不确定写入重放保护。
#[test]
fn lifecycle_guards_are_explicit_and_replay_safe() {
    // Create Core and a deterministic host.
    // 创建 Core 与确定性宿主。
    let core = MemoryMeshCore::new().expect("Core must initialize");
    // Create shared domain identities.
    // 创建共享领域身份。
    let (subject_id, scope_access, vector_space_id) = identities();
    // Start one ChatCompact command without executing its effect.
    // 启动一个 ChatCompact 命令但不执行其 Effect。
    let (cancel_handle, cancel_step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::ChatCompact(ChatCompactCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: SessionSourceId::new("session-cancel")
                        .expect("session identity must be valid"),
                    compacted_at_ns: 1,
                    mutation_identity: identity,
                })
            },
            "compact:cancel",
            1,
        ))
        .expect("ChatCompact start must succeed");
    // Preserve the pending request for post-cancel rejection verification.
    // 保留待处理请求以验证取消后的拒绝。
    let CommandStep::Effects(mut cancel_effects) = cancel_step else {
        assert!(false, "ChatCompact start must return an effect");
        return;
    };
    // Extract the exact pending effect.
    // 提取精确待处理 Effect。
    let cancel_request = cancel_effects.pop().expect("pending effect must exist");
    let late_outcome = committed_outcome(
        &cancel_request,
        1,
        Some(ChatCompactMutationResult {
            accepted: true,
            updated: false,
            compacted_turn_id: None,
        }),
    );
    core.cancel(&cancel_handle)
        .expect("waiting command must cancel");
    assert_eq!(
        core.command_diagnostics(&cancel_handle)
            .expect("cancelled diagnostics must exist")
            .status,
        CommandStatus::Cancelled
    );
    core.release_command(&cancel_handle)
        .expect("cancelled command must release");
    // Return a validly shaped late result after explicit release.
    // 在显式释放后返回形状有效的迟到结果。
    let late_error = core
        .resume(
            &cancel_handle,
            EffectResult {
                effect_id: cancel_request.effect_id,
                result: HostEffectResult::StorageMutation(late_outcome),
            },
        )
        .expect_err("released command must reject late result");
    assert_eq!(late_error.code, ErrorCode::CommandNotFound);
    // Start another command to verify mismatched identity does not consume the pending state.
    // 启动另一命令以验证不匹配身份不会消耗待处理状态。
    let (mismatch_handle, mismatch_step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::ChatCompact(ChatCompactCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: SessionSourceId::new("session-mismatch")
                        .expect("session identity must be valid"),
                    compacted_at_ns: 2,
                    mutation_identity: identity,
                })
            },
            "compact:mismatch",
            2,
        ))
        .expect("ChatCompact start must succeed");
    // Extract the actual pending request.
    // 提取实际待处理请求。
    let CommandStep::Effects(mut mismatch_effects) = mismatch_step else {
        assert!(false, "ChatCompact start must return an effect");
        return;
    };
    // Retain the valid request for a second resume attempt.
    // 保留有效请求以进行第二次恢复尝试。
    let mismatch_request = mismatch_effects.pop().expect("pending effect must exist");
    let recovered_outcome = committed_outcome(
        &mismatch_request,
        1,
        Some(ChatCompactMutationResult {
            accepted: true,
            updated: false,
            compacted_turn_id: None,
        }),
    );
    // Resume with an unrelated effect identity.
    // 使用无关 Effect 标识恢复。
    let mismatch_error = core
        .resume(
            &mismatch_handle,
            EffectResult {
                effect_id: EffectId::new("wrong-effect").expect("effect identity must be valid"),
                result: HostEffectResult::Llm(LlmOutput::PreCheckIntent {
                    need_memory: false,
                    queries: Vec::new(),
                    reason: "mismatched test effect".to_owned(),
                }),
            },
        )
        .expect_err("mismatched effect must fail");
    assert_eq!(mismatch_error.code, ErrorCode::EffectMismatch);
    assert_eq!(
        core.command_diagnostics(&mismatch_handle)
            .expect("waiting diagnostics must exist")
            .status,
        CommandStatus::Waiting
    );
    // Resume with the exact original effect identity.
    // 使用精确原始 Effect 标识恢复。
    let recovered = core
        .resume(
            &mismatch_handle,
            EffectResult {
                effect_id: mismatch_request.effect_id.clone(),
                result: HostEffectResult::StorageMutation(recovered_outcome),
            },
        )
        .expect("exact pending result must recover");
    let recovered_receipt = receipt_for_request(&mismatch_request);
    assert_eq!(
        recovered,
        CommandStep::Complete(Box::new(CommandResult::ChatCompact(ChatCompactResult {
            accepted: true,
            updated: false,
            compacted_turn_id: None,
            receipt: recovered_receipt,
        })))
    );
    core.release_command(&mismatch_handle)
        .expect("completed command must release");
    // Start a direct write and drive its first embedding effect.
    // 启动直接写入并驱动其首个嵌入 Effect。
    let (uncertain_handle, uncertain_step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "uncertain write",
                        Vec::new(),
                        50,
                    ),
                    occurred_at_ns: 3,
                    mutation_identity: identity,
                })
            },
            "uncertain-write-1",
            3,
        ))
        .expect("memory write start must succeed");
    // Create a host configured for one uncertain mutation.
    // 创建配置为一次不确定变更的宿主。
    let mut host = DeterministicHost::new();
    host.uncertain_next_mutation = true;
    // Execute the first embedding request.
    // 执行首个嵌入请求。
    let CommandStep::Effects(mut embedding_effects) = uncertain_step else {
        assert!(false, "memory write must first request embedding");
        return;
    };
    // Execute the exact first effect through the public host interface.
    // 通过公共宿主接口执行精确首个 Effect。
    let embedding_result = host
        .execute_effect(
            embedding_effects
                .pop()
                .expect("embedding effect must exist"),
        )
        .expect("embedding effect must succeed");
    // Resume to the storage mutation request.
    // 恢复到存储变更请求。
    let mutation_step = core
        .resume(&uncertain_handle, embedding_result)
        .expect("embedding resume must succeed");
    // Extract the exact mutation request.
    // 提取精确变更请求。
    // Execute the forced uncertain mutation result after all read-only review effects.
    // 在全部只读审核 Effect 后执行强制结果不确定的变更结果。
    let mutation_request =
        drive_to_storage_mutation(&core, &uncertain_handle, &mut host, mutation_step);
    let mutation_identity = match &mutation_request.effect {
        HostEffect::StorageMutation(batch) => batch.identity.clone(),
        _ => panic!("direct write must emit a storage mutation"),
    };
    let uncertain_result = host
        .execute_effect(mutation_request)
        .expect("host must return typed uncertain outcome");
    // Resume and capture the explicit no-replay error.
    // 恢复并捕获显式禁止重放错误。
    let uncertain_error = core
        .resume(&uncertain_handle, uncertain_result)
        .expect_err("uncertain mutation must not complete");
    assert_eq!(uncertain_error.code, ErrorCode::StorageResultUncertain);
    assert!(uncertain_error.side_effect_possible);
    assert!(uncertain_error.idempotency_key.is_some());
    assert_eq!(
        core.command_diagnostics(&uncertain_handle)
            .expect("uncertain diagnostics must exist")
            .status,
        CommandStatus::ResultUncertain
    );
    assert_eq!(
        core.release_command(&uncertain_handle)
            .expect_err("result-uncertain commands must retain reconciliation authority")
            .code,
        ErrorCode::InvalidArgument
    );
    // Refuse a reconciliation result that still cannot resolve durable commit state.
    // 拒绝仍无法解析持久提交状态的对账结果。
    assert_eq!(
        core.reconcile(&uncertain_handle, MutationOutcome::Uncertain)
            .expect_err("reconciliation cannot remain uncertain")
            .code,
        ErrorCode::InvalidArgument
    );
    // Complete the retained flow from the host's durable committed receipt without replay.
    // 使用宿主持久已提交收据完成保留流程，且不重放写入。
    let reconciled = core
        .reconcile(
            &uncertain_handle,
            committed_direct_memory_outcome(&mutation_identity, 1, "memory-reconciled"),
        )
        .expect("committed reconciliation must complete the command");
    assert!(matches!(
        reconciled,
        CommandStep::Complete(ref result)
            if matches!(result.as_ref(), CommandResult::MemoryWrite(_))
    ));
    assert_eq!(
        core.command_diagnostics(&uncertain_handle)
            .expect("reconciled diagnostics must exist")
            .status,
        CommandStatus::Completed
    );
    core.release_command(&uncertain_handle)
        .expect("reconciled command must release after diagnostics");

    // Verify that caller-owned batch idempotency survives an uncertain result unchanged.
    // 验证调用方自有的批次幂等键在结果不确定时保持不变。
    let (batch_subject_id, batch_scope_access, batch_vector_space_id) = identities();
    let (batch_handle, batch_step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                    subject_id: batch_subject_id,
                    scope_access: batch_scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&batch_scope_access),
                    session_source_id: SessionSourceId::new("session-uncertain-batch")
                        .expect("session identity must be valid"),
                    vector_space_id: batch_vector_space_id,
                    occurred_at_ns: 4,
                    authorized_write_scopes: vec![batch_scope_access.write_scope.clone()],
                    items: vec![MemoryWriteBatchItem {
                        target_scope_id: batch_scope_access.write_scope.clone(),
                        scope_class: MemoryScopeClass::Environment,
                        category: MemoryCategory::General,
                        summary: "uncertain batch".to_owned(),
                        details: "uncertain batch details".to_owned(),
                        priority: Some(50),
                        memory_level: Some(MemoryLevel::Stable),
                        expires_at_ns: None,
                        tags: Vec::new(),
                    }],
                    mutation_identity: identity,
                })
            },
            "caller-owned-uncertain-key",
            4,
        ))
        .expect("batch write start must succeed");
    let batch_mutation_request =
        drive_to_storage_mutation(&core, &batch_handle, &mut host, batch_step);
    let HostEffect::StorageMutation(ref batch) = batch_mutation_request.effect else {
        assert!(false, "batch write must emit storage mutation");
        return;
    };
    // The durable batch identity is deterministically derived from the caller-owned key.
    // 持久批次身份从调用方自有键确定性派生。
    assert_eq!(
        batch.identity.idempotency_key.as_str(),
        "vmm-key-v1:637ecc5cea6305aa3629a322c0b979dffce647dd7e1f37548d90cd1e0f035c8a"
    );
    let batch_uncertain_error = core
        .resume(
            &batch_handle,
            EffectResult {
                effect_id: batch_mutation_request.effect_id,
                result: HostEffectResult::StorageMutation(MutationOutcome::Uncertain),
            },
        )
        .expect_err("uncertain batch mutation must not complete");
    assert_eq!(
        batch_uncertain_error.idempotency_key,
        Some(
            IdempotencyKey::new(
                "vmm-key-v1:637ecc5cea6305aa3629a322c0b979dffce647dd7e1f37548d90cd1e0f035c8a"
            )
            .expect("idempotency identity must be valid")
        )
    );
    // Resolve a confirmed non-commit as a terminal storage failure without replay.
    // 将已确认未提交解析为终态存储失败，且不重放写入。
    assert_eq!(
        core.reconcile(
            &batch_handle,
            MutationOutcome::Rejected {
                reason: "durable lookup confirmed no commit".to_owned(),
            },
        )
        .expect_err("rejected reconciliation must fail the command")
        .code,
        ErrorCode::StorageConflict
    );
    assert_eq!(
        core.command_diagnostics(&batch_handle)
            .expect("failed reconciliation diagnostics must exist")
            .status,
        CommandStatus::Failed
    );
    core.release_command(&batch_handle)
        .expect("reconciled batch command must release");
    assert_eq!(
        core.command_diagnostics(&batch_handle)
            .expect_err("released command diagnostics must fail")
            .code,
        ErrorCode::CommandNotFound
    );
}

/// Verifies an oversized Host result is classified as an Effect violation and leaves the request retryable.
/// 验证超限宿主结果被归类为 Effect 违规，并保留原请求供宿主纠正后重试。
#[test]
fn oversized_effect_result_does_not_consume_pending_request() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let (handle, step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::ChatCompact(ChatCompactCommand {
                    subject_id,
                    scope_access,
                    session_source_id: SessionSourceId::new("session-oversized-effect")
                        .expect("session identity must be valid"),
                    compacted_at_ns: 2,
                    mutation_identity: identity,
                })
            },
            "compact:oversized-effect",
            2,
        ))
        .expect("ChatCompact start must succeed");
    let CommandStep::Effects(mut effects) = step else {
        panic!("ChatCompact start must return an effect");
    };
    let request = effects.pop().expect("pending effect must exist");
    let recovered_outcome = committed_outcome(
        &request,
        1,
        Some(ChatCompactMutationResult {
            accepted: true,
            updated: false,
            compacted_turn_id: None,
        }),
    );

    // Cross the private continuation bound without relying on ABI payload validation.
    // 不依赖 ABI 载荷校验，直接跨越私有续体上限。
    let error = core
        .resume(
            &handle,
            EffectResult {
                effect_id: request.effect_id.clone(),
                result: HostEffectResult::Llm(LlmOutput::PreCheckIntent {
                    need_memory: false,
                    queries: Vec::new(),
                    reason: "x".repeat(16_777_217),
                }),
            },
        )
        .expect_err("oversized Host result must fail closed");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
    assert_eq!(error.stage, ErrorStage::Effect);
    assert_eq!(
        core.command_diagnostics(&handle)
            .expect("waiting diagnostics must remain available")
            .status,
        CommandStatus::Waiting
    );

    let recovered = core
        .resume(
            &handle,
            EffectResult {
                effect_id: request.effect_id,
                result: HostEffectResult::StorageMutation(recovered_outcome),
            },
        )
        .expect("corrected Host result must still resume the pending request");
    assert!(matches!(
        recovered,
        CommandStep::Complete(result)
            if matches!(*result, CommandResult::ChatCompact(_))
    ));
}

/// Verifies the synchronous convenience runner retains uncertain mutation state for reconciliation.
/// 验证同步便捷执行器会保留结果不确定的变更状态以供对账。
#[test]
fn automatic_runner_preserves_uncertain_command_for_reconciliation() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut host = DeterministicHost::new();
    host.uncertain_next_mutation = true;
    let error = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::ChatCompact(ChatCompactCommand {
                        subject_id,
                        scope_access,
                        session_source_id: SessionSourceId::new("session-auto-uncertain")
                            .expect("session identity must be valid"),
                        compacted_at_ns: 3,
                        mutation_identity: identity,
                    })
                },
                "compact:auto-uncertain",
                3,
            ),
            &mut host,
        )
        .expect_err("uncertain mutation must leave the automatic run unresolved");
    assert_eq!(error.code, ErrorCode::StorageResultUncertain);
    assert!(error.side_effect_possible);
    let handle = vulcan_memorymesh_core::CommandHandle {
        command_id: error
            .command_id
            .clone()
            .expect("uncertain error must expose the retained command identity"),
        config_snapshot_id: error
            .config_snapshot_id
            .clone()
            .expect("uncertain error must expose the retained snapshot identity"),
    };
    assert_eq!(
        core.command_diagnostics(&handle)
            .expect("uncertain command must remain retained")
            .status,
        CommandStatus::ResultUncertain
    );

    let reconciliation_error = core
        .reconcile(
            &handle,
            MutationOutcome::Rejected {
                reason: "host ledger confirms no commit".to_owned(),
            },
        )
        .expect_err("a rejected reconciliation must report the durable conflict");
    assert_eq!(reconciliation_error.code, ErrorCode::StorageConflict);
    assert_eq!(
        core.command_diagnostics(&handle)
            .expect("reconciled failure diagnostics must remain available")
            .status,
        CommandStatus::Failed
    );
    core.release_command(&handle)
        .expect("reconciled terminal command must release");
}

/// Verifies a Host error after commit is retained as uncertain instead of being cancelled.
/// 验证提交后的宿主错误会被保留为结果不确定，而不是被取消。
#[test]
fn automatic_runner_preserves_committed_mutation_after_acknowledgement_loss() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut host = DeterministicHost::new();
    host.fail_after_committed_mutation = true;
    let operation_key = "compact:auto-ack-loss";
    let error = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::ChatCompact(ChatCompactCommand {
                        subject_id,
                        scope_access,
                        session_source_id: SessionSourceId::new("session-auto-ack-loss")
                            .expect("session identity must be valid"),
                        compacted_at_ns: 4,
                        mutation_identity: identity,
                    })
                },
                operation_key,
                4,
            ),
            &mut host,
        )
        .expect_err("lost committed acknowledgement must require reconciliation");
    assert_eq!(error.code, ErrorCode::StorageResultUncertain);
    assert!(error.side_effect_possible);
    assert!(
        error
            .diagnostic
            .contains("lost the committed mutation acknowledgement")
    );
    let idempotency_key = error
        .idempotency_key
        .clone()
        .expect("uncertain error must expose the durable idempotency key");
    let durable_outcome = host
        .storage
        .reconcile(&idempotency_key)
        .expect("Host ledger must retain the committed outcome");
    assert!(matches!(durable_outcome, MutationOutcome::Committed { .. }));
    let handle = vulcan_memorymesh_core::CommandHandle {
        command_id: error
            .command_id
            .expect("uncertain error must expose the retained command identity"),
        config_snapshot_id: error
            .config_snapshot_id
            .expect("uncertain error must expose the retained snapshot identity"),
    };
    assert_eq!(
        core.command_diagnostics(&handle)
            .expect("uncertain command must remain retained")
            .status,
        CommandStatus::ResultUncertain
    );
    let reconciled = core
        .reconcile(&handle, durable_outcome)
        .expect("the exact durable outcome must reconcile without replay");
    assert!(matches!(reconciled, CommandStep::Complete(_)));
    core.release_command(&handle)
        .expect("reconciled terminal command must release");
}

/// Verifies identity-correct Host records still fail when their durable domain data is malformed.
/// 验证身份正确但持久领域数据畸形的宿主记录仍会被拒绝。
#[test]
fn memory_details_rejects_nonfinite_host_record_scores() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let written = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id,
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::General,
                            "invalid Host score fixture",
                            Vec::new(),
                            50,
                        ),
                        occurred_at_ns: 4,
                        mutation_identity: identity,
                    })
                },
                "write:invalid-host-score-fixture",
                4,
            ),
            &mut host,
        )
        .expect("fixture memory write must complete");
    let CommandResult::MemoryWrite(written) = written else {
        panic!("fixture write must return a memory");
    };
    let mut malformed = written.memory;
    malformed.confidence = Some(f32::NAN);
    let (handle, step) = core
        .start(CoreCommand::MemoryDetails(MemoryDetailsCommand {
            subject_id,
            scope_access,
            memory_id: malformed.id.clone(),
        }))
        .expect("details command must start");
    let CommandStep::Effects(mut effects) = step else {
        panic!("details command must request storage");
    };
    let effect = effects.pop().expect("details effect must exist");
    let error = core
        .resume(
            &handle,
            EffectResult {
                effect_id: effect.effect_id,
                result: HostEffectResult::StorageQuery(StorageQueryResult::MemoryDetails(
                    Box::new(Some(malformed)),
                )),
            },
        )
        .expect_err("non-finite Host record score must fail closed");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
    assert_eq!(error.stage, ErrorStage::Effect);
}

/// Verifies a committed receipt cannot smuggle an unrelated entity version into a valid batch.
/// 验证已提交收据不能把无关实体版本夹带进有效批次。
#[test]
fn commit_receipt_rejects_unrelated_entity_versions() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id,
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id,
                item: direct_write_item(
                    scope_access.write_scope,
                    MemoryCategory::General,
                    "receipt exactness",
                    Vec::new(),
                    50,
                ),
                occurred_at_ns: 1,
                mutation_identity: identity,
            })
        },
        "write:receipt-exactness",
        1,
    );
    let (handle, step) = core.start(command).expect("direct write must start");
    let request = drive_to_storage_mutation(&core, &handle, &mut host, step);
    let HostEffect::StorageMutation(batch) = &request.effect else {
        panic!("direct write must expose a storage mutation");
    };
    let correct_outcome =
        committed_direct_memory_outcome(&batch.identity, 1, "memory-receipt-exact");
    let mut outcome = correct_outcome.clone();
    let MutationOutcome::Committed { receipt, .. } = &mut outcome else {
        panic!("fixture must be committed");
    };
    receipt.committed_versions.push(VersionedEntity {
        entity_kind: EntityKind::Memory,
        domain_id: "memory-unrelated".to_owned(),
        version: 1,
    });
    let error = core
        .resume(
            &handle,
            EffectResult {
                effect_id: request.effect_id,
                result: HostEffectResult::StorageMutation(outcome.clone()),
            },
        )
        .expect_err("unrelated receipt version must be rejected");
    assert_eq!(error.code, ErrorCode::StorageResultUncertain);
    assert!(error.side_effect_possible);
    assert_eq!(
        core.command_diagnostics(&handle)
            .expect("uncertain command diagnostics must exist")
            .status,
        CommandStatus::ResultUncertain
    );
    let repeated_error = core
        .reconcile(&handle, outcome)
        .expect_err("an invalid reconciliation receipt must remain uncertain");
    assert_eq!(repeated_error.code, ErrorCode::StorageResultUncertain);
    assert_eq!(
        core.command_diagnostics(&handle)
            .expect("reconciliation retry diagnostics must exist")
            .status,
        CommandStatus::ResultUncertain
    );
    let completed = core
        .reconcile(&handle, correct_outcome)
        .expect("corrected durable outcome must reconcile");
    assert!(matches!(
        completed,
        CommandStep::Complete(result) if matches!(*result, CommandResult::MemoryWrite(_))
    ));
}

/// Verifies Core preserves VMM semantics by clamping finite Provider rerank scores.
/// 验证 Core 通过钳制有限 Provider 重排评分保持 VMM 语义。
#[test]
fn rerank_provider_scores_are_clamped() {
    let (subject_id, scope_access, vector_space_id) = identities();
    let rerank_core = MemoryMeshCore::new().expect("Core must initialize");
    let mut rerank_host = DeterministicHost::new();
    rerank_core
        .run_to_completion(
            with_identity(
                &rerank_core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::General,
                            "normalized rerank contract",
                            Vec::new(),
                            50,
                        ),
                        occurred_at_ns: 2,
                        mutation_identity: identity,
                    })
                },
                "write:rerank-contract",
                2,
            ),
            &mut rerank_host,
        )
        .expect("seed write must complete");
    rerank_core
        .run_to_completion(
            with_identity(
                &rerank_core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::BusinessLogic,
                            "secondary rerank document",
                            Vec::new(),
                            50,
                        ),
                        occurred_at_ns: 2,
                        mutation_identity: identity,
                    })
                },
                "write:rerank-contract-secondary",
                2,
            ),
            &mut rerank_host,
        )
        .expect("secondary seed write must complete");
    rerank_host.out_of_range_rerank_scores = true;
    let searched = rerank_core
        .run_to_completion(
            CoreCommand::MemorySearch(MemorySearchCommand {
                subject_id,
                scope_access: scope_access.clone(),
                query: "normalized rerank contract".to_owned(),
                limit: 10,
                vector_space_id,
                boundary: fixture_search_boundary(&scope_access),
                now_ns: 3,
            }),
            &mut rerank_host,
        )
        .expect("finite Provider rerank scores must be clamped by Core");
    let CommandResult::MemorySearch(hits) = searched else {
        panic!("search must return typed hits");
    };
    assert_eq!(hits.len(), 2);
    assert!(
        hits.iter()
            .all(|hit| matches!(hit.rerank_score, Some(0.0 | 1.0)))
    );
    assert!(
        hits.iter()
            .all(|hit| (0.0..=1.0).contains(&hit.final_score))
    );
    assert!(
        hits.iter()
            .any(|hit| hit.search_score > 0.0 && hit.final_score == 0.0),
        "rerank must replace a lower fusion score instead of blending with it"
    );
}

/// Verifies one waiting command checkpoints and restores into a fresh Core at the exact effect position.
/// 验证一个等待命令可检查点化并在全新 Core 中于精确 Effect 位置恢复。
#[test]
fn waiting_command_checkpoints_and_restores() {
    let first = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _vector_space_id) = identities();
    let (handle, step) = first
        .start(with_identity(
            &first,
            |identity| {
                CoreCommand::ChatCompact(ChatCompactCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: SessionSourceId::new("session-checkpoint")
                        .expect("session identity must be valid"),
                    compacted_at_ns: 7,
                    mutation_identity: identity,
                })
            },
            "compact:checkpoint",
            7,
        ))
        .expect("ChatCompact start must succeed");
    let CommandStep::Effects(mut effects) = step else {
        assert!(false, "ChatCompact start must return an effect");
        return;
    };
    let pending = effects.pop().expect("pending effect must exist");
    let checkpoint = first
        .export_checkpoint(&handle)
        .expect("waiting command checkpoint must export");

    // A fresh Core shares the same content-addressed default snapshot, so restore
    // must resume the exact waiting effect without re-emitting earlier stages.
    // 全新 Core 共享相同内容寻址默认快照，因此恢复必须在精确等待 Effect 处续跑，
    // 而不重新发出更早阶段。
    let restored = MemoryMeshCore::new().expect("restored Core must initialize");
    let (restored_handle, restored_step) = restored
        .restore_checkpoint(&checkpoint)
        .expect("checkpoint must restore");
    let CommandStep::Effects(mut restored_effects) = restored_step else {
        assert!(false, "restored command must still be waiting");
        return;
    };
    let restored_pending = restored_effects.pop().expect("restored effect must exist");
    assert_eq!(
        restored_pending.effect_id, pending.effect_id,
        "restored command must resume at the exact pending effect"
    );
    let restored_outcome = committed_outcome(
        &restored_pending,
        1,
        Some(ChatCompactMutationResult {
            accepted: true,
            updated: true,
            compacted_turn_id: Some(
                TurnId::new("turn-checkpoint").expect("turn identity must be valid"),
            ),
        }),
    );
    let result = restored
        .resume(
            &restored_handle,
            EffectResult {
                effect_id: restored_pending.effect_id,
                result: HostEffectResult::StorageMutation(restored_outcome),
            },
        )
        .expect("restored command must resume");
    assert!(
        matches!(result, CommandStep::Complete(ref value) if matches!(value.as_ref(), CommandResult::ChatCompact(_))),
        "restored command must complete after its pending effect"
    );
    restored
        .release_command(&restored_handle)
        .expect("restored terminal command must release");
    let duplicate_error = restored
        .restore_checkpoint(&checkpoint)
        .expect_err("one Core instance must never reissue a released checkpoint handle");
    assert_eq!(duplicate_error.code, ErrorCode::CheckpointIncompatible);

    let (next_handle, _) = restored
        .start(CoreCommand::MemoryDetails(MemoryDetailsCommand {
            subject_id,
            scope_access,
            memory_id: MemoryId::new("memory-after-checkpoint")
                .expect("memory identity must be valid"),
        }))
        .expect("a new local command must start after releasing the restored command");
    assert_ne!(
        next_handle.command_id, restored_handle.command_id,
        "a local command must not reuse a released restored handle"
    );
    restored
        .cancel(&next_handle)
        .expect("new local command must cancel");
    restored
        .release_command(&next_handle)
        .expect("cancelled local command must release");
}

/// Verifies shared-memory delete and maintenance preflights survive checkpoint restore with exact access identity.
/// 验证共享记忆删除与维护预检携带精确访问身份，并可跨检查点恢复。
#[test]
fn shared_memory_preflights_preserve_authority_across_checkpoint_restore() {
    let first = MemoryMeshCore::new().expect("Core must initialize");
    let mut host = DeterministicHost::new();
    let subject_id = SubjectId::new("shared-reader").expect("subject identity must be valid");
    let scope_id = ScopeId::new("shared-scope").expect("scope identity must be valid");
    let scope_access = ScopeAccess {
        write_scope: scope_id.clone(),
        visible_scopes: vec![scope_id.clone()],
    };
    let delete_record = shared_memory_record("shared-delete", &scope_id, MemoryState::Active);
    seed_shared_memory(&mut host, delete_record.clone(), "delete");

    let (handle, step) = first
        .start(with_identity(
            &first,
            |identity| {
                CoreCommand::MemoryDelete(MemoryDeleteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    memory_id: delete_record.id.clone(),
                    expected_version: Some(1),
                    mutation_identity: identity,
                })
            },
            "shared-delete-checkpoint",
            2,
        ))
        .expect("shared delete must start");
    let CommandStep::Effects(mut effects) = step else {
        panic!("shared delete must begin with a storage preflight");
    };
    let pending = effects.pop().expect("shared delete preflight must exist");
    assert!(matches!(
        pending.effect,
        HostEffect::StorageQuery(StorageQuery::MemoryDetails { .. })
    ));
    let checkpoint = first
        .export_checkpoint(&handle)
        .expect("shared delete preflight checkpoint must export");

    let restored = MemoryMeshCore::new().expect("restored Core must initialize");
    let (restored_handle, restored_step) = restored
        .restore_checkpoint(&checkpoint)
        .expect("shared delete preflight checkpoint must restore");
    let CommandStep::Effects(mut restored_effects) = restored_step else {
        panic!("restored shared delete must still wait on its preflight");
    };
    let restored_pending = restored_effects
        .pop()
        .expect("restored shared delete preflight must exist");
    assert_eq!(restored_pending, pending);
    let preflight_result = host
        .execute_effect(restored_pending)
        .expect("shared delete preflight must execute");
    let mutation_step = restored
        .resume(&restored_handle, preflight_result)
        .expect("shared delete preflight must resume");
    let mutation_request =
        drive_to_storage_mutation(&restored, &restored_handle, &mut host, mutation_step);
    let completed = host
        .execute_effect(mutation_request)
        .and_then(|result| restored.resume(&restored_handle, result))
        .expect("shared delete mutation must complete");
    assert!(matches!(
        completed,
        CommandStep::Complete(ref result)
            if matches!(result.as_ref(), CommandResult::MemoryDelete(value) if value.deleted)
    ));
    restored
        .release_command(&restored_handle)
        .expect("shared delete command must release");
    assert!(
        host.storage
            .dataset()
            .memories
            .iter()
            .all(|record| record.id != delete_record.id)
    );

    let terminal_record =
        shared_memory_record("shared-retention", &scope_id, MemoryState::Superseded);
    seed_shared_memory(&mut host, terminal_record.clone(), "retention");
    let retention_result = restored
        .run_to_completion(
            with_identity(
                &restored,
                |identity| {
                    CoreCommand::Retention(RetentionCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        now_ns: 3,
                        policy: retention_policy(),
                        mutation_identity: identity,
                    })
                },
                "shared-retention",
                3,
            ),
            &mut host,
        )
        .expect("shared retention planning must complete");
    let CommandResult::Retention(retention_plan) = retention_result else {
        panic!("shared retention must return its typed plan");
    };
    assert_eq!(retention_plan.terminal_memory_recycle.len(), 1);
    assert_eq!(
        retention_plan.terminal_memory_recycle[0]
            .expected_access
            .access_class,
        MemoryAccessClass::ScopeShared
    );
    assert!(
        retention_plan.terminal_memory_recycle[0]
            .expected_access
            .source_subject_id
            .is_none()
    );
    apply_retention_recycle_plan(&mut host, &retention_plan);
    assert!(host.storage.dataset().memories.iter().any(|record| {
        record.id == terminal_record.id && record.state == MemoryState::Recycled
    }));

    let restore_record = shared_memory_record("shared-restore", &scope_id, MemoryState::Recycled);
    let purge_record = shared_memory_record("shared-purge", &scope_id, MemoryState::Recycled);
    seed_shared_memory(&mut host, restore_record.clone(), "restore");
    seed_shared_memory(&mut host, purge_record.clone(), "purge");
    let restored_result = restored
        .run_to_completion(
            with_identity(
                &restored,
                |identity| {
                    CoreCommand::Maintenance(MaintenanceCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        occurred_at_ns: 3,
                        action: MaintenanceAction::RestoreRecycled(vec![VersionedMemoryRef {
                            memory_id: restore_record.id.clone(),
                            expected_version: 1,
                        }]),
                        mutation_identity: identity,
                    })
                },
                "shared-restore",
                3,
            ),
            &mut host,
        )
        .expect("shared restore must complete");
    assert!(matches!(restored_result, CommandResult::Maintenance(_)));
    let active_shared = host
        .storage
        .dataset()
        .memories
        .into_iter()
        .find(|record| record.id == restore_record.id)
        .expect("restored shared record must exist");
    assert_eq!(active_shared.state, MemoryState::Active);
    assert_eq!(active_shared.access_class, MemoryAccessClass::ScopeShared);
    assert!(active_shared.source_subject_id.is_none());

    let purged_result = restored
        .run_to_completion(
            with_identity(
                &restored,
                |identity| {
                    CoreCommand::Maintenance(MaintenanceCommand {
                        subject_id,
                        scope_access,
                        occurred_at_ns: 4,
                        action: MaintenanceAction::PurgeRecycled(vec![VersionedMemoryRef {
                            memory_id: purge_record.id.clone(),
                            expected_version: 1,
                        }]),
                        mutation_identity: identity,
                    })
                },
                "shared-purge",
                4,
            ),
            &mut host,
        )
        .expect("shared purge must complete");
    assert!(matches!(purged_result, CommandResult::Maintenance(_)));
    assert!(
        host.storage
            .dataset()
            .memories
            .iter()
            .all(|record| record.id != purge_record.id)
    );
}

/// Verifies an ambiguous storage-mutation failure preserves reconciliation authority.
/// 验证语义不确定的存储变更失败会保留对账权威。
#[test]
fn nonretryable_storage_failure_becomes_result_uncertain() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    core.run_to_completion(
        with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::ProjectContext,
                        "Rust systems preference",
                        vec!["rust".to_owned()],
                        20,
                    ),
                    occurred_at_ns: 10,
                    mutation_identity: identity,
                })
            },
            "uncertain-reinforcement-seed",
            10,
        ),
        &mut host,
    )
    .expect("reinforcement seed must persist");

    let (handle, mut step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::PreCheck(PreCheckCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: SessionSourceId::new("session-reinforcement-uncertain")
                        .expect("session identity must be valid"),
                    vector_space_id: vector_space_id.clone(),
                    candidate_scope: fixture_candidate_scope(&scope_access),
                    user_content: "Which language do I prefer for systems work?".to_owned(),
                    recent_turns: Vec::new(),
                    recall_mode: RecallMode::Auto,
                    session_recall_mode: SessionRecallMode::CompactAware,
                    max_search_queries: 3,
                    max_memories: 3,
                    now_ns: 20,
                    mutation_identity: identity,
                })
            },
            "uncertain-reinforcement",
            20,
        ))
        .expect("PreCheck must start");
    loop {
        let CommandStep::Effects(mut effects) = step else {
            panic!("PreCheck completed before its reinforcement mutation");
        };
        let request = effects.pop().expect("one pending effect must exist");
        let is_reinforcement = matches!(
            &request.effect,
            HostEffect::StorageMutation(batch)
                if batch
                    .mutations
                    .iter()
                    .any(|mutation| matches!(mutation, Mutation::ReinforceMemory { .. }))
        );
        if is_reinforcement {
            let error = core
                .resume(
                    &handle,
                    EffectResult {
                        effect_id: request.effect_id,
                        result: HostEffectResult::Failure(EffectFailure {
                            kind: EffectFailureKind::StorageMutation,
                            class: FailureClass::Transport,
                            retryable: false,
                            allow_degrade: true,
                            diagnostic: "commit acknowledgement was lost".to_owned(),
                        }),
                    },
                )
                .expect_err("an ambiguous mutation failure must require reconciliation");
            assert_eq!(error.code, ErrorCode::StorageResultUncertain);
            assert!(error.side_effect_possible);
            assert_eq!(
                core.command_diagnostics(&handle)
                    .expect("uncertain diagnostics must exist")
                    .status,
                CommandStatus::ResultUncertain
            );
            let reconciliation_error = core
                .reconcile(
                    &handle,
                    MutationOutcome::Rejected {
                        reason: "durable operation ledger confirms no commit".to_owned(),
                    },
                )
                .expect_err("a rejected reinforcement reconciliation must fail the command");
            assert_eq!(reconciliation_error.code, ErrorCode::StorageConflict);
            core.release_command(&handle)
                .expect("reconciled failed command must release");
            return;
        }
        step = core
            .resume(
                &handle,
                host.execute_effect(request)
                    .expect("pre-reinforcement effect must execute"),
            )
            .expect("pre-reinforcement effect must resume");
    }
}

/// Verifies a structured host effect failure degrades an optional stage instead of terminating.
/// 验证一个结构化宿主 Effect 失败会降级可选阶段而非终止命令。
#[test]
fn host_effect_failure_degrades_optional_lexical_stage() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed one direct memory so the search has a candidate.
    // 播种一条主动记忆，使搜索拥有候选。
    let write = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "喜欢确定性测试",
                    vec!["测试".to_owned()],
                    20,
                ),
                occurred_at_ns: 1,
                mutation_identity: identity,
            })
        },
        "write:degrade",
        1,
    );
    core.run_to_completion(write, &mut host)
        .expect("seed write must complete");

    // Configure the host to fail every lexical candidate effect.
    // 配置宿主使每次词法候选 Effect 都失败。
    host.fail_lexical = true;
    let search = core
        .start(CoreCommand::MemorySearch(MemorySearchCommand {
            subject_id,
            scope_access: scope_access.clone(),
            query: "确定性".to_owned(),
            limit: 10,
            vector_space_id,
            boundary: fixture_search_boundary(&scope_access),
            now_ns: 2,
        }))
        .expect("search start must succeed");
    let (handle, step) = search;
    let CommandStep::Effects(mut effects) = step else {
        assert!(false, "search must be waiting");
        return;
    };
    let pending = effects.pop().expect("pending effect must exist");
    let final_step = core
        .resume(
            &handle,
            host.execute_effect(pending)
                .expect("host effect must execute"),
        )
        .expect("degraded search must resume");
    // Drive the remaining effects to completion.
    let mut step = final_step;
    loop {
        match step {
            CommandStep::Complete(result) => {
                let search_result = match result.as_ref() {
                    CommandResult::MemorySearch(hits) => hits,
                    other => {
                        assert!(false, "unexpected degraded search result: {other:?}");
                        return;
                    }
                };
                assert!(
                    !search_result.is_empty(),
                    "degraded search must still return vector-channel hits"
                );
                break;
            }
            CommandStep::Effects(mut effects) => {
                let pending = effects.pop().expect("pending effect must exist");
                let result = host
                    .execute_effect(pending)
                    .expect("host effect must execute");
                step = core
                    .resume(&handle, result)
                    .expect("degraded search effect must resume");
            }
        }
    }
    core.release_command(&handle)
        .expect("release degraded search");
}

/// Verifies a `PreCheck` L2 review failure returns an empty degraded context instead of
/// injecting every candidate, matching the VMM degradation matrix.
/// 验证 `PreCheck` L2 审核失败返回空的降级上下文而非注入全部候选，与 VMM 降级矩阵一致。
#[test]
fn precheck_review_failure_returns_empty_degraded_context() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    configure_precheck_stage_fixture(&core);
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed one directly relevant memory so recall finds a candidate.
    // 播种一条直接相关的主动记忆，使召回发现候选。
    let write = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "用户偏好 Rust 系统编程",
                    vec!["测试".to_owned()],
                    80,
                ),
                occurred_at_ns: 1,
                mutation_identity: identity,
            })
        },
        "write:review-degrade",
        1,
    );
    core.run_to_completion(write, &mut host)
        .expect("seed write must complete");

    // Fail the L2 review so the L1 accepted context must be dropped, not injected.
    // 使 L2 审核失败，因此 L1 已准入上下文必须被丢弃而非注入。
    host.fail_precheck_review = true;
    let precheck = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PreCheck(PreCheckCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-review-degrade")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        candidate_scope: fixture_candidate_scope(&scope_access),
                        user_content: "What do I prefer for systems work?".to_owned(),
                        recent_turns: Vec::new(),
                        recall_mode: RecallMode::Auto,
                        session_recall_mode: SessionRecallMode::CompactAware,
                        max_search_queries: 3,
                        max_memories: 3,
                        now_ns: 2,
                        mutation_identity: identity,
                    })
                },
                "precheck:review-degrade",
                2,
            ),
            &mut host,
        )
        .expect("degraded PreCheck must complete");
    let CommandResult::PreCheck(precheck) = precheck else {
        assert!(false, "PreCheck must return its typed result");
        return;
    };
    assert!(
        precheck.degraded,
        "review failure must mark the PreCheck result degraded"
    );
    assert!(
        !precheck.should_inject,
        "review failure must not inject every candidate"
    );
    assert!(
        precheck.context_items.is_empty(),
        "review failure must return an empty context"
    );
    assert!(
        !precheck.candidates.is_empty(),
        "review failure must retain non-adopted candidates for audit"
    );
    assert_eq!(precheck.queries, vec!["What do I prefer for systems work?"]);
    assert_eq!(
        precheck.reinforcement, None,
        "review failure must not fabricate reinforcement"
    );
}

/// Verifies every accepted review identity is checked before the caller result limit is applied.
/// 验证在应用调用方结果限制前会校验每一个已接受的审核标识。
#[test]
fn precheck_rejects_foreign_identity_after_result_limit() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let write = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "User prefers Rust systems programming",
                    vec!["review-boundary".to_owned()],
                    80,
                ),
                occurred_at_ns: 1,
                mutation_identity: identity,
            })
        },
        "write:review-foreign",
        1,
    );
    core.run_to_completion(write, &mut host)
        .expect("seed write must complete");

    // The valid first identity fills max_memories=1; the trailing foreign identity must
    // still be rejected instead of being hidden by truncation.
    // 第一个有效标识会填满 max_memories=1；尾部外来标识仍必须被拒绝，不能被截断隐藏。
    host.append_foreign_precheck_identity = true;
    let error = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PreCheck(PreCheckCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-review-foreign")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        candidate_scope: fixture_candidate_scope(&scope_access),
                        user_content: "What do I prefer for systems work?".to_owned(),
                        recent_turns: Vec::new(),
                        recall_mode: RecallMode::Auto,
                        session_recall_mode: SessionRecallMode::CompactAware,
                        max_search_queries: 3,
                        max_memories: 1,
                        now_ns: 2,
                        mutation_identity: identity,
                    })
                },
                "precheck:review-foreign",
                2,
            ),
            &mut host,
        )
        .expect_err("foreign review identity must be rejected");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Verifies `PostAction` commands reject impossible completion, event, and cutoff ordering.
/// 验证 `PostAction` 命令会拒绝不可能的完成时间、事件时间与截止时间顺序。
#[test]
fn postaction_commands_reject_invalid_time_boundaries() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let invalid_intake = with_identity(
        &core,
        |identity| {
            CoreCommand::PostActionIntake(PostActionIntakeCommand {
                input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                session_source_id: SessionSourceId::new("session-invalid-time")
                    .expect("session identity must be valid"),
                vector_space_id: vector_space_id.clone(),
                timeline: vec![TimelineItem {
                    role: Role::User,
                    content: "event after completion".to_owned(),
                    occurred_at_ns: 11,
                }],
                completed_at_ns: 10,
                mutation_identity: identity,
            })
        },
        "postaction:intake-invalid-time",
        12,
    );
    let intake_error = core
        .start(invalid_intake)
        .expect_err("event after completion must be rejected");
    assert_eq!(intake_error.code, ErrorCode::InvalidArgument);

    let future_completion = with_identity(
        &core,
        |identity| {
            CoreCommand::PostActionIntake(PostActionIntakeCommand {
                input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                session_source_id: SessionSourceId::new("session-future-completion")
                    .expect("session identity must be valid"),
                vector_space_id: vector_space_id.clone(),
                timeline: vec![TimelineItem {
                    role: Role::User,
                    content: "completion after observation".to_owned(),
                    occurred_at_ns: 10,
                }],
                completed_at_ns: 13,
                mutation_identity: identity,
            })
        },
        "postaction:intake-future-completion",
        12,
    );
    let completion_error = core
        .start(future_completion)
        .expect_err("completion after observation must be rejected");
    assert_eq!(completion_error.code, ErrorCode::InvalidArgument);

    let invalid_analyze = with_identity(
        &core,
        |identity| {
            CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                profile_sections: vec![subject_profile_section(&scope_access)],
                session_source_id: SessionSourceId::new("session-invalid-time")
                    .expect("session identity must be valid"),
                vector_space_id: vector_space_id.clone(),
                turn_id: TurnId::new("turn-invalid-time").expect("turn identity must be valid"),
                analysis_generation: 1,
                analysis_cutoff_ns: 12,
                direct_write_observed_after_ns: 13,
                mutation_identity: identity,
            })
        },
        "postaction:analyze-invalid-time",
        12,
    );
    let analyze_error = core
        .start(invalid_analyze)
        .expect_err("watermark after cutoff must be rejected");
    assert_eq!(analyze_error.code, ErrorCode::InvalidArgument);
}

/// Verifies every timed mutation rejects business time beyond its host observation boundary.
/// 验证每个带时间的变更都会拒绝超过宿主观测边界的业务时间。
#[test]
fn timed_mutations_reject_future_business_time() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let session_source_id = SessionSourceId::new("session-future-business-time")
        .expect("session identity must be valid");
    let event_time = 11;
    let observed_at_ns = 10;

    let commands = vec![
        with_identity(
            &core,
            |identity| {
                CoreCommand::PreCheck(PreCheckCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    candidate_scope: fixture_candidate_scope(&scope_access),
                    user_content: "future precheck".to_owned(),
                    recent_turns: Vec::new(),
                    recall_mode: RecallMode::Disabled,
                    session_recall_mode: SessionRecallMode::CompactAware,
                    max_search_queries: 1,
                    max_memories: 1,
                    now_ns: event_time,
                    mutation_identity: identity,
                })
            },
            "future-time:precheck",
            observed_at_ns,
        ),
        with_identity(
            &core,
            |identity| {
                CoreCommand::ChatCompact(ChatCompactCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: session_source_id.clone(),
                    compacted_at_ns: event_time,
                    mutation_identity: identity,
                })
            },
            "future-time:compact",
            observed_at_ns,
        ),
        with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "future direct write",
                        Vec::new(),
                        20,
                    ),
                    occurred_at_ns: event_time,
                    mutation_identity: identity,
                })
            },
            "future-time:write",
            observed_at_ns,
        ),
        with_identity(
            &core,
            |identity| {
                CoreCommand::ProfileInstruction(ProfileInstructionCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    target_section: subject_profile_section(&scope_access),
                    instruction: "remember this preference".to_owned(),
                    occurred_at_ns: event_time,
                    mutation_identity: identity,
                })
            },
            "future-time:profile",
            observed_at_ns,
        ),
        with_identity(
            &core,
            |identity| {
                CoreCommand::Retention(RetentionCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    now_ns: event_time,
                    policy: retention_policy(),
                    mutation_identity: identity,
                })
            },
            "future-time:retention",
            observed_at_ns,
        ),
        with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    authorized_write_scopes: vec![scope_access.write_scope.clone()],
                    session_source_id: session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    occurred_at_ns: event_time,
                    items: vec![direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "future direct write batch",
                        Vec::new(),
                        20,
                    )],
                    mutation_identity: identity,
                })
            },
            "future-time:write-batch",
            observed_at_ns,
        ),
        with_identity(
            &core,
            |identity| {
                CoreCommand::Maintenance(MaintenanceCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    occurred_at_ns: event_time,
                    action: MaintenanceAction::RebuildVectors {
                        memory_ids: vec![
                            MemoryId::new("memory-future-maintenance")
                                .expect("memory identity must be valid"),
                        ],
                        vector_space_id: vector_space_id.clone(),
                    },
                    mutation_identity: identity,
                })
            },
            "future-time:maintenance",
            observed_at_ns,
        ),
    ];

    for command in commands {
        let error = core
            .start(command)
            .expect_err("future business time must be rejected before effects");
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }
}

/// Verifies refined `PostAction` references cannot cross the subject ownership boundary.
/// 验证已提炼 `PostAction` 参考不能跨越主体所有权边界。
#[test]
fn postaction_rejects_cross_subject_reference_context() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let session_source_id =
        SessionSourceId::new("session-reference-boundary").expect("session identity must be valid");
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: session_source_id.clone(),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![TimelineItem {
                            role: Role::User,
                            content: "I prefer Rust for this authoritative project".to_owned(),
                            occurred_at_ns: 10,
                        }],
                        completed_at_ns: 10,
                        mutation_identity: identity,
                    })
                },
                "postaction:intake-reference-boundary",
                10,
            ),
            &mut host,
        )
        .expect("pending turn intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        panic!("intake must return its typed result");
    };
    let turn_id = intake.turn_id.expect("intake must bind a pending turn");
    host.inject_foreign_postaction_reference = true;
    let error = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: session_source_id.clone(),
                        vector_space_id: vector_space_id.clone(),
                        turn_id,
                        analysis_generation: 1,
                        analysis_cutoff_ns: 11,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "postaction:analyze-reference-boundary",
                11,
            ),
            &mut host,
        )
        .expect_err("cross-subject reference must be rejected");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Verifies analysis refuses a Host response that no longer represents the requested pending turn.
/// 验证分析会拒绝不再表示所请求待处理轮次的 Host 响应。
#[test]
fn postaction_rejects_already_completed_target_turn() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let session_source_id =
        SessionSourceId::new("session-completed-target").expect("session identity must be valid");
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: session_source_id.clone(),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![TimelineItem {
                            role: Role::User,
                            content: "我下周去北京出差，偏好 Rust 技术栈".to_owned(),
                            occurred_at_ns: 20,
                        }],
                        completed_at_ns: 20,
                        mutation_identity: identity,
                    })
                },
                "postaction:intake-completed-target",
                20,
            ),
            &mut host,
        )
        .expect("pending turn intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        panic!("intake must return its typed result");
    };
    let turn_id = intake.turn_id.expect("intake must bind a pending turn");
    host.complete_postaction_target_before_analysis = true;
    let error = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: session_source_id.clone(),
                        vector_space_id: vector_space_id.clone(),
                        turn_id,
                        analysis_generation: 1,
                        analysis_cutoff_ns: 21,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "postaction:analyze-completed-target",
                21,
            ),
            &mut host,
        )
        .expect_err("completed target turn must be rejected");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Verifies Host memory metadata is revalidated before a public details result is returned.
/// 验证 Host 记忆元数据会在公开详情结果返回前重新校验。
#[test]
fn memory_details_rejects_invalid_host_metadata() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let written = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::General,
                            "bounded host metadata",
                            Vec::new(),
                            20,
                        ),
                        occurred_at_ns: 30,
                        mutation_identity: identity,
                    })
                },
                "write:invalid-host-metadata",
                30,
            ),
            &mut host,
        )
        .expect("fixture write must complete");
    let CommandResult::MemoryWrite(written) = written else {
        panic!("fixture write must return a typed memory");
    };
    host.inject_invalid_memory_metadata = true;
    let error = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id,
                scope_access: scope_access.clone(),
                memory_id: written.memory.id,
            }),
            &mut host,
        )
        .expect_err("NUL-bearing Host metadata must be rejected");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Accepts context read after query start because this source path is a live observation, not an as-of snapshot.
/// 验证接受查询开始后的情境读取；此来源路径是实时观察，而非按查询开始时间冻结的历史快照。
#[test]
fn memory_search_accepts_context_observed_after_query_start() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    core.run_to_completion(
        with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "bounded context evidence",
                        Vec::new(),
                        20,
                    ),
                    occurred_at_ns: 30,
                    mutation_identity: identity,
                })
            },
            "write:future-context-evidence",
            30,
        ),
        &mut host,
    )
    .expect("fixture write must complete");
    host.inject_future_context_evidence = true;
    let result = core
        .run_to_completion(
            CoreCommand::MemorySearch(MemorySearchCommand {
                subject_id,
                scope_access: scope_access.clone(),
                query: "bounded context evidence".to_owned(),
                limit: 1,
                vector_space_id,
                boundary: fixture_search_boundary(&scope_access),
                now_ns: 31,
            }),
            &mut host,
        )
        .expect("live evidence may have been written after query start");
    let CommandResult::MemorySearch(hits) = result else {
        panic!("expected memory search result");
    };
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0]
            .explanation
            .as_ref()
            .expect("explanation")
            .context_total_support_count,
        1
    );
}

/// Queries explicitly supplied profile facts without trusting the reference storage's ordering.
/// 查询显式提供的画像事实，不依赖参考存储的排序顺序。
fn query_profile_facts(
    core: &MemoryMeshCore,
    subject_id: SubjectId,
    scope_access: ScopeAccess,
    view: ProfileView,
    nodes: Vec<ProfileNode>,
) -> Result<vulcan_memorymesh_core::ProfileBundle, CoreError> {
    let (handle, step) = core.start(CoreCommand::ProfileQuery(ProfileQueryCommand {
        subject_id,
        scope_access,
        view,
        evaluated_at_ns: i64::MAX,
    }))?;
    let CommandStep::Effects(mut effects) = step else {
        panic!("profile query must request profile facts");
    };
    let effect = effects.pop().expect("profile query must emit one effect");
    let step = core.resume(
        &handle,
        EffectResult {
            effect_id: effect.effect_id,
            result: HostEffectResult::StorageQuery(StorageQueryResult::ProfileNodes(nodes)),
        },
    )?;
    let CommandStep::Complete(result) = step else {
        panic!("profile facts must complete the query");
    };
    let CommandResult::ProfileQuery(bundle) = *result else {
        panic!("profile query must return its bundle");
    };
    core.release_command(&handle)?;
    Ok(bundle)
}

/// Verifies historical DST dates, P-band sorting, stable ties, empty rendering, and explicit timezone errors.
/// 验证历史夏令时日期、P 档位排序、稳定平局、空渲染与显式时区错误。
#[test]
fn profile_timeline_preserves_historical_timezone_and_semantic_order() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut host = DeterministicHost::new();
    seed_profile_node(&mut host, &subject_id, &scope_access.write_scope, 1);
    let template = host.storage.dataset().profile_nodes[0].clone();
    let mut nodes = [
        // Domain timestamps use nanoseconds, including historical timezone fixtures.
        // 领域时间戳使用纳秒，历史时区夹具也必须如此。
        ("winter", 1_768_451_400_000_000_000, 20, 0, 1),
        ("ordinary", 1_784_089_800_000_000_000, 20, 0, 1),
        ("important", 1_784_089_800_000_000_000, 60, 1, 1),
        ("confirmed", 1_784_089_800_000_000_000, 50, 2, 1),
        ("hard-later", 1_784_089_800_000_000_000, 100, 1, 2),
        ("hard-first", 1_784_089_800_000_000_000, 100, 1, 1),
    ]
    .into_iter()
    .map(|(content, effective, priority, refresh, created)| {
        let mut node = template.clone();
        node.id = ProfileNodeId::new(content).expect("fixture profile ID must be valid");
        node.content = content.to_owned();
        node.effective_date_ns = Some(effective);
        node.priority = priority;
        node.refresh_weight = refresh;
        node.created_at_ns = created;
        node.updated_at_ns = effective;
        node
    })
    .collect::<Vec<_>>();
    let view = ProfileView {
        mode: ProfileViewMode::Full,
        sections: vec![subject_profile_section(&scope_access)],
        include_explanations: true,
        display_timezone: "America/New_York".to_owned(),
    };
    let first = query_profile_facts(
        &core,
        subject_id.clone(),
        scope_access.clone(),
        view.clone(),
        nodes.clone(),
    )
    .expect("historical timeline must render");
    assert_eq!(
        first.sections[0].rendered,
        "2026-01-14:\n[P2][L2][W0] winter\n\n2026-07-15:\n[P0][L2][W1] hard-first\n[P0][L2][W1] hard-later\n[P1][L2][W2] confirmed\n[P1][L2][W1] important\n[P2][L2][W0] ordinary"
    );
    assert!(first.rendered.contains("P/L/W"));
    nodes.reverse();
    let reversed = query_profile_facts(
        &core,
        subject_id.clone(),
        scope_access.clone(),
        view.clone(),
        nodes.clone(),
    )
    .expect("reversed Host facts must render");
    assert_eq!(first, reversed);
    let empty = query_profile_facts(
        &core,
        subject_id.clone(),
        scope_access.clone(),
        view.clone(),
        Vec::new(),
    )
    .expect("empty profile query must succeed");
    assert!(empty.rendered.is_empty());
    assert!(empty.sections[0].rendered.is_empty());
    for timezone in ["", "Local", "Not/A_Timezone"] {
        let mut invalid = view.clone();
        invalid.display_timezone = timezone.to_owned();
        assert_eq!(
            query_profile_facts(
                &core,
                subject_id.clone(),
                scope_access.clone(),
                invalid,
                Vec::new()
            )
            .expect_err("ambiguous or invalid timezone must fail")
            .code,
            ErrorCode::InvalidArgument
        );
    }
    nodes[0].effective_date_ns = Some(i64::MAX);
    // The maximum signed nanosecond timestamp is in year 2262, not outside Chrono's supported calendar.
    // 最大有符号纳秒时间戳位于 2262 年，并未超出 Chrono 支持的日历范围。
    assert!(
        query_profile_facts(&core, subject_id, scope_access, view, nodes)
            .expect("maximum nanosecond timestamp must render")
            .rendered
            .contains("2262-04-11:")
    );
}

/// Verifies one-nanosecond midnight boundaries and the creation anchor when an effective date is absent.
/// 验证午夜前后一纳秒边界，以及生效日期缺失时使用创建锚点。
#[test]
fn profile_timeline_preserves_nanosecond_midnight_boundaries() {
    // Query the public facade with valid authoritative facts instead of calling an internal formatter.
    // 使用有效权威事实查询公共门面，而不是调用内部格式化器。
    let core = MemoryMeshCore::new().expect("Core");
    let (subject_id, scope_access, _) = identities();
    let mut host = DeterministicHost::new();
    seed_profile_node(&mut host, &subject_id, &scope_access.write_scope, 1);
    let template = host.storage.dataset().profile_nodes[0].clone();
    // Exact nanoseconds on either side of the first UTC midnight after the Unix epoch.
    // Unix 纪元后首个 UTC 午夜两侧的准确纳秒。
    let nodes = [
        ("before", 86_400_000_000_000 - 1),
        ("after", 86_400_000_000_000),
    ]
    .into_iter()
    .map(|(content, anchor)| {
        // With no explicit effective date, creation time remains the sole declared fallback.
        // 未声明生效日期时，创建时间仍是唯一已声明的备用锚点。
        let mut node = template.clone();
        node.id = ProfileNodeId::new(content).expect("node ID");
        node.content = content.to_owned();
        node.effective_date_ns = None;
        node.created_at_ns = anchor;
        node.updated_at_ns = anchor;
        node
    })
    .collect();
    let bundle = query_profile_facts(
        &core,
        subject_id,
        scope_access.clone(),
        ProfileView {
            mode: ProfileViewMode::Full,
            sections: vec![subject_profile_section(&scope_access)],
            include_explanations: false,
            display_timezone: "UTC".to_owned(),
        },
        nodes,
    )
    .expect("nanosecond timeline");
    assert_eq!(
        bundle.sections[0].rendered,
        "1970-01-01:\n[P1][L2][W0] before\n\n1970-01-02:\n[P1][L2][W0] after"
    );
}

/// Verifies Core removes an expired active-state profile fact even when the Host returns it.
/// 验证即使宿主返回已过期的 active 状态画像事实，Core 仍会将其移除。
#[test]
fn profile_query_filters_expired_host_facts_at_the_explicit_cutoff() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut host = DeterministicHost::new();
    seed_profile_node(&mut host, &subject_id, &scope_access.write_scope, 1);
    host.inject_expired_profile_fact = true;
    let result = core
        .run_to_completion(
            CoreCommand::ProfileQuery(ProfileQueryCommand {
                subject_id,
                scope_access: scope_access.clone(),
                view: ProfileView {
                    mode: ProfileViewMode::Full,
                    sections: vec![subject_profile_section(&scope_access)],
                    include_explanations: false,
                    display_timezone: "UTC".to_owned(),
                },
                evaluated_at_ns: 10,
            }),
            &mut host,
        )
        .expect("expired Host profile fact must be deterministically filtered");
    let CommandResult::ProfileQuery(bundle) = result else {
        panic!("profile query must return a profile bundle")
    };
    assert_eq!(bundle.nodes.len(), 1);
    assert_eq!(bundle.nodes[0].content, "stable profile fact");
    assert_eq!(
        bundle.sections[0].rendered,
        "1970-01-01:\n[P1][L2][W0] stable profile fact"
    );
    assert!(bundle.rendered.contains("[Subject]\n1970-01-01:"));

    // Split mode exposes only independently rendered section bodies and never returns
    // a second combined string that callers could accidentally concatenate again.
    // Split 模式只暴露独立渲染的分区正文，绝不再返回可能被调用方重复拼接的组合字符串。
    let split = core
        .run_to_completion(
            CoreCommand::ProfileQuery(ProfileQueryCommand {
                subject_id: bundle.subject_id.clone(),
                scope_access: scope_access.clone(),
                view: ProfileView {
                    mode: ProfileViewMode::Split,
                    sections: vec![subject_profile_section(&scope_access)],
                    include_explanations: true,
                    display_timezone: "Atlantic/Cape_Verde".to_owned(),
                },
                evaluated_at_ns: 10,
            }),
            &mut host,
        )
        .expect("split profile query must complete");
    let CommandResult::ProfileQuery(split) = split else {
        panic!("split profile query must return a profile bundle")
    };
    assert!(split.rendered.is_empty());
    assert_eq!(
        split.sections[0].rendered,
        "1969-12-31:\n[P1][L2][W0] stable profile fact"
    );
}

/// Verifies Core rejects a Host profile fact whose source kind and source identity disagree.
/// 验证 Core 拒绝来源类别与来源身份不一致的宿主画像事实。
#[test]
fn profile_query_rejects_inconsistent_host_source_identity() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let mut host = DeterministicHost::new();
    seed_profile_node(&mut host, &subject_id, &scope_access.write_scope, 2);
    host.inject_invalid_profile_source = true;
    let error = core
        .run_to_completion(
            CoreCommand::ProfileQuery(ProfileQueryCommand {
                subject_id,
                scope_access: scope_access.clone(),
                view: ProfileView {
                    mode: ProfileViewMode::Full,
                    sections: vec![subject_profile_section(&scope_access)],
                    include_explanations: false,
                    display_timezone: "UTC".to_owned(),
                },
                evaluated_at_ns: 10,
            }),
            &mut host,
        )
        .expect_err("inconsistent Host profile source identity must be rejected");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Verifies sparse L2 acceptance keeps context evidence bound to the correct committed
/// memory even when an earlier candidate was dropped by L1 or rejected by L2.
/// 验证 L2 稀疏接受使情境证据始终绑定到正确的已提交记忆，即使较早候选被
/// L1 丢弃或 L2 拒绝。
#[test]
fn sparse_review_acceptance_binds_evidence_to_committed_memory() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed one similar memory so the L2 review effect actually fires; otherwise the engine
    // fast-paths an accept-all synthetic review and the sparse path stays untested.
    // 播种一条相似记忆使 L2 审核 Effect 真正触发；否则引擎走"全接受"合成审核
    // 快路径，稀疏路径不会被测试到。
    let seed = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "用户偏好 Rust 技术栈",
                    vec!["测试".to_owned()],
                    50,
                ),
                occurred_at_ns: 0,
                mutation_identity: identity,
            })
        },
        "write:sparse-seed",
        0,
    );
    core.run_to_completion(seed, &mut host)
        .expect("seed write must complete");
    // Persist one pending turn so analysis can run against it.
    // 持久化一个待处理轮次，使分析可对其运行。
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-sparse")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![TimelineItem {
                            role: Role::User,
                            content: "我下周去北京出差，偏好 Rust 技术栈".to_owned(),
                            occurred_at_ns: 1,
                        }],
                        completed_at_ns: 1,
                        mutation_identity: identity,
                    })
                },
                "intake:sparse",
                1,
            ),
            &mut host,
        )
        .expect("intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        assert!(false, "intake must return its typed result");
        return;
    };
    let pending_turn = intake.turn_id.expect("intake must persist a pending turn");

    // Analysis drops the first of three candidates and the review accepts only the second
    // admitted candidate, leaving a gap the old accepted-array indexing could not cover.
    // 分析丢弃三个候选中的首个，审核仅接受第二个已准入候选，留下旧接受数组
    // 下标方式无法覆盖的空隙。
    host.scenario = HostScenario::SparseAcceptance;
    let analysis = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: SessionSourceId::new("session-sparse")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        turn_id: pending_turn,
                        analysis_generation: 1,
                        analysis_cutoff_ns: 2,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "analyze:sparse",
                2,
            ),
            &mut host,
        )
        .expect("sparse analysis must complete");
    let CommandResult::PostActionAnalysis(analysis) = analysis else {
        assert!(false, "analysis must return its typed result");
        return;
    };
    assert_eq!(
        analysis.accepted_memory_ids.len(),
        1,
        "only the second admitted candidate must be accepted"
    );
    // Exactly one evidence row survives: the one bound to the accepted candidate. The rows
    // for the L1-dropped and L2-rejected candidates must be filtered out.
    // 恰好一条证据行存活：绑定到已接受候选的那条。绑定到 L1 丢弃与 L2 拒绝
    // 候选的证据行必须被过滤。
    assert_eq!(
        analysis.context_evidence.len(),
        1,
        "evidence must only survive for the accepted candidate"
    );
    let evidence = &analysis.context_evidence[0];
    assert_eq!(
        evidence.memory_id, analysis.accepted_memory_ids[0],
        "evidence must bind to the single accepted memory"
    );
    assert_eq!(evidence.context_key, "city");
    assert_eq!(evidence.context_value, "beijing");
    assert_eq!(evidence.support_count, 1);
    // The receipt must bind the memory-1 slot to the same committed domain identity the
    // result exposes, proving the evidence mapping used the real commit and not a placeholder.
    // 收据必须将 memory-1 槽位绑定到结果暴露的同一已提交领域身份，证明证据映射
    // 使用真实提交而非占位符。
    let receipt = analysis.receipt.expect("analysis must return a receipt");
    let binding = receipt
        .entity_bindings
        .iter()
        .find(|binding| binding.entity_slot_id.as_str() == "memory-1")
        .expect("memory-1 slot binding must exist");
    assert_eq!(
        MemoryId::new(binding.domain_id.clone()).expect("bound id must be valid"),
        analysis.accepted_memory_ids[0],
        "the accepted memory identity must be the slot-bound domain id"
    );
}

/// Verifies an empty lexical channel cannot invent a rank boost that bypasses the replacement threshold.
/// 验证词法通道为空时不能凭空补排名分从而绕过更替阈值。
#[test]
fn replace_similarity_floor_does_not_inflate_vector_only_candidates() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // This unrelated memory has no lexical match; its single-channel score must stay below the floor.
    // 这条无关记忆不命中词法通道，其单通道分数必须保持低于阈值，而非按第一名抬成满分。
    let seed = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "完全无关的日常流水",
                    vec!["测试".to_owned()],
                    20,
                ),
                occurred_at_ns: 0,
                mutation_identity: identity,
            })
        },
        "write:below-floor",
        0,
    );
    let seed = core
        .run_to_completion(seed, &mut host)
        .expect("seed write must complete");
    let CommandResult::MemoryWrite(seed) = seed else {
        panic!("seed write must return its bound memory");
    };
    let below_floor_memory_id = seed.memory.id;
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-floor")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![TimelineItem {
                            role: Role::User,
                            content: "我偏好 Rust 技术栈".to_owned(),
                            occurred_at_ns: 1,
                        }],
                        completed_at_ns: 1,
                        mutation_identity: identity,
                    })
                },
                "intake:floor",
                1,
            ),
            &mut host,
        )
        .expect("intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        assert!(false, "intake must return its typed result");
        return;
    };
    let pending_turn = intake.turn_id.expect("intake must persist a pending turn");
    let review_calls_before_analysis = host.review_llm_calls;
    let analysis = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: SessionSourceId::new("session-floor")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        turn_id: pending_turn,
                        analysis_generation: 1,
                        analysis_cutoff_ns: 2,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "analyze:floor",
                2,
            ),
            &mut host,
        )
        .expect("analysis must complete");
    let CommandResult::PostActionAnalysis(analysis) = analysis else {
        assert!(false, "analysis must return its typed result");
        return;
    };
    assert!(
        !analysis.accepted_memory_ids.is_empty(),
        "the admitted candidate must still be accepted"
    );
    assert_eq!(
        host.review_llm_calls,
        review_calls_before_analysis + 1,
        "L2 still runs with low raw vector similarities"
    );
    assert_eq!(
        host.max_review_similar_count, 0,
        "an empty lexical channel cannot boost an unrelated vector candidate into the review"
    );
    // The below-floor seed must survive untouched; it was never superseded.
    // 低于下限的种子记忆必须原样存活；它从未被更替。
    let details = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                memory_id: below_floor_memory_id,
            }),
            &mut host,
        )
        .expect("memory details must complete");
    let CommandResult::MemoryDetails(details) = details else {
        assert!(false, "memory details must return its typed result");
        return;
    };
    assert!(
        details.is_some(),
        "the below-floor memory must remain in storage"
    );
}

/// Verifies every grouped batch-search query runs through the shared pipeline with exactly
/// its own single query embedding, so one group cannot absorb another query's candidates.
/// 验证每个分组批量搜索查询都以恰好自身的单条查询嵌入通过共享流水线，
/// 使一组不可能吸收其他查询的候选。
#[test]
fn batch_search_groups_are_query_exclusive() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed two memories with sharply distinct embeddings: one Rust-flavored and one Beijing.
    // 播种两条嵌入截然不同的记忆：一条 Rust 风格、一条北京风格。
    let mut seeded_ids = Vec::new();
    for (memory_id, content, at) in [
        (
            "memory-batch-excl-rust",
            "I prefer Rust for systems work",
            0,
        ),
        ("memory-batch-excl-beijing", "我下周去北京出差", 1),
    ] {
        let write = with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        content,
                        vec!["测试".to_owned()],
                        50,
                    ),
                    occurred_at_ns: at,
                    mutation_identity: identity,
                })
            },
            &format!("write:excl:{memory_id}"),
            at,
        );
        let written = core
            .run_to_completion(write, &mut host)
            .expect("seed write must complete");
        let CommandResult::MemoryWrite(result) = written else {
            panic!("seed write must return its bound memory");
        };
        seeded_ids.push(result.memory.id);
    }

    let (handle, step) = core
        .start(CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
            subject_id: subject_id.clone(),
            scope_access: scope_access.clone(),
            vector_space_id: vector_space_id.clone(),
            boundary: fixture_search_boundary(&scope_access),
            queries: vec![
                "rust systems".to_owned(),
                "北京 出差".to_owned(),
                "completely unrelated".to_owned(),
            ],
            top_k: 5,
            now_ns: 2,
        }))
        .expect("batch search must start");
    let mut step = step;
    // Batch embedding is shared, while every physical query remains independent.
    // 批量嵌入共用一次调用，但每个物理查询仍独立执行。
    let mut embedding_requests = 0;
    let mut vector_requests = 0;
    loop {
        match step {
            CommandStep::Complete(result) => {
                let CommandResult::MemorySearchBatch(groups) = *result else {
                    assert!(false, "batch search must return its typed groups");
                    return;
                };
                assert_eq!(groups.len(), 3, "one group per caller query");
                assert_eq!(
                    embedding_requests, 1,
                    "one batch prepares all query vectors"
                );
                assert_eq!(vector_requests, 3, "one physical read per unique query");
                for (index, group) in groups.iter().enumerate() {
                    assert_eq!(group.query_index, index as u32);
                }
                // The Rust query must rank the Rust memory first and the Beijing query must
                // rank the Beijing memory first; no group may be polluted by the other query.
                // Rust 查询必须将 Rust 记忆排在首位，北京查询必须将北京记忆排在首位；
                // 任何一组都不得被另一条查询污染。
                assert_eq!(
                    groups[0].hits.first().map(|hit| hit.memory.id.as_str()),
                    Some(seeded_ids[0].as_str()),
                    "group zero must rank the Rust memory from its own query"
                );
                assert_eq!(
                    groups[1].hits.first().map(|hit| hit.memory.id.as_str()),
                    Some(seeded_ids[1].as_str()),
                    "group one must rank the Beijing memory from its own query"
                );
                break;
            }
            CommandStep::Effects(mut effects) => {
                let pending = effects.pop().expect("pending effect must exist");
                if let HostEffect::Embedding(effect) = &pending.effect {
                    embedding_requests += 1;
                    assert_eq!(
                        effect.inputs.len(),
                        3,
                        "all unique query embeddings must be prepared together"
                    );
                }
                if let HostEffect::StorageQuery(StorageQuery::VectorCandidates {
                    queries,
                    query_embeddings,
                    ..
                }) = &pending.effect
                {
                    vector_requests += 1;
                    assert_eq!(queries.len(), 1, "physical groups must not be merged");
                    assert_eq!(
                        query_embeddings.len(),
                        1,
                        "each physical query keeps its own vector"
                    );
                }
                let result = host
                    .execute_effect(pending)
                    .expect("host effect must execute");
                step = core
                    .resume(&handle, result)
                    .expect("batch search must resume");
            }
        }
    }
    core.release_command(&handle)
        .expect("completed batch search must release");
}

/// Verifies the memory written by analysis carries its own candidate embedding, proving the
/// embedding batch is never reordered, dropped, or swapped across the L2 review path.
/// 验证分析写入的记忆携带其自身候选嵌入，证明嵌入批次在 L2 审核路径中
/// 从未被重排、丢弃或错位。
#[test]
fn analyzed_memory_keeps_its_own_embedding() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed one similar memory so the L2 review effect fires and the derived path is exercised.
    // 播种一条相似记忆使 L2 审核 Effect 触发并走派生路径。
    let seed = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "用户偏好 Rust 技术栈",
                    vec!["测试".to_owned()],
                    50,
                ),
                occurred_at_ns: 0,
                mutation_identity: identity,
            })
        },
        "write:fidelity-seed",
        0,
    );
    core.run_to_completion(seed, &mut host)
        .expect("seed write must complete");
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-fidelity")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![TimelineItem {
                            role: Role::User,
                            content: "我偏好 Rust 技术栈".to_owned(),
                            occurred_at_ns: 1,
                        }],
                        completed_at_ns: 1,
                        mutation_identity: identity,
                    })
                },
                "intake:fidelity",
                1,
            ),
            &mut host,
        )
        .expect("intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        assert!(false, "intake must return its typed result");
        return;
    };
    let pending_turn = intake.turn_id.expect("intake must persist a pending turn");
    let analysis = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: SessionSourceId::new("session-fidelity")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        turn_id: pending_turn,
                        analysis_generation: 1,
                        analysis_cutoff_ns: 2,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "analyze:fidelity",
                2,
            ),
            &mut host,
        )
        .expect("analysis must complete");
    let CommandResult::PostActionAnalysis(analysis) = analysis else {
        assert!(false, "analysis must return its typed result");
        return;
    };
    assert_eq!(
        analysis.accepted_memory_ids.len(),
        1,
        "the single admitted candidate must be accepted"
    );
    let accepted_id = analysis.accepted_memory_ids[0].clone();
    let details = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                memory_id: accepted_id.clone(),
            }),
            &mut host,
        )
        .expect("memory details must complete");
    let CommandResult::MemoryDetails(details) = details else {
        assert!(false, "memory details must return its typed result");
        return;
    };
    let memory = details.expect("accepted memory must exist");
    let embedding = memory
        .embedding
        .expect("accepted memory must carry its embedding");
    // Recompute the deterministic embedding of the stored summary; a swapped or reordered
    // batch would mismatch this exact vector.
    // 重算存储摘要的确定性嵌入；批次被错位或重排必然与此精确向量不匹配。
    let lowercase = memory.summary.to_lowercase();
    let length = u8::try_from(memory.summary.chars().count().min(100)).expect("bounded length");
    let mut expected = vec![0.0; 32];
    expected[..3].copy_from_slice(&[
        f32::from(lowercase.contains("rust")),
        f32::from(lowercase.contains("prefer") || memory.summary.contains("喜欢")),
        f32::from(length) / 100.0,
    ]);
    assert_eq!(
        embedding, expected,
        "the accepted memory must keep the embedding of its own summary"
    );
}

/// Verifies partial and all-dropped persistence embedding outcomes survive exact checkpoint restore.
/// 验证部分成功与全部丢弃的持久化嵌入结果能精确恢复检查点。
#[test]
fn analysis_persistence_embedding_partitions_survive_checkpoint_restore() {
    use vulcan_memorymesh_core::{
        EmbeddingBatchResult, EmbeddingDropReason, EmbeddingDroppedInput,
    };
    for dropped_indexes in [vec![], vec![0], vec![0, 1]] {
        let mut core = MemoryMeshCore::new().expect("Core initializes");
        let (subject_id, scope_access, vector_space_id) = identities();
        let session_source_id = test_direct_session();
        let mut host = DeterministicHost::new();
        let seed_command = with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    occurred_at_ns: 0,
                    mutation_identity: identity,
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::SecurityPolicy,
                        "Rust archive security policy",
                        Vec::new(),
                        20,
                    ),
                })
            },
            "seed:embedding-partition",
            0,
        );
        let CommandResult::MemoryWrite(seed) = core
            .run_to_completion(seed_command, &mut host)
            .expect("supersede seed writes")
        else {
            panic!("write result")
        };
        let seed_id = seed.memory.id;
        let prior_review_calls = host.review_llm_calls;
        let intake = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionIntake(PostActionIntakeCommand {
                    input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    timeline: vec![TimelineItem {
                        role: Role::User,
                        content: "  I prefer Ｒｕｓｔ\n    for reliable systems  ".to_owned(),
                        occurred_at_ns: 1,
                    }],
                    completed_at_ns: 1,
                    mutation_identity: identity,
                })
            },
            "intake:embedding-partition",
            1,
        );
        let CommandResult::PostActionAccepted(intake) = core
            .run_to_completion(intake, &mut host)
            .expect("intake completes")
        else {
            panic!("intake result")
        };
        let turn_id = intake.turn_id.expect("pending turn exists");
        assert_eq!(
            host.storage.dataset().turns[0].timeline[0].content,
            "I prefer Ｒｕｓｔ\n    for reliable systems"
        );
        let command = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    profile_sections: vec![subject_profile_section(&scope_access)],
                    session_source_id: session_source_id.clone(),
                    vector_space_id: vector_space_id.clone(),
                    turn_id: turn_id.clone(),
                    analysis_generation: 1,
                    analysis_cutoff_ns: 2,
                    direct_write_observed_after_ns: 0,
                    mutation_identity: identity,
                })
            },
            "analyze:embedding-partition",
            2,
        );
        let (mut handle, mut step) = core.start(command).expect("analysis starts");
        let mut persistence_calls = 0;
        let mut query_inputs = Vec::new();
        let mut outcome = None;
        for _ in 0..80 {
            let CommandStep::Effects(mut effects) = step else {
                if let CommandStep::Complete(result) = step {
                    outcome = Some(*result);
                    break;
                }
                panic!("analysis must wait for an effect or complete");
            };
            let mut request = effects.pop().expect("single effect");
            if let HostEffect::Embedding(effect) = &request.effect {
                if effect.allow_partial_invalid_inputs {
                    persistence_calls += 1;
                    assert_eq!(effect.inputs, vec!["Prefers Rust", "Beijing residence"]);
                    assert_eq!(
                        host.review_llm_calls,
                        prior_review_calls + 1,
                        "persistence is after L2"
                    );
                    let checkpoint = core
                        .export_checkpoint(&handle)
                        .expect("persistence checkpoint exports");
                    let restored = MemoryMeshCore::new().expect("fresh Core initializes");
                    let (restored_handle, restored_step) = restored
                        .restore_checkpoint(&checkpoint)
                        .expect("persistence state restores");
                    let CommandStep::Effects(mut restored_effects) = restored_step else {
                        panic!("restored embedding effect")
                    };
                    let restored_request = restored_effects.pop().expect("restored effect");
                    assert_eq!(
                        restored_request, request,
                        "restore does not repeat L1 or L2"
                    );
                    core = restored;
                    handle = restored_handle;
                    request = restored_request;
                    let HostEffect::Embedding(effect) = &request.effect else {
                        panic!("embedding")
                    };
                    let all_vectors = DeterministicHost::execute_embedding(effect);
                    let kept = (0..2_u32)
                        .rev()
                        .filter(|index| !dropped_indexes.contains(index))
                        .collect::<Vec<_>>();
                    let result = EmbeddingBatchResult {
                        vectors: kept
                            .iter()
                            .map(|index| all_vectors[*index as usize].clone())
                            .collect(),
                        result_indices: kept,
                        dropped: dropped_indexes
                            .iter()
                            .map(|index| EmbeddingDroppedInput {
                                input_index: *index,
                                reason: EmbeddingDropReason::InputTooLarge,
                            })
                            .collect(),
                    };
                    step = core
                        .resume(
                            &handle,
                            EffectResult {
                                effect_id: request.effect_id,
                                result: HostEffectResult::Embedding(result),
                            },
                        )
                        .expect("indexed persistence results resume");
                    continue;
                }
                query_inputs.extend(effect.inputs.clone());
            }
            let mut result = host.execute_effect(request).expect("host effect succeeds");
            if let HostEffectResult::Llm(LlmOutput::PostActionAnalysis {
                details,
                memories,
                profiles,
                context_evidence,
                ..
            }) = &mut result.result
            {
                *details = "  Analysis Ａ\n    preserved details  ".to_owned();
                profiles[0].content = "  Prefers Ｒｕｓｔ\n    with precise layout  ".to_owned();
                memories[0].content = "User preference:\n    Ｒｕｓｔ systems".to_owned();
                memories.push(MemoryCandidate {
                    category: MemoryCategory::General,
                    summary: "Beijing residence".to_owned(),
                    content: "Lives in Beijing near the university".to_owned(),
                    evidence_source: "user_asserted".to_owned(),
                    admission: Admission::Accept,
                    admission_reason: None,
                });
                for index in 0..2 {
                    context_evidence.push(ContextEvidenceCandidate {
                        candidate_index: index,
                        context_key: "source".to_owned(),
                        context_value: format!("candidate-{index}"),
                        relation: ContextEvidenceRelation::Support,
                    });
                }
            }
            if let HostEffectResult::Llm(LlmOutput::PostActionReview {
                accepted_candidates,
                ..
            }) = &mut result.result
            {
                accepted_candidates[0].supersede_memory_ids = vec![seed_id.to_string()];
            }
            step = core
                .resume(&handle, result)
                .expect("analysis effect resumes");
        }
        let CommandResult::PostActionAnalysis(analysis) =
            outcome.expect("bounded analysis completes")
        else {
            panic!("analysis result")
        };
        assert_eq!(persistence_calls, 1);
        assert!(
            query_inputs
                .iter()
                .any(|text| text.contains("Prefers Rust User preference:"))
        );
        assert_eq!(
            analysis.accepted_memory_ids.len(),
            2 - dropped_indexes.len()
        );
        assert_eq!(
            analysis
                .embedding_dropped_candidates
                .iter()
                .map(|row| row.input_index)
                .collect::<Vec<_>>(),
            dropped_indexes
        );
        assert_eq!(
            analysis.context_evidence.len(),
            analysis.accepted_memory_ids.len()
        );
        assert_eq!(
            analysis.accepted_profile_nodes.len(),
            1,
            "profile survives all memory drops"
        );
        assert_eq!(
            analysis.accepted_profile_nodes[0].content,
            "Prefers Ｒｕｓｔ\n    with precise layout"
        );
        assert_eq!(
            host.storage.dataset().turns[0].analysis_details.as_deref(),
            Some("Analysis Ａ\n    preserved details")
        );
        for memory in &host.storage.dataset().memories {
            if memory.summary == "Prefers Rust" {
                assert_eq!(memory.content, "User preference:\n    Ｒｕｓｔ systems");
            }
        }
        let seed_after = host
            .storage
            .dataset()
            .memories
            .into_iter()
            .find(|memory| memory.id == seed_id)
            .expect("seed remains auditable");
        if dropped_indexes.contains(&0) {
            assert_eq!(
                seed_after.state,
                MemoryState::Active,
                "rejected replacement never retires its target"
            );
            assert!(analysis.superseded_memory_ids.is_empty());
        } else {
            assert_eq!(seed_after.state, MemoryState::Superseded);
            assert_eq!(analysis.superseded_memory_ids, vec![seed_id]);
        }
        assert_eq!(
            host.storage.dataset().turns[0].status,
            vulcan_memorymesh_core::TurnStatus::Completed
        );
        for memory in &host.storage.dataset().memories {
            let expected = DeterministicHost::execute_embedding(&EmbeddingEffect {
                inputs: vec![memory.summary.clone()],
                vector_space_id: vector_space_id.clone(),
                expected_dimension: 32,
                allow_partial_invalid_inputs: false,
            });
            assert_eq!(
                memory.embedding.as_ref(),
                expected.first(),
                "sparse results bind to their own summary"
            );
        }
    }
}

/// Verifies local Analyze hard dedupe skips L2 only for category-matching cosine duplicates.
/// 验证本地 Analyze 硬去重仅对类别匹配的余弦重复项跳过 L2。
#[test]
fn analyze_hard_dedupe_is_local_and_category_scoped() {
    for (same_category, lexical_only, fail_review, threshold) in [
        (true, false, false, 0.99),
        (false, false, false, 0.99),
        (false, true, false, 0.99),
        (true, false, true, 0.99),
        (true, false, false, 0.0),
    ] {
        let core = MemoryMeshCore::new().expect("Core initializes");
        // policy exercises explicit hard-dedupe disablement through the same public analysis flow.
        // policy 通过同一公共分析流程验证显式关闭硬去重。
        let mut policy = vulcan_memorymesh_core::ConfigBundle {
            name: "analysis-hard-dedupe-policy".to_owned(),
            ..Default::default()
        };
        policy.memory_policy.retrieval_policy.hard_dedupe[0].threshold = threshold;
        core.register_config(policy)
            .expect("register dedupe policy");
        core.activate_config("analysis-hard-dedupe-policy")
            .expect("activate dedupe policy");
        let (subject_id, scope_access, vector_space_id) = identities();
        let mut host = DeterministicHost::new();
        let long_text = format!(
            "I prefer Rust {}",
            "long stable technical detail ".repeat(8)
        );
        let seed_command = with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    occurred_at_ns: 0,
                    mutation_identity: identity,
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        if same_category {
                            MemoryCategory::ProjectContext
                        } else {
                            MemoryCategory::General
                        },
                        &long_text,
                        Vec::new(),
                        20,
                    ),
                })
            },
            "seed:local-hard-dedupe",
            0,
        );
        let CommandResult::MemoryWrite(seed) = core
            .run_to_completion(seed_command, &mut host)
            .expect("seed writes")
        else {
            panic!("write result")
        };
        let prior_reviews = host.review_llm_calls;
        let intake = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionIntake(PostActionIntakeCommand {
                    input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    timeline: vec![TimelineItem {
                        role: Role::User,
                        content: long_text,
                        occurred_at_ns: 1,
                    }],
                    completed_at_ns: 1,
                    mutation_identity: identity,
                })
            },
            "intake:local-hard-dedupe",
            1,
        );
        let CommandResult::PostActionAccepted(intake) =
            core.run_to_completion(intake, &mut host).expect("intake")
        else {
            panic!("intake result")
        };
        let command = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    profile_sections: vec![subject_profile_section(&scope_access)],
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    turn_id: intake.turn_id.expect("turn"),
                    analysis_generation: 1,
                    analysis_cutoff_ns: 2,
                    direct_write_observed_after_ns: 0,
                    mutation_identity: identity,
                })
            },
            "analyze:local-hard-dedupe",
            2,
        );
        let (handle, mut step) = core.start(command).expect("analysis starts");
        let mut persistence_calls = 0;
        let mut outcome = None;
        for _ in 0..40 {
            let CommandStep::Effects(mut effects) = step else {
                if let CommandStep::Complete(result) = step {
                    outcome = Some(*result);
                    break;
                }
                panic!("unexpected analysis state");
            };
            let request = effects.pop().expect("effect");
            if matches!(&request.effect, HostEffect::Embedding(effect) if effect.allow_partial_invalid_inputs)
            {
                persistence_calls += 1;
            }
            let mut result = host.execute_effect(request).expect("host effect");
            if let HostEffectResult::Llm(LlmOutput::PostActionAnalysis { profiles, .. }) =
                &mut result.result
                && !fail_review
            {
                profiles.clear();
            }
            if lexical_only
                && let HostEffectResult::StorageQuery(StorageQueryResult::VectorCandidates(
                    candidates,
                )) = &mut result.result
            {
                candidates.candidates.clear();
                candidates.memories.clear();
            }
            if fail_review
                && matches!(
                    &result.result,
                    HostEffectResult::Llm(LlmOutput::PostActionReview { .. })
                )
            {
                result.result = HostEffectResult::Failure(EffectFailure {
                    kind: EffectFailureKind::Llm,
                    class: FailureClass::Provider,
                    retryable: false,
                    allow_degrade: true,
                    diagnostic: "injected profile review failure after local memory dedupe"
                        .to_owned(),
                });
            }
            step = core.resume(&handle, result).expect("resume");
        }
        let CommandResult::PostActionAnalysis(analysis) = outcome.expect("analysis completes")
        else {
            panic!("analysis result")
        };
        if same_category && !fail_review && threshold > 0.0 {
            assert!(analysis.accepted_memory_ids.is_empty());
            assert_eq!(analysis.dropped_candidates.len(), 1);
            assert_eq!(
                analysis.dropped_candidates[0].dedupe_memory_id.as_deref(),
                Some(seed.memory.id.as_str())
            );
            assert_eq!(
                host.review_llm_calls,
                prior_reviews + u32::from(fail_review),
                "only remaining profile candidates may trigger L2 after local memory dedupe"
            );
            assert_eq!(
                persistence_calls, 0,
                "a reused memory does not need a new vector"
            );
        } else {
            assert_eq!(analysis.accepted_memory_ids.len(), 1);
            assert!(analysis.dropped_candidates.is_empty());
            assert_eq!(host.review_llm_calls, prior_reviews + 1);
            assert_eq!(persistence_calls, 1);
            if lexical_only {
                assert!(
                    host.max_review_similar_count > 0,
                    "valid lexical-only evidence reaches L2"
                );
            }
        }
        assert_eq!(
            analysis.accepted_profile_nodes.len(),
            usize::from(fail_review),
            "review failure restores pre-review L1 memories and pending profile evidence"
        );
    }
}

/// Verifies direct writes request distinct query and summary vectors and reject partial persistence.
/// 验证主动写入请求不同的查询与摘要向量，并拒绝部分持久化。
#[test]
fn direct_persistence_embedding_is_strict_and_checkpointed() {
    use vulcan_memorymesh_core::{
        EmbeddingBatchResult, EmbeddingDropReason, EmbeddingDroppedInput,
    };
    for reject_persistence in [false, true] {
        let mut core = MemoryMeshCore::new().expect("Core initializes");
        let (subject_id, scope_access, vector_space_id) = identities();
        let mut host = DeterministicHost::new();
        let command = with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    occurred_at_ns: 10,
                    mutation_identity: identity,
                    item: MemoryWriteBatchItem {
                        target_scope_id: scope_access.write_scope.clone(),
                        scope_class: MemoryScopeClass::Environment,
                        category: MemoryCategory::General,
                        summary: "  Ａ abstract\nsecond line  ".to_owned(),
                        details: "  def run():\n    return \"Ａ\"  ".to_owned(),
                        priority: Some(20),
                        memory_level: Some(MemoryLevel::Stable),
                        expires_at_ns: None,
                        tags: Vec::new(),
                    },
                })
            },
            "write:distinct-embedding",
            10,
        );
        let (mut handle, mut step) = core.start(command).expect("write starts");
        let mut embedding_calls = 0;
        let mut completed = false;
        for _ in 0..40 {
            let CommandStep::Effects(mut effects) = step else {
                assert!(matches!(step, CommandStep::Complete(_)));
                completed = true;
                break;
            };
            let request = effects.pop().expect("single request");
            let result = if let HostEffect::Embedding(effect) = &request.effect {
                embedding_calls += 1;
                assert!(
                    !effect.allow_partial_invalid_inputs,
                    "direct writes are strict at every embedding stage"
                );
                assert_eq!(
                    effect.inputs,
                    if embedding_calls == 1 {
                        vec!["Ａ abstract second line def run(): return \"Ａ\""]
                    } else {
                        vec!["Ａ abstract\nsecond line"]
                    }
                );
                if embedding_calls == 2 {
                    assert_eq!(host.review_llm_calls, 1);
                    let checkpoint = core
                        .export_checkpoint(&handle)
                        .expect("direct persistence checkpoint");
                    let restored = MemoryMeshCore::new().expect("fresh Core");
                    let (restored_handle, restored_step) = restored
                        .restore_checkpoint(&checkpoint)
                        .expect("direct persistence restores");
                    let CommandStep::Effects(restored_effects) = restored_step else {
                        panic!("restored request")
                    };
                    assert_eq!(restored_effects, vec![request.clone()]);
                    core = restored;
                    handle = restored_handle;
                    if reject_persistence {
                        let error = core
                            .resume(
                                &handle,
                                EffectResult {
                                    effect_id: request.effect_id,
                                    result: HostEffectResult::Embedding(EmbeddingBatchResult {
                                        vectors: Vec::new(),
                                        result_indices: Vec::new(),
                                        dropped: vec![EmbeddingDroppedInput {
                                            input_index: 0,
                                            reason: EmbeddingDropReason::InputTooLarge,
                                        }],
                                    }),
                                },
                            )
                            .expect_err("strict persistence refuses partial success");
                        assert_eq!(error.code, ErrorCode::EffectMismatch);
                        assert!(host.storage.dataset().memories.is_empty());
                        break;
                    }
                }
                let mut vector = vec![0.0; effect.expected_dimension as usize];
                vector[embedding_calls - 1] = 1.0;
                EffectResult {
                    effect_id: request.effect_id,
                    result: HostEffectResult::Embedding(vec![vector].into()),
                }
            } else {
                host.execute_effect(request).expect("host effect")
            };
            step = core.resume(&handle, result).expect("write resumes");
        }
        assert_eq!(embedding_calls, 2);
        assert_eq!(completed, !reject_persistence);
        if !reject_persistence {
            let before = host.storage.dataset().memories[0].clone();
            assert_eq!(before.summary, "Ａ abstract\nsecond line");
            assert_eq!(before.content, "def run():\n    return \"Ａ\"");
            let stored = host.storage.dataset().memories[0]
                .embedding
                .clone()
                .expect("stored vector");
            let mut expected = vec![0.0; 32];
            expected[1] = 1.0;
            assert_eq!(stored, expected, "the query vector must never be persisted");
            // Maintenance must embed the same durable summary representation as creation.
            // 维护重建必须嵌入与新建时相同的持久摘要表示。
            let rebuild = with_identity(
                &core,
                |identity| {
                    CoreCommand::Maintenance(MaintenanceCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        occurred_at_ns: 11,
                        action: MaintenanceAction::RebuildVectors {
                            memory_ids: vec![before.id.clone()],
                            vector_space_id: vector_space_id.clone(),
                        },
                        mutation_identity: identity,
                    })
                },
                "rebuild:body-preservation",
                11,
            );
            let (rebuild_handle, mut rebuild_step) = core.start(rebuild).expect("rebuild starts");
            let mut rebuild_embeddings = 0;
            for _ in 0..10 {
                let CommandStep::Effects(mut requests) = rebuild_step else {
                    break;
                };
                let request = requests.pop().expect("one rebuild effect");
                let result = if let HostEffect::Embedding(effect) = &request.effect {
                    rebuild_embeddings += 1;
                    assert_eq!(effect.inputs, vec![before.summary.clone()]);
                    assert!(!effect.allow_partial_invalid_inputs);
                    EffectResult {
                        effect_id: request.effect_id,
                        result: HostEffectResult::Embedding(vec![expected.clone()].into()),
                    }
                } else {
                    host.execute_effect(request).expect("rebuild host effect")
                };
                rebuild_step = core
                    .resume(&rebuild_handle, result)
                    .expect("rebuild resumes");
            }
            assert_eq!(rebuild_embeddings, 1);
            assert!(matches!(rebuild_step, CommandStep::Complete(_)));
            let after = host.storage.dataset().memories[0].clone();
            assert_eq!(after.summary, before.summary);
            assert_eq!(after.content, before.content);
            assert_eq!(after.embedding, Some(expected));
        }
    }
}

/// Verifies a mixed direct batch embeds only unresolved inputs without shifting caller rows.
/// 验证混合主动批次仅嵌入未解析输入且不移动调用方结果行。
#[test]
fn mixed_direct_batch_reuses_existing_rows_without_embedding_them() {
    let core = MemoryMeshCore::new().expect("Core initializes");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let existing_item = direct_write_item(
        scope_access.write_scope.clone(),
        MemoryCategory::ProjectContext,
        "Rust systems",
        Vec::new(),
        20,
    );
    let seed = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                occurred_at_ns: 10,
                item: existing_item.clone(),
                mutation_identity: identity,
            })
        },
        "seed:mixed-batch",
        10,
    );
    let CommandResult::MemoryWrite(seed) = core
        .run_to_completion(seed, &mut host)
        .expect("seed writes")
    else {
        panic!("write result")
    };
    let new_item = MemoryWriteBatchItem {
        target_scope_id: scope_access.write_scope.clone(),
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::ArchitectureDecision,
        summary: "Go network".to_owned(),
        details: "Distinct networking architecture".to_owned(),
        priority: Some(20),
        memory_level: Some(MemoryLevel::Stable),
        expires_at_ns: None,
        tags: Vec::new(),
    };
    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id,
                occurred_at_ns: 11,
                authorized_write_scopes: vec![scope_access.write_scope.clone()],
                items: vec![existing_item, new_item],
                mutation_identity: identity,
            })
        },
        "write:mixed-batch",
        11,
    );
    let (handle, mut step) = core.start(command).expect("mixed batch starts");
    let mut embedded_inputs = Vec::new();
    for _ in 0..40 {
        let CommandStep::Effects(mut effects) = step else {
            break;
        };
        let request = effects.pop().expect("one request");
        if let HostEffect::Embedding(effect) = &request.effect {
            assert!(!effect.allow_partial_invalid_inputs);
            embedded_inputs.push(effect.inputs.clone());
        }
        let result = host.execute_effect(request).expect("host effect");
        step = core.resume(&handle, result).expect("mixed batch resumes");
    }
    assert_eq!(
        embedded_inputs,
        vec![
            vec!["Go network Distinct networking architecture"],
            vec!["Go network"]
        ]
    );
    let CommandStep::Complete(result) = step else {
        panic!("mixed batch completes")
    };
    let CommandResult::MemoryWriteBatch(result) = *result else {
        panic!("batch result")
    };
    assert_eq!(result.deduplicated, vec![true, false]);
    assert_eq!(
        result.memories[0], seed.memory,
        "reused row is not recreated or re-embedded"
    );
    assert_eq!(result.memories[1].summary, "Go network");
    assert_eq!(
        result
            .receipt
            .expect("new row receipt")
            .entity_bindings
            .len(),
        1
    );
}

/// Verifies terminal, cancelled, and failed commands refuse checkpoint export.
/// 验证已终态、已取消与已失败命令拒绝检查点导出。
#[test]
fn terminal_and_cancelled_checkpoints_refuse_export() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Complete one command, then refuse its checkpoint export.
    // 完成一个命令，然后拒绝其检查点导出。
    let (write_handle, write_step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "terminal export",
                        Vec::new(),
                        50,
                    ),
                    occurred_at_ns: 1,
                    mutation_identity: identity,
                })
            },
            "write:terminal-export",
            1,
        ))
        .expect("write must start");
    let mut write_step = write_step;
    loop {
        match write_step {
            CommandStep::Complete(_) => break,
            CommandStep::Effects(mut effects) => {
                let pending = effects.pop().expect("pending effect must exist");
                let result = host
                    .execute_effect(pending)
                    .expect("host effect must execute");
                write_step = core
                    .resume(&write_handle, result)
                    .expect("write must resume");
            }
        }
    }
    let error = core
        .export_checkpoint(&write_handle)
        .expect_err("completed command must refuse checkpoint export");
    assert_eq!(error.code, ErrorCode::CommandCompleted);
    core.release_command(&write_handle)
        .expect("completed write must release");

    // Cancel one waiting command, then refuse its checkpoint export.
    // 取消一个等待命令，然后拒绝其检查点导出。
    let (handle, step) = core
        .start(with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "cancelled export",
                        Vec::new(),
                        50,
                    ),
                    occurred_at_ns: 2,
                    mutation_identity: identity,
                })
            },
            "write:cancelled-export",
            2,
        ))
        .expect("write must start");
    assert!(matches!(step, CommandStep::Effects(_)));
    core.cancel(&handle).expect("waiting command must cancel");
    let error = core
        .export_checkpoint(&handle)
        .expect_err("cancelled command must refuse checkpoint export");
    assert_eq!(error.code, ErrorCode::CommandCancelled);

    // A definitive read failure is terminal too; it must not produce a resumable checkpoint.
    // 确定性的读取失败同样是终态，不得生成可恢复检查点。
    let (failed_handle, failed_step) = core
        .start(CoreCommand::MemoryDetails(MemoryDetailsCommand {
            subject_id,
            scope_access,
            memory_id: MemoryId::new("failed-checkpoint-record").expect("valid memory ID"),
        }))
        .expect("details starts");
    let CommandStep::Effects(mut failed_effects) = failed_step else {
        panic!("read effect")
    };
    let failed_request = failed_effects.pop().expect("pending read");
    core.resume(
        &failed_handle,
        EffectResult {
            effect_id: failed_request.effect_id,
            result: HostEffectResult::Failure(EffectFailure {
                kind: EffectFailureKind::StorageQuery,
                class: FailureClass::Transport,
                retryable: false,
                allow_degrade: false,
                diagnostic: "definitive read failure".to_owned(),
            }),
        },
    )
    .expect_err("required read failure must terminate");
    assert_eq!(
        core.command_diagnostics(&failed_handle)
            .expect("failed diagnostics")
            .status,
        CommandStatus::Failed
    );
    assert_eq!(
        core.export_checkpoint(&failed_handle)
            .expect_err("failed command cannot export")
            .code,
        ErrorCode::CommandCompleted
    );
    core.release_command(&failed_handle)
        .expect("failed command releases");
}

/// Isolates stage/recovery tests from the independently verified default reviewer score floor.
/// 将阶段及恢复测试与独立验证的默认审核评分下限分离，确保夹具确实进入被测分支。
fn configure_precheck_stage_fixture(core: &MemoryMeshCore) -> vulcan_memorymesh_core::ConfigBundle {
    let mut bundle = vulcan_memorymesh_core::ConfigBundle {
        name: "precheck-stage-fixture".to_owned(),
        ..vulcan_memorymesh_core::ConfigBundle::default()
    };
    bundle.memory_policy.precheck_policy.min_review_score = 0.0;
    core.register_config(bundle.clone())
        .expect("register stage fixture");
    core.activate_config(&bundle.name)
        .expect("activate stage fixture");
    bundle
}

/// Verifies public single and grouped searches preserve exact Host-resolved scope and session boundaries in physical effects.
/// 验证公共单条及分组搜索在物理效果中精确保留 Host 解析的作用域与会话边界。
#[test]
fn public_searches_propagate_explicit_candidate_boundaries() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let candidate_scope = fixture_candidate_scope(&scope_access);
    let boundary = vulcan_memorymesh_core::CandidateQueryBoundary {
        candidate_scope: Some(candidate_scope),
        session_recall_boundary: Some(vulcan_memorymesh_core::SessionRecallBoundary {
            session_source_id: SessionSourceId::new("session-explicit-boundary")
                .expect("session identity must be valid"),
            mode: SessionRecallMode::Full,
        }),
    };
    let mut host = DeterministicHost::new();

    // Drive each public command through embedding and stop at its first physical candidate read.
    // 驱动每条公共命令通过嵌入阶段，并停在首次物理候选读取处。
    let commands = [
        CoreCommand::MemorySearch(MemorySearchCommand {
            subject_id: subject_id.clone(),
            scope_access: scope_access.clone(),
            query: "explicit boundary".to_owned(),
            limit: 2,
            vector_space_id: vector_space_id.clone(),
            boundary: boundary.clone(),
            now_ns: 10,
        }),
        CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
            subject_id: subject_id.clone(),
            scope_access: scope_access.clone(),
            queries: vec!["first boundary".to_owned(), "second boundary".to_owned()],
            top_k: 2,
            vector_space_id: vector_space_id.clone(),
            boundary: boundary.clone(),
            now_ns: 10,
        }),
    ];
    for command in commands {
        let (handle, step) = core.start(command).expect("public search must start");
        let CommandStep::Effects(mut effects) = step else {
            panic!("public search must request embeddings first");
        };
        let embedding = effects.pop().expect("embedding effect must exist");
        let embedding_result = host
            .execute_effect(embedding)
            .expect("embedding effect must succeed");
        let CommandStep::Effects(mut effects) = core
            .resume(&handle, embedding_result)
            .expect("embedding result must resume")
        else {
            panic!("public search must request physical candidates");
        };
        let candidate_read = effects.pop().expect("candidate effect must exist");
        let HostEffect::StorageQuery(StorageQuery::VectorCandidates {
            boundary: actual, ..
        }) = candidate_read.effect
        else {
            panic!("public search must first request vector candidates");
        };
        assert_eq!(actual, boundary);
    }

    let error = core
        .start(CoreCommand::MemorySearch(MemorySearchCommand {
            subject_id,
            scope_access,
            query: "unauthorized boundary".to_owned(),
            limit: 1,
            vector_space_id,
            boundary: vulcan_memorymesh_core::CandidateQueryBoundary {
                candidate_scope: Some(vulcan_memorymesh_core::MemoryCandidateScope {
                    scope_ids: vec![ScopeId::new("scope-not-visible").expect("valid scope")],
                    origin_session_source_id: None,
                }),
                session_recall_boundary: None,
            },
            now_ns: 10,
        }))
        .expect_err("candidate scope outside the read grant must fail");
    assert_eq!(error.code, ErrorCode::InvalidArgument);
}

/// Verifies a `PreCheck` command restores in a fresh Core instance at a deep mid-search-pipeline
/// position and continues to the same final result.
/// 验证 `PreCheck` 命令在一个全新 Core 实例中的深层中途检索流水线位置恢复，
/// 并继续得到相同最终结果。
#[test]
fn precheck_restores_mid_search_pipeline_in_fresh_instance() {
    let first = MemoryMeshCore::new().expect("Core must initialize");
    let precheck_config = configure_precheck_stage_fixture(&first);
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed one directly relevant memory so the search pipeline produces candidates.
    // 播种一条直接相关的主动记忆，使检索流水线产生候选。
    let seed = with_identity(
        &first,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "用户偏好 Rust 系统编程",
                    vec!["测试".to_owned()],
                    80,
                ),
                occurred_at_ns: 0,
                mutation_identity: identity,
            })
        },
        "write:precheck-continue",
        0,
    );
    first
        .run_to_completion(seed, &mut host)
        .expect("seed write must complete");

    let (handle, step) = first
        .start(with_identity(
            &first,
            |identity| {
                CoreCommand::PreCheck(PreCheckCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    session_source_id: SessionSourceId::new("session-precheck-continue")
                        .expect("session identity must be valid"),
                    vector_space_id: vector_space_id.clone(),
                    candidate_scope: fixture_candidate_scope(&scope_access),
                    user_content: "What do I prefer for systems work?".to_owned(),
                    recent_turns: Vec::new(),
                    recall_mode: RecallMode::Auto,
                    session_recall_mode: SessionRecallMode::CompactAware,
                    max_search_queries: 3,
                    max_memories: 3,
                    now_ns: 1,
                    mutation_identity: identity,
                })
            },
            "precheck:continue",
            1,
        ))
        .expect("PreCheck must start");
    // Drive through the L1 intent and query-embedding effects, stopping at the vector search
    // stage inside the shared retrieval pipeline.
    // 驱动通过 L1 意图与查询嵌入两个 Effect，停在共享检索流水线内的向量搜索阶段。
    let CommandStep::Effects(mut intent_effects) = step else {
        assert!(false, "PreCheck must first request intent");
        return;
    };
    let intent_result = host
        .execute_effect(intent_effects.pop().expect("intent effect must exist"))
        .expect("intent effect must succeed");
    let CommandStep::Effects(mut embedding_effects) = first
        .resume(&handle, intent_result)
        .expect("intent resume must succeed")
    else {
        assert!(false, "PreCheck must next request embeddings");
        return;
    };
    let embedding_result = host
        .execute_effect(
            embedding_effects
                .pop()
                .expect("embedding effect must exist"),
        )
        .expect("embedding effect must succeed");
    let CommandStep::Effects(mut search_effects) = first
        .resume(&handle, embedding_result)
        .expect("embedding resume must succeed")
    else {
        assert!(false, "PreCheck must next enter the search pipeline");
        return;
    };
    let pending_search = search_effects
        .pop()
        .expect("vector search effect must exist");
    assert!(
        matches!(
            pending_search.effect,
            HostEffect::StorageQuery(StorageQuery::VectorCandidates { .. })
        ),
        "checkpoint must actually be inside the retrieval pipeline"
    );
    let HostEffect::StorageQuery(StorageQuery::VectorCandidates {
        boundary: pending_boundary,
        ..
    }) = &pending_search.effect
    else {
        panic!("checkpoint must preserve a vector candidate request");
    };
    assert_eq!(
        pending_boundary.candidate_scope.as_ref(),
        Some(&fixture_candidate_scope(&scope_access))
    );
    assert_eq!(
        pending_boundary.session_recall_boundary.as_ref(),
        Some(&vulcan_memorymesh_core::SessionRecallBoundary {
            session_source_id: SessionSourceId::new("session-precheck-continue")
                .expect("session identity must be valid"),
            mode: SessionRecallMode::CompactAware,
        })
    );
    let checkpoint = first
        .export_checkpoint(&handle)
        .expect("mid-pipeline checkpoint must export");

    // A fresh Core restores the exact pending vector search and continues to completion.
    // 全新 Core 恢复精确待处理向量搜索并继续到完成。
    let restored = MemoryMeshCore::new().expect("restored Core must initialize");
    restored
        .register_config(precheck_config)
        .expect("restore original PreCheck fixture snapshot");
    let (restored_handle, restored_step) = restored
        .restore_checkpoint(&checkpoint)
        .expect("mid-pipeline checkpoint must restore");
    let CommandStep::Effects(mut restored_effects) = restored_step else {
        assert!(false, "restored command must re-issue its pending search");
        return;
    };
    let restored_search = restored_effects
        .pop()
        .expect("restored search effect must exist");
    assert_eq!(
        restored_search.effect_id, pending_search.effect_id,
        "restored command must resume at the exact pending search effect"
    );
    assert_eq!(&restored_search.effect, &pending_search.effect);
    let mut step = restored
        .resume(
            &restored_handle,
            host.execute_effect(restored_search)
                .expect("restored search must execute"),
        )
        .expect("restored search must resume");
    // Drive every remaining effect to completion.
    loop {
        match step {
            CommandStep::Complete(result) => {
                let CommandResult::PreCheck(precheck) = result.as_ref() else {
                    assert!(false, "restored PreCheck must return its typed result");
                    return;
                };
                assert!(
                    precheck.should_inject,
                    "restored PreCheck must inject the recalled context"
                );
                assert!(
                    precheck.context_text.contains("Rust"),
                    "restored PreCheck must keep the recalled context"
                );
                assert_eq!(precheck.queries, vec!["What do I prefer for systems work?"]);
                break;
            }
            CommandStep::Effects(mut effects) => {
                let pending = effects.pop().expect("pending effect must exist");
                if let HostEffect::Llm(LlmEffect {
                    input:
                        LlmInput::PreCheckReview {
                            current_timestamp_ns,
                            search_queries,
                            intent_reason,
                            ..
                        },
                    ..
                }) = &pending.effect
                {
                    assert_eq!(*current_timestamp_ns, 1);
                    assert_eq!(search_queries, &vec!["What do I prefer for systems work?"]);
                    assert_eq!(
                        intent_reason.as_deref(),
                        Some("deterministic intent classification")
                    );
                }
                let result = host
                    .execute_effect(pending)
                    .expect("host effect must execute");
                step = restored
                    .resume(&restored_handle, result)
                    .expect("restored command must resume");
            }
        }
    }
    restored
        .release_command(&restored_handle)
        .expect("restored terminal command must release");
}

/// Verifies an uncertain mutation checkpoint restores in a fresh Core instance and reconciles
/// without replaying the write.
/// 验证一个结果不确定的变更检查点在新 Core 实例中恢复，并在不重放写入的情况下对账。
#[test]
fn uncertain_checkpoint_restores_and_reconciles_in_fresh_instance() {
    let first = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    host.uncertain_next_mutation = true;
    let (handle, step) = first
        .start(with_identity(
            &first,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    subject_id: subject_id.clone(),
                    scope_access: scope_access.clone(),
                    replacement_scope: fixture_replacement_scope(&scope_access),
                    session_source_id: test_direct_session(),
                    vector_space_id: vector_space_id.clone(),
                    item: direct_write_item(
                        scope_access.write_scope.clone(),
                        MemoryCategory::General,
                        "uncertain checkpoint write",
                        Vec::new(),
                        50,
                    ),
                    occurred_at_ns: 1,
                    mutation_identity: identity,
                })
            },
            "write:uncertain-checkpoint",
            1,
        ))
        .expect("write must start");
    let CommandStep::Effects(mut embedding_effects) = step else {
        assert!(false, "write must first request embedding");
        return;
    };
    let embedding_result = host
        .execute_effect(
            embedding_effects
                .pop()
                .expect("embedding effect must exist"),
        )
        .expect("embedding effect must succeed");
    let review_step = first
        .resume(&handle, embedding_result)
        .expect("embedding resume must succeed");
    let mutation_request = drive_to_storage_mutation(&first, &handle, &mut host, review_step);
    let mutation_identity = match &mutation_request.effect {
        HostEffect::StorageMutation(batch) => batch.identity.clone(),
        _ => panic!("direct write must emit a storage mutation"),
    };
    let uncertain_result = host
        .execute_effect(mutation_request)
        .expect("host must return typed uncertain outcome");
    let replay_probe = uncertain_result.clone();
    let uncertain_error = first
        .resume(&handle, uncertain_result)
        .expect_err("uncertain mutation must not complete");
    assert_eq!(uncertain_error.code, ErrorCode::StorageResultUncertain);
    let checkpoint = first
        .export_checkpoint(&handle)
        .expect("uncertain command checkpoint must export");

    // A fresh Core restores the uncertain command and reconciles with the host receipt.
    // 全新 Core 恢复结果不确定命令，并使用宿主收据对账。
    let restored = MemoryMeshCore::new().expect("restored Core must initialize");
    let (restored_handle, restored_step) = restored
        .restore_checkpoint(&checkpoint)
        .expect("uncertain checkpoint must restore");
    assert!(
        matches!(restored_step, CommandStep::Effects(ref effects) if effects.is_empty()),
        "restored uncertain command must not reissue a mutation"
    );
    assert_eq!(
        restored
            .command_diagnostics(&restored_handle)
            .expect("restored diagnostics must exist")
            .status,
        CommandStatus::ResultUncertain,
        "restored command must retain the uncertain status"
    );
    let restored_error = restored
        .command_diagnostics(&restored_handle)
        .expect("diagnostics")
        .last_error
        .expect("uncertain diagnostics need reconciliation identity");
    assert_eq!(restored_error, uncertain_error);
    let blocked_replay = restored
        .resume(&restored_handle, replay_probe)
        .expect_err("uncertain commands require reconcile, not resume");
    assert_eq!(blocked_replay, uncertain_error);
    let reconciled = restored
        .reconcile(
            &restored_handle,
            committed_direct_memory_outcome(&mutation_identity, 1, "memory-checkpoint-reconciled"),
        )
        .expect("committed reconciliation must complete the restored command");
    assert!(matches!(
        reconciled,
        CommandStep::Complete(ref result)
            if matches!(result.as_ref(), CommandResult::MemoryWrite(_))
    ));
    assert_eq!(
        restored
            .command_diagnostics(&restored_handle)
            .expect("reconciled diagnostics must exist")
            .status,
        CommandStatus::Completed
    );
    restored
        .release_command(&restored_handle)
        .expect("reconciled restored command must release");
}

/// Verifies Core-owned lifecycle policy, reviewer-owned supersede targets, and L1 evidence
/// source text reach the durable memory without assigning lifecycle authority to the model.
/// 验证 Core 自有生命周期策略、审核器自有替代目标与 L1 证据来源文本均到达持久记忆，
/// 且不会把生命周期权威错误交给模型。
#[test]
fn candidate_lifecycle_fields_reach_the_durable_memory() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    // Seed one memory the candidate declares as superseded.
    // 播种一条候选声明为被替代的记忆。
    let seed = with_identity(
        &core,
        |identity| {
            CoreCommand::MemoryWrite(MemoryWriteCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                replacement_scope: fixture_replacement_scope(&scope_access),
                session_source_id: test_direct_session(),
                vector_space_id: vector_space_id.clone(),
                item: direct_write_item(
                    scope_access.write_scope.clone(),
                    MemoryCategory::General,
                    "User preference: 我偏好 Rust 技术栈",
                    Vec::new(),
                    20,
                ),
                occurred_at_ns: 0,
                mutation_identity: identity,
            })
        },
        "write:lifecycle-seed",
        0,
    );
    let seed = core
        .run_to_completion(seed, &mut host)
        .expect("seed write must complete");
    let CommandResult::MemoryWrite(seed) = seed else {
        panic!("seed write must return its bound memory");
    };
    let superseded_memory_id = seed.memory.id;
    let intake = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionIntake(PostActionIntakeCommand {
                        input_kind: vulcan_memorymesh_core::PostActionInputKind::SingleTurn,
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        session_source_id: SessionSourceId::new("session-lifecycle")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        timeline: vec![TimelineItem {
                            role: Role::User,
                            content: "我偏好 Rust 技术栈".to_owned(),
                            occurred_at_ns: 1,
                        }],
                        completed_at_ns: 1,
                        mutation_identity: identity,
                    })
                },
                "intake:lifecycle",
                1,
            ),
            &mut host,
        )
        .expect("intake must complete");
    let CommandResult::PostActionAccepted(intake) = intake else {
        assert!(false, "intake must return its typed result");
        return;
    };
    let pending_turn = intake.turn_id.expect("intake must persist a pending turn");
    host.scenario = HostScenario::PrioritySupersede;
    let analysis = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        profile_sections: vec![subject_profile_section(&scope_access)],
                        session_source_id: SessionSourceId::new("session-lifecycle")
                            .expect("session identity must be valid"),
                        vector_space_id: vector_space_id.clone(),
                        turn_id: pending_turn,
                        analysis_generation: 1,
                        analysis_cutoff_ns: 2,
                        direct_write_observed_after_ns: 0,
                        mutation_identity: identity,
                    })
                },
                "analyze:lifecycle",
                2,
            ),
            &mut host,
        )
        .expect("analysis must complete");
    let CommandResult::PostActionAnalysis(analysis) = analysis else {
        assert!(false, "analysis must return its typed result");
        return;
    };
    assert_eq!(analysis.accepted_memory_ids.len(), 1);
    // L1 does not own lifecycle values. The durable record must use the immutable snapshot
    // policy while preserving the candidate's evidence source text.
    // L1 不拥有生命周期值。持久记录必须使用不可变快照策略，同时保留候选的证据来源文本。
    let details = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                memory_id: analysis.accepted_memory_ids[0].clone(),
            }),
            &mut host,
        )
        .expect("memory details must complete");
    let CommandResult::MemoryDetails(details) = details else {
        assert!(false, "memory details must return its typed result");
        return;
    };
    let memory = details.expect("accepted memory must exist");
    assert_eq!(
        memory.priority, 20,
        "the frozen PostAction priority policy must reach the record"
    );
    assert_eq!(memory.scope_class, MemoryScopeClass::Environment);
    assert_eq!(memory.stability, MemoryStability::Stable);
    assert_eq!(memory.memory_level, MemoryLevel::Stable);
    assert_eq!(
        memory.created_at_ns, 2,
        "the analysis cutoff must deterministically represent the VMM analysis commit time"
    );
    assert_eq!(memory.updated_at_ns, 2);
    assert_eq!(
        memory.expires_at_ns,
        Some(15_552_000_000_000_001),
        "the frozen PostAction expiry policy must be anchored to the target turn"
    );
    assert_eq!(
        memory.evidence_source.as_deref(),
        Some("user_asserted"),
        "candidate evidence source must be durably written"
    );
    // The supersede target must be retired only after the unified L2 review confirms it.
    // 替代目标必须仅在统一 L2 审核确认后退役。
    let superseded = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                memory_id: superseded_memory_id,
            }),
            &mut host,
        )
        .expect("superseded memory details must complete");
    let CommandResult::MemoryDetails(superseded) = superseded else {
        assert!(
            false,
            "superseded memory details must return its typed result"
        );
        return;
    };
    let superseded = superseded.expect("superseded memory must exist");
    assert_eq!(
        superseded.status_reason.as_deref(),
        Some("superseded-by-review"),
        "the declared supersede target must be confirmed and retired by the reviewer"
    );
}

/// Verifies direct writes share durable soft dedupe and semantic supersede behavior with `PostAction` L2.
/// 验证主动写入与 `PostAction` L2 共享持久软去重与语义替代行为。
#[test]
fn direct_write_reuses_exact_memory_and_applies_candidate_local_supersede() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let build_write = |identity: MutationIdentity, content: &str, occurred_at_ns: i64| {
        CoreCommand::MemoryWrite(MemoryWriteCommand {
            subject_id: subject_id.clone(),
            scope_access: scope_access.clone(),
            replacement_scope: fixture_replacement_scope(&scope_access),
            session_source_id: test_direct_session(),
            vector_space_id: vector_space_id.clone(),
            item: direct_write_item(
                scope_access.write_scope.clone(),
                MemoryCategory::ProjectContext,
                content,
                Vec::new(),
                50,
            ),
            occurred_at_ns,
            mutation_identity: identity,
        })
    };
    let seed = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, "prefer rust", 1),
                "write:direct-review-seed",
                1,
            ),
            &mut host,
        )
        .expect("direct seed must complete");
    let CommandResult::MemoryWrite(seed) = seed else {
        panic!("direct seed must return its typed result");
    };
    assert!(!seed.deduplicated);
    assert!(seed.receipt.is_some());
    assert!(
        seed.memory
            .dedupe_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("v1:"))
    );

    let duplicate = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, "prefer rust", 2),
                "write:direct-review-duplicate",
                2,
            ),
            &mut host,
        )
        .expect("exact direct duplicate must complete");
    let CommandResult::MemoryWrite(duplicate) = duplicate else {
        panic!("direct duplicate must return its typed result");
    };
    assert!(duplicate.deduplicated);
    assert!(duplicate.receipt.is_none());
    assert_eq!(duplicate.memory.id, seed.memory.id);

    host.scenario = HostScenario::PrioritySupersede;
    let replacement = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    build_write(
                        identity,
                        "prefer rust for deterministic systems architecture and long-lived services",
                        3,
                    )
                },
                "write:direct-review-supersede",
                3,
            ),
            &mut host,
        )
        .expect("semantic direct replacement must complete");
    let CommandResult::MemoryWrite(replacement) = replacement else {
        panic!("direct replacement must return its typed result");
    };
    assert!(!replacement.deduplicated);
    assert!(replacement.receipt.is_some());
    assert_ne!(replacement.memory.id, seed.memory.id);
    let old = core
        .run_to_completion(
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id,
                scope_access,
                memory_id: seed.memory.id,
            }),
            &mut host,
        )
        .expect("superseded direct memory details must complete");
    let CommandResult::MemoryDetails(Some(old)) = old else {
        panic!("superseded direct memory must remain queryable for audit");
    };
    assert_eq!(old.state, MemoryState::Superseded);
}

/// Verifies hard dedupe uses the pre-threshold vector pool when final retrieval hits are empty.
/// 验证最终检索命中为空时，硬去重仍使用阈值前向量候选池。
#[test]
fn direct_hard_dedupe_survives_final_candidate_filtering() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let build_write = |identity: MutationIdentity, content: &str, occurred_at_ns: i64| {
        CoreCommand::MemoryWrite(MemoryWriteCommand {
            subject_id: subject_id.clone(),
            scope_access: scope_access.clone(),
            replacement_scope: fixture_replacement_scope(&scope_access),
            session_source_id: test_direct_session(),
            vector_space_id: vector_space_id.clone(),
            item: direct_write_item(
                scope_access.write_scope.clone(),
                MemoryCategory::ProjectContext,
                content,
                Vec::new(),
                50,
            ),
            occurred_at_ns,
            mutation_identity: identity,
        })
    };
    let seed = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, "prefer rust alpha", 10),
                "write:hard-pool-seed",
                10,
            ),
            &mut host,
        )
        .expect("hard-pool seed must complete");
    let CommandResult::MemoryWrite(seed) = seed else {
        assert!(false, "hard-pool seed must return its typed result");
        return;
    };
    let review_calls_before = host.review_llm_calls;
    host.depress_candidate_scores = true;
    let duplicate = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, "prefer rust bravo", 11),
                "write:hard-pool-duplicate",
                11,
            ),
            &mut host,
        )
        .expect("hard-pool duplicate must complete");
    let CommandResult::MemoryWrite(duplicate) = duplicate else {
        assert!(false, "hard-pool duplicate must return its typed result");
        return;
    };
    assert!(duplicate.deduplicated);
    assert_eq!(duplicate.memory.id, seed.memory.id);
    assert_eq!(host.review_llm_calls, review_calls_before);
    // Zero must bypass only the local hard gate while still sending the new candidate to L2.
    // 零值必须仅跳过本地硬门控，仍将新候选发送到 L2。
    let mut policy = vulcan_memorymesh_core::ConfigBundle {
        name: "direct-hard-dedupe-disabled".to_owned(),
        ..Default::default()
    };
    policy.memory_policy.retrieval_policy.hard_dedupe[0].threshold = 0.0;
    core.register_config(policy)
        .expect("register disabled hard dedupe");
    core.activate_config("direct-hard-dedupe-disabled")
        .expect("activate disabled hard dedupe");
    let admitted = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, "prefer rust charlie", 12),
                "write:hard-pool-disabled",
                12,
            ),
            &mut host,
        )
        .expect("write with disabled hard dedupe");
    let CommandResult::MemoryWrite(admitted) = admitted else {
        panic!("write result")
    };
    assert!(!admitted.deduplicated);
    assert_ne!(admitted.memory.id, seed.memory.id);
    assert_eq!(host.review_llm_calls, review_calls_before + 1);
}

/// Verifies the recent direct-write soft-idempotency window is inclusive at 24 hours only.
/// 验证近期主动写入软幂等窗口仅在二十四小时边界内闭区间生效。
#[test]
fn direct_soft_dedupe_freezes_the_twenty_four_hour_boundary() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let build_write = |identity: MutationIdentity, occurred_at_ns: i64| {
        CoreCommand::MemoryWrite(MemoryWriteCommand {
            subject_id: subject_id.clone(),
            scope_access: scope_access.clone(),
            replacement_scope: fixture_replacement_scope(&scope_access),
            session_source_id: test_direct_session(),
            vector_space_id: vector_space_id.clone(),
            item: direct_write_item(
                scope_access.write_scope.clone(),
                MemoryCategory::General,
                "remember exact soft boundary",
                Vec::new(),
                20,
            ),
            occurred_at_ns,
            mutation_identity: identity,
        })
    };
    let seed_time = 100_000_001_i64;
    let seed = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, seed_time),
                "write:soft-window-seed",
                seed_time,
            ),
            &mut host,
        )
        .expect("soft-window seed must complete");
    let CommandResult::MemoryWrite(seed) = seed else {
        assert!(false, "soft-window seed must return its typed result");
        return;
    };
    host.suppress_candidate_queries = true;
    let boundary_time = seed_time + 24 * 60 * 60 * 1_000_000_000;
    let boundary = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, boundary_time),
                "write:soft-window-boundary",
                boundary_time,
            ),
            &mut host,
        )
        .expect("inclusive soft-window boundary must complete");
    let CommandResult::MemoryWrite(boundary) = boundary else {
        assert!(false, "soft-window boundary must return its typed result");
        return;
    };
    assert!(boundary.deduplicated);
    assert_eq!(boundary.memory.id, seed.memory.id);

    let outside_time = boundary_time + 1;
    let outside = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| build_write(identity, outside_time),
                "write:soft-window-outside",
                outside_time,
            ),
            &mut host,
        )
        .expect("outside soft-window write must complete");
    let CommandResult::MemoryWrite(outside) = outside else {
        assert!(
            false,
            "outside soft-window write must return its typed result"
        );
        return;
    };
    assert!(!outside.deduplicated);
    assert_ne!(outside.memory.id, seed.memory.id);
}

/// Verifies retention facts cannot cross the subject or write-scope trust boundary.
/// 验证保留事实不能跨越主体或写入作用域信任边界。
#[test]
fn retention_rejects_mismatched_result_identity() {
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, _) = identities();
    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::Retention(RetentionCommand {
                subject_id,
                scope_access,
                now_ns: 10_000,
                policy: retention_policy(),
                mutation_identity: identity,
            })
        },
        "retention:reject-cross-scope",
        10_000,
    );
    let (handle, step) = core.start(command).expect("Retention must start");
    let CommandStep::Effects(effects) = step else {
        panic!("Retention must request storage facts");
    };
    let effect = effects
        .into_iter()
        .next()
        .expect("Retention must emit exactly one storage query");
    let error = core
        .resume(
            &handle,
            EffectResult {
                effect_id: effect.effect_id,
                result: HostEffectResult::StorageQuery(StorageQueryResult::RetentionScan(
                    vulcan_memorymesh_core::RetentionScanFacts {
                        subject_id: SubjectId::new("other-subject")
                            .expect("foreign subject identity must be valid"),
                        scope_id: ScopeId::new("other-scope")
                            .expect("foreign scope identity must be valid"),
                        expired_profile_nodes: Vec::new(),
                        terminal_memories: Vec::new(),
                        cold_turn_candidates: Vec::new(),
                        cold_turn_jobs: Vec::new(),
                        idle_sessions: Vec::new(),
                        vector_gc_jobs: Vec::new(),
                        trash_batches: Vec::new(),
                    },
                )),
            },
        )
        .expect_err("mismatched retention identity must be rejected");
    assert_eq!(error.code, ErrorCode::EffectMismatch);
}

/// Verifies all seven retention families from typed Host facts through deterministic Core plans.
/// 验证七类保留维护从强类型宿主事实到确定性 Core 计划的完整链路。
#[test]
fn retention_plans_all_seven_families_with_due_and_protection_rules() {
    const NOW_NS: i64 = 10_000_000;
    let core = MemoryMeshCore::new().expect("Core must initialize");
    let (subject_id, scope_access, vector_space_id) = identities();
    let mut host = DeterministicHost::new();
    let written = core
        .run_to_completion(
            with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        item: direct_write_item(
                            scope_access.write_scope.clone(),
                            MemoryCategory::General,
                            "six-family retention fixture",
                            Vec::new(),
                            20,
                        ),
                        occurred_at_ns: 1_000_000,
                        mutation_identity: identity,
                    })
                },
                "retention:six-family-seed",
                1_000_000,
            ),
            &mut host,
        )
        .expect("retention fixture write must complete");
    let CommandResult::MemoryWrite(written) = written else {
        panic!("retention fixture write must return one memory");
    };
    terminalize_memory(&mut host, written.memory.id.as_str(), 900_000);
    let terminal_memory = host
        .storage
        .dataset()
        .memories
        .into_iter()
        .find(|memory| memory.id == written.memory.id)
        .expect("terminal retention fixture must exist");

    // Reject malformed terminal records before their version can become executable plan authority.
    // 在畸形终态记录的版本成为可执行计划权威前拒绝该记录。
    let invalid_command = with_identity(
        &core,
        |identity| {
            CoreCommand::Retention(RetentionCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                now_ns: NOW_NS,
                policy: retention_policy(),
                mutation_identity: identity,
            })
        },
        "retention:invalid-terminal-record",
        NOW_NS,
    );
    let (invalid_handle, invalid_step) = core
        .start(invalid_command)
        .expect("invalid-record Retention must start");
    let CommandStep::Effects(mut invalid_effects) = invalid_step else {
        panic!("invalid-record Retention must request storage facts");
    };
    let invalid_effect = invalid_effects
        .pop()
        .expect("invalid-record Retention query must exist");
    let mut invalid_memory = terminal_memory.clone();
    invalid_memory.version = 0;
    let invalid_error = core
        .resume(
            &invalid_handle,
            EffectResult {
                effect_id: invalid_effect.effect_id,
                result: HostEffectResult::StorageQuery(StorageQueryResult::RetentionScan(
                    RetentionScanFacts {
                        subject_id: subject_id.clone(),
                        scope_id: scope_access.write_scope.clone(),
                        expired_profile_nodes: Vec::new(),
                        terminal_memories: vec![RetentionMemoryFact {
                            memory: invalid_memory,
                            protected_shared: false,
                        }],
                        cold_turn_candidates: Vec::new(),
                        cold_turn_jobs: Vec::new(),
                        idle_sessions: Vec::new(),
                        vector_gc_jobs: Vec::new(),
                        trash_batches: Vec::new(),
                    },
                )),
            },
        )
        .expect_err("invalid terminal record must not produce a plan");
    assert_eq!(invalid_error.code, ErrorCode::EffectMismatch);

    let mut protected_shared_memory = terminal_memory.clone();
    protected_shared_memory.id =
        MemoryId::new("memory-protected-shared").expect("protected memory identity must be valid");
    protected_shared_memory.priority = 100;
    let mut policy = retention_policy();
    policy.skip_protected_shared_memories = true;
    policy.protected_priority_floor = 50;

    let command = with_identity(
        &core,
        |identity| {
            CoreCommand::Retention(RetentionCommand {
                subject_id: subject_id.clone(),
                scope_access: scope_access.clone(),
                now_ns: NOW_NS,
                policy: policy.clone(),
                mutation_identity: identity,
            })
        },
        "retention:six-family-plan",
        NOW_NS,
    );
    let (handle, step) = core.start(command).expect("Retention must start");
    let CommandStep::Effects(mut effects) = step else {
        panic!("Retention must request storage facts");
    };
    assert_eq!(effects.len(), 1);
    let effect = effects.pop().expect("Retention query effect must exist");
    let facts = RetentionScanFacts {
        subject_id: subject_id.clone(),
        scope_id: scope_access.write_scope.clone(),
        expired_profile_nodes: vec![ProfileNode {
            id: ProfileNodeId::new("profile-due").expect("profile identity must be valid"),
            subject_id: Some(subject_id.clone()),
            scope_id: scope_access.write_scope.clone(),
            content: "due profile fact".to_owned(),
            confidence: Some(1.0),
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
            expires_at_ns: Some(NOW_NS),
            status_reason: None,
            superseded_by: None,
            version: 1,
            created_at_ns: 1,
            updated_at_ns: 1,
        }],
        terminal_memories: vec![
            RetentionMemoryFact {
                memory: protected_shared_memory,
                protected_shared: true,
            },
            RetentionMemoryFact {
                memory: terminal_memory.clone(),
                protected_shared: false,
            },
        ],
        cold_turn_candidates: vec![RetentionColdTurnCandidateFact {
            session_source_id: SessionSourceId::new("session-cold-enqueue")
                .expect("cold session identity must be valid"),
            recyclable_turn_count: 2,
        }],
        cold_turn_jobs: vec![
            RetentionColdTurnJobFact {
                job_id: "cold-future".to_owned(),
                session_source_id: SessionSourceId::new("session-cold-future")
                    .expect("future cold session identity must be valid"),
                next_run_at_ns: NOW_NS + 1,
            },
            RetentionColdTurnJobFact {
                job_id: "cold-due".to_owned(),
                session_source_id: SessionSourceId::new("session-cold-due")
                    .expect("due cold session identity must be valid"),
                next_run_at_ns: NOW_NS,
            },
        ],
        idle_sessions: vec![
            RetentionIdleSessionFact {
                session_source_id: SessionSourceId::new("session-idle-pending")
                    .expect("pending idle session identity must be valid"),
                last_activity_ns: 1_000_000,
                has_pending_turn: true,
            },
            RetentionIdleSessionFact {
                session_source_id: SessionSourceId::new("session-idle-eligible")
                    .expect("eligible idle session identity must be valid"),
                last_activity_ns: NOW_NS - 1_000_000,
                has_pending_turn: false,
            },
            RetentionIdleSessionFact {
                session_source_id: SessionSourceId::new("session-idle-recent")
                    .expect("recent idle session identity must be valid"),
                last_activity_ns: NOW_NS,
                has_pending_turn: false,
            },
        ],
        vector_gc_jobs: vec![
            RetentionVectorGcJobFact {
                job_id: "vector-future".to_owned(),
                vector_id: "vector-future".to_owned(),
                next_run_at_ns: NOW_NS + 1,
            },
            RetentionVectorGcJobFact {
                job_id: "vector-due".to_owned(),
                vector_id: "vector-due".to_owned(),
                next_run_at_ns: NOW_NS - 1,
            },
        ],
        trash_batches: vec![
            RetentionTrashBatchFact {
                batch_id: "trash-fresh".to_owned(),
                recycled_at_ns: NOW_NS,
                expires_at_ns: NOW_NS + 1,
            },
            RetentionTrashBatchFact {
                batch_id: "trash-expired".to_owned(),
                // A changed policy must not extend this batch's shorter original window.
                // 策略变更不得延长该批次原先更短的窗口。
                recycled_at_ns: NOW_NS - 10,
                expires_at_ns: NOW_NS,
            },
        ],
    };
    let completed = core
        .resume(
            &handle,
            EffectResult {
                effect_id: effect.effect_id,
                result: HostEffectResult::StorageQuery(StorageQueryResult::RetentionScan(facts)),
            },
        )
        .expect("seven-family Retention facts must complete");
    let CommandStep::Complete(result) = completed else {
        panic!("Retention facts must complete synchronously");
    };
    let CommandResult::Retention(plan) = result.as_ref() else {
        panic!("Retention must return one typed plan set");
    };
    assert_eq!(plan.subject_id, subject_id);
    assert_eq!(plan.scope_id, scope_access.write_scope);
    assert_eq!(plan.profile_expiry.len(), 1);
    assert_eq!(
        plan.profile_expiry[0].profile_node_id.as_str(),
        "profile-due"
    );
    assert_eq!(
        plan.profile_expiry[0].reason,
        RetentionReasonCode::ProfileExpiry
    );
    assert_eq!(plan.terminal_memory_recycle.len(), 1);
    assert_eq!(
        plan.terminal_memory_recycle[0].memory_id,
        terminal_memory.id.as_str()
    );
    assert_eq!(plan.cold_turn_enqueue.len(), 1);
    assert_eq!(
        plan.cold_turn_enqueue[0].session_source_id,
        "session-cold-enqueue"
    );
    assert_eq!(plan.cold_turn_execute.len(), 1);
    assert_eq!(plan.cold_turn_execute[0].job_id, "cold-due");
    assert_eq!(plan.idle_session_recycle.len(), 1);
    assert_eq!(
        plan.idle_session_recycle[0].session_source_id,
        "session-idle-eligible"
    );
    assert_eq!(plan.vector_gc_retry.len(), 1);
    assert_eq!(plan.vector_gc_retry[0].job_id, "vector-due");
    assert_eq!(plan.trash_purge.len(), 1);
    assert_eq!(plan.trash_purge[0].batch_id, "trash-expired");
    assert_eq!(
        plan.terminal_memory_recycle[0].reason,
        RetentionReasonCode::TerminalMemoryRecycle
    );
    assert_eq!(
        plan.cold_turn_enqueue[0].reason,
        RetentionReasonCode::ColdTurnRecycle
    );
    assert_eq!(
        plan.cold_turn_execute[0].reason,
        RetentionReasonCode::ColdTurnRecycle
    );
    assert_eq!(
        plan.idle_session_recycle[0].reason,
        RetentionReasonCode::IdleSessionRecycle
    );
    assert_eq!(
        plan.vector_gc_retry[0].reason,
        RetentionReasonCode::VectorGcRetry
    );
    assert_eq!(
        plan.trash_purge[0].reason,
        RetentionReasonCode::TrashExpiryPurge
    );
}

/// Verifies reused rows reflect this batch's committed supersede, including restored reconciliation.
/// 验证复用行反映本批次已提交的替代状态，并覆盖恢复后的对账路径。
#[test]
fn mixed_batch_reused_rows_reflect_committed_supersede() {
    for reverse in [false, true] {
        for uncertain in [false, true] {
            // Persist a soft-dedupe target and prepare a distinct semantic replacement.
            // 持久化软去重目标，并准备内容不同的语义替代项。
            let core = MemoryMeshCore::new().expect("Core initializes");
            let (subject_id, scope_access, vector_space_id) = identities();
            let mut host = DeterministicHost::new();
            let existing_item = direct_write_item(
                scope_access.write_scope.clone(),
                MemoryCategory::ProjectContext,
                "prefer rust",
                Vec::new(),
                50,
            );
            let seed = with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWrite(MemoryWriteCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        session_source_id: test_direct_session(),
                        vector_space_id: vector_space_id.clone(),
                        occurred_at_ns: 1,
                        item: existing_item.clone(),
                        mutation_identity: identity,
                    })
                },
                "seed:batch-reuse-supersede",
                1,
            );
            let CommandResult::MemoryWrite(seed) = core
                .run_to_completion(seed, &mut host)
                .expect("seed writes")
            else {
                panic!("seed result")
            };
            let replacement = direct_write_item(
                scope_access.write_scope.clone(),
                MemoryCategory::ProjectContext,
                "prefer rust for deterministic systems architecture and long-lived services",
                Vec::new(),
                50,
            );
            let mut items = vec![existing_item.clone(), existing_item, replacement];
            if reverse {
                items.reverse();
            }
            host.scenario = HostScenario::PrioritySupersede;
            let command = with_identity(
                &core,
                |identity| {
                    CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                        subject_id: subject_id.clone(),
                        scope_access: scope_access.clone(),
                        replacement_scope: fixture_replacement_scope(&scope_access),
                        authorized_write_scopes: vec![scope_access.write_scope.clone()],
                        session_source_id: test_direct_session(),
                        vector_space_id,
                        occurred_at_ns: 2,
                        items,
                        mutation_identity: identity,
                    })
                },
                "write:batch-reuse-supersede",
                2,
            );
            let (handle, step) = core.start(command).expect("batch starts");
            let request = drive_to_storage_mutation(&core, &handle, &mut host, step);
            let HostEffect::StorageMutation(batch) = &request.effect else {
                panic!("mutation")
            };
            assert!(
                batch.mutations.iter().any(|mutation| matches!(mutation,
                    Mutation::SupersedeMemory { memory_id, .. } if *memory_id == seed.memory.id
                )),
                "the regression must actually supersede the reused row"
            );

            // Restore the pending batch, then use a real reference-store commit receipt.
            // 恢复待提交批次，再使用参考存储实际提交的收据。
            let checkpoint = core.export_checkpoint(&handle).expect("checkpoint exports");
            let restored = MemoryMeshCore::new().expect("fresh Core");
            let (restored_handle, restored_step) =
                restored.restore_checkpoint(&checkpoint).expect("restore");
            assert_eq!(restored_step, CommandStep::Effects(vec![request.clone()]));
            let acknowledgement = host
                .execute_effect(request.clone())
                .expect("commit succeeds");
            let completed = if uncertain {
                restored
                    .resume(
                        &restored_handle,
                        EffectResult {
                            effect_id: request.effect_id,
                            result: HostEffectResult::StorageMutation(MutationOutcome::Uncertain),
                        },
                    )
                    .expect_err("lost acknowledgement requires reconciliation");
                let uncertain_checkpoint = restored
                    .export_checkpoint(&restored_handle)
                    .expect("uncertain export");
                let reconciler = MemoryMeshCore::new().expect("fresh reconciler");
                let (reconcile_handle, _) = reconciler
                    .restore_checkpoint(&uncertain_checkpoint)
                    .expect("uncertain restore");
                let HostEffectResult::StorageMutation(outcome) = acknowledgement.result else {
                    panic!("receipt")
                };
                reconciler
                    .reconcile(&reconcile_handle, outcome)
                    .expect("reconciles")
            } else {
                restored
                    .resume(&restored_handle, acknowledgement)
                    .expect("completes")
            };
            let CommandStep::Complete(result) = completed else {
                panic!("complete result")
            };
            let CommandResult::MemoryWriteBatch(result) = *result else {
                panic!("batch result")
            };

            // Compare every public row with durable state, not just identity and result count.
            // 逐行比较公开结果与持久状态，而不只检查身份和结果数量。
            assert_eq!(result.memories.len(), 3);
            assert_eq!(
                result.deduplicated,
                if reverse {
                    vec![false, true, true]
                } else {
                    vec![true, true, false]
                }
            );
            let durable = host.storage.dataset().memories;
            for row in &result.memories {
                let committed = durable
                    .iter()
                    .find(|memory| memory.id == row.id)
                    .expect("durable row");
                assert_eq!(
                    row, committed,
                    "returned records must include this batch's own mutations"
                );
            }
            let reused = result
                .memories
                .iter()
                .find(|memory| memory.id == seed.memory.id)
                .expect("reused row");
            assert_eq!(reused.state, MemoryState::Superseded);
            assert_eq!(reused.version, seed.memory.version + 1);
        }
    }
}
