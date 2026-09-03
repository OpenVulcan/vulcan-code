//! Cross-language fingerprint golden tests fixing exact material bytes.
//! 固定精确材料字节的跨语言指纹黄金测试。

#![allow(clippy::expect_used)]

use vulcan_memorymesh_core_contracts::{
    ChatCompactCommand, CoreCommand, MaintenanceAction, MaintenanceCommand, MemoryCategory,
    MemoryDeleteBatchCommand, MemoryDeleteBatchItem, MemoryDeleteCommand,
    MemoryDetailsBatchCommand, MemoryDetailsCommand, MemoryId, MemoryLevel, MemoryScopeClass,
    MemorySearchBatchCommand, MemorySearchCommand, MemoryWriteBatchCommand, MemoryWriteBatchItem,
    MemoryWriteCommand, MutationIdentity, MutationSourceKind, OperationId,
    PostActionAnalyzePendingTurnCommand, PostActionIntakeCommand, PreCheckCommand,
    ProfileInstructionCommand, ProfileQueryCommand, ProfileView, ProfileViewMode,
    ProfileViewSection, RecallMode, RetentionCommand, RetentionPolicy, Role, ScopeAccess, ScopeId,
    SectionRole, SessionRecallMode, SessionSourceId, SubjectId, TimelineItem, TurnId,
    VectorSpaceId, request_fingerprint,
};

/// Preserves the existing fixture's readable scopes while explicitly allowing replacement only in its primary write scope.
/// 保留既有用例的可读范围，同时仅显式允许替代其主要写作用域。
fn fixture_replacement_scope(
    access: &ScopeAccess,
) -> vulcan_memorymesh_core_contracts::MemoryReplacementScope {
    vulcan_memorymesh_core_contracts::MemoryReplacementScope {
        candidates: vulcan_memorymesh_core_contracts::MemoryCandidateScope {
            scope_ids: access.visible_scopes.clone(),
            origin_session_source_id: None,
        },
        authorized_supersede_scopes: vec![access.write_scope.clone()],
    }
}

/// Snapshot id shared by every golden vector.
/// 所有黄金向量共享的快照标识。
const SNAPSHOT_ID: &str = "snapshot-0123456789abcdef";

/// Builds the shared scope access used by golden commands.
/// 构建黄金命令使用的共享作用域访问。
fn scope_access() -> ScopeAccess {
    ScopeAccess {
        write_scope: ScopeId::new("scope-golden").expect("scope identity must be valid"),
        visible_scopes: vec![
            ScopeId::new("scope-golden").expect("scope identity must be valid"),
            ScopeId::new("scope-visible").expect("scope identity must be valid"),
        ],
    }
}

/// Builds the shared subject identity.
/// 构建共享主体身份。
fn subject() -> SubjectId {
    SubjectId::new("subject-golden").expect("subject identity must be valid")
}

/// Builds one shared golden mutation identity seed.
/// 构建一个共享黄金变更身份种子。
fn identity(source: MutationSourceKind) -> MutationIdentity {
    MutationIdentity {
        operation_id: OperationId::new("op-golden").expect("operation identity must be valid"),
        idempotency_key: vulcan_memorymesh_core_contracts::IdempotencyKey::new("golden-key")
            .expect("idempotency identity must be valid"),
        request_fingerprint: String::new(),
        source_kind: source,
        request_source_id: "golden-test".to_owned(),
        observed_at_ns: 9_999,
    }
}

/// Builds one representative `PreCheck` command with timeline and unicode content.
/// 构建一个带时间线与 Unicode 内容的代表性 `PreCheck` 命令。
fn precheck_command() -> CoreCommand {
    CoreCommand::PreCheck(PreCheckCommand {
        subject_id: subject(),
        scope_access: scope_access(),
        session_source_id: SessionSourceId::new("session-golden")
            .expect("session identity must be valid"),
        vector_space_id: VectorSpaceId::new("vector-golden")
            .expect("vector identity must be valid"),
        candidate_scope: vulcan_memorymesh_core_contracts::MemoryCandidateScope {
            scope_ids: scope_access().visible_scopes,
            origin_session_source_id: Some(
                SessionSourceId::new("session-golden").expect("session identity must be valid"),
            ),
        },
        user_content: "中文内容 Ünïcodé ✓".to_owned(),
        recent_timeline: vec![TimelineItem {
            role: Role::User,
            content: "历史轮次".to_owned(),
            occurred_at_ns: 111,
        }],
        recall_mode: RecallMode::Force,
        session_recall_mode: SessionRecallMode::CompactAware,
        max_search_queries: 3,
        max_memories: 5,
        now_ns: 1_000,
        mutation_identity: identity(MutationSourceKind::HostRpc),
    })
}

/// Builds one representative analysis command covering its recovery watermark and profile bindings.
/// 构建一个覆盖恢复水位与画像绑定的代表性分析命令。
fn analyze_command() -> CoreCommand {
    CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
        subject_id: subject(),
        scope_access: scope_access(),
        replacement_scope: fixture_replacement_scope(&scope_access()),
        profile_sections: vec![
            ProfileViewSection {
                scope_id: ScopeId::new("scope-visible").expect("scope identity must be valid"),
                role: SectionRole::Environment,
                display_label: "project".to_owned(),
            },
            ProfileViewSection {
                scope_id: ScopeId::new("scope-golden").expect("scope identity must be valid"),
                role: SectionRole::Subject,
                display_label: "user".to_owned(),
            },
        ],
        session_source_id: SessionSourceId::new("session-golden")
            .expect("session identity must be valid"),
        vector_space_id: VectorSpaceId::new("vector-golden")
            .expect("vector identity must be valid"),
        turn_id: TurnId::new("turn-golden").expect("turn identity must be valid"),
        analysis_generation: 2,
        analysis_cutoff_ns: 9_999,
        direct_write_observed_after_ns: 777,
        mutation_identity: identity(MutationSourceKind::BackgroundWorker),
    })
}

/// Builds the ten additional commands needed to freeze all sixteen public variants.
/// 构建冻结全部十六条公共命令所需的另外十条命令。
#[allow(clippy::too_many_lines)]
fn remaining_commands() -> Vec<(&'static str, CoreCommand)> {
    let session = SessionSourceId::new("session-golden").expect("session identity must be valid");
    let vector = VectorSpaceId::new("vector-golden").expect("vector identity must be valid");
    let memory = MemoryId::new("memory-golden").expect("memory identity must be valid");
    let section = ProfileViewSection {
        scope_id: ScopeId::new("scope-golden").expect("scope identity must be valid"),
        role: SectionRole::Subject,
        display_label: "user".to_owned(),
    };
    let item = MemoryWriteBatchItem {
        target_scope_id: ScopeId::new("scope-golden").expect("scope identity must be valid"),
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::General,
        summary: "batch summary".to_owned(),
        details: "batch details".to_owned(),
        priority: Some(42),
        memory_level: Some(MemoryLevel::Stable),
        expires_at_ns: Some(12_345),
        tags: vec!["golden".to_owned(), "跨语言".to_owned()],
    };
    vec![
        (
            "PostActionIntake",
            CoreCommand::PostActionIntake(PostActionIntakeCommand {
                input_kind: vulcan_memorymesh_core_contracts::PostActionInputKind::SingleTurn,
                subject_id: subject(),
                scope_access: scope_access(),
                session_source_id: session.clone(),
                vector_space_id: vector.clone(),
                timeline: vec![TimelineItem {
                    role: Role::Assistant,
                    content: "intake 回复".to_owned(),
                    occurred_at_ns: 222,
                }],
                completed_at_ns: 5_000,
                mutation_identity: identity(MutationSourceKind::HostRpc),
            }),
        ),
        (
            "MemoryDetails",
            CoreCommand::MemoryDetails(MemoryDetailsCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                memory_id: memory.clone(),
            }),
        ),
        (
            "MemoryDelete",
            CoreCommand::MemoryDelete(MemoryDeleteCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                memory_id: memory.clone(),
                expected_version: Some(7),
                mutation_identity: identity(MutationSourceKind::HostRpc),
            }),
        ),
        (
            "ProfileQuery",
            CoreCommand::ProfileQuery(ProfileQueryCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                view: ProfileView {
                    mode: ProfileViewMode::Full,
                    display_timezone: "Asia/Shanghai".to_owned(),
                    sections: vec![section.clone()],
                    include_explanations: true,
                },
                evaluated_at_ns: 5_050,
            }),
        ),
        (
            "ProfileInstruction",
            CoreCommand::ProfileInstruction(ProfileInstructionCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                target_section: section,
                instruction: "保持简洁并保留证据".to_owned(),
                occurred_at_ns: 5_100,
                mutation_identity: identity(MutationSourceKind::HostRpc),
            }),
        ),
        (
            "MemorySearchBatch",
            CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                queries: vec!["query-a".to_owned(), "查询乙".to_owned()],
                top_k: 6,
                vector_space_id: vector.clone(),
                boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
                    candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                        scope_ids: scope_access().visible_scopes,
                        origin_session_source_id: None,
                    }),
                    session_recall_boundary: Some(
                        vulcan_memorymesh_core_contracts::SessionRecallBoundary {
                            session_source_id: session.clone(),
                            mode: SessionRecallMode::Full,
                        },
                    ),
                },
                now_ns: 5_200,
            }),
        ),
        (
            "MemoryDetailsBatch",
            CoreCommand::MemoryDetailsBatch(MemoryDetailsBatchCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                memory_ids: vec![
                    memory.clone(),
                    MemoryId::new("memory-two").expect("valid memory"),
                ],
                turn_ids: vec![TurnId::new("turn-two").expect("valid turn")],
            }),
        ),
        (
            "MemoryWriteBatch",
            CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                replacement_scope: fixture_replacement_scope(&scope_access()),
                session_source_id: session,
                vector_space_id: vector,
                authorized_write_scopes: vec![
                    ScopeId::new("scope-golden").expect("valid scope"),
                    ScopeId::new("scope-visible").expect("valid scope"),
                ],
                occurred_at_ns: 5_300,
                items: vec![item],
                mutation_identity: identity(MutationSourceKind::Import),
            }),
        ),
        (
            "MemoryDeleteBatch",
            CoreCommand::MemoryDeleteBatch(MemoryDeleteBatchCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                reason: "golden cleanup".to_owned(),
                items: vec![MemoryDeleteBatchItem {
                    memory_id: memory,
                    expected_version: Some(8),
                }],
                mutation_identity: identity(MutationSourceKind::HostRpc),
            }),
        ),
        (
            "Maintenance",
            CoreCommand::Maintenance(MaintenanceCommand {
                subject_id: subject(),
                scope_access: scope_access(),
                occurred_at_ns: 5_400,
                action: MaintenanceAction::RebuildVectors {
                    memory_ids: vec![MemoryId::new("memory-golden").expect("valid memory")],
                    vector_space_id: VectorSpaceId::new("vector-rebuild").expect("valid vector"),
                },
                mutation_identity: identity(MutationSourceKind::HostRpc),
            }),
        ),
    ]
}

/// Freezes the ten additional digests so the complete sixteen-command surface is covered.
/// 冻结另外十条摘要，使全部十六条命令表面均被覆盖。
#[test]
fn remaining_commands_match_golden() {
    let expected = [
        (
            "PostActionIntake",
            "fp256:a540e5aa8d41c47af2db6caa67db5a9257a6481dde6d579222efc2fe9bb43806",
        ),
        (
            "MemoryDetails",
            "fp256:6a4e7c0aa67b5447753e74ddc0cee2d400a72d1aa8b169f3f71eae53e9713e9b",
        ),
        (
            "MemoryDelete",
            "fp256:aa01444518fcdbcfd935822ff1e4eb8659dad7a852bb8c528b3c036561df08d2",
        ),
        (
            "ProfileQuery",
            "fp256:c956a1795518642b65fb94de6592cc3d63d809d598f90dff7ff2a516e258ba5e",
        ),
        (
            "ProfileInstruction",
            "fp256:49f2a0b4e89db735dfd6c057dc07ecd90ebfd756665f14f01f92976969a907c3",
        ),
        (
            "MemorySearchBatch",
            "fp256:5f355969c7f35c86e64161bed9dcc0a52473a9eb93bb551f3568c88ba4ac1d6b",
        ),
        (
            "MemoryDetailsBatch",
            "fp256:96ea4f1e8d1806bd4cf2b9830c557e111b6d415a16325dbea5bd798750859ca6",
        ),
        (
            "MemoryWriteBatch",
            "fp256:02bb26f7e47e52e3e2d4a59f87dbb64eaea125db489d4bcc16e0f40764586d55",
        ),
        (
            "MemoryDeleteBatch",
            "fp256:1bf57de9fd1dd25c019cef3d4617fae4bd15e71b32873e5f5b32c0dbea0eb253",
        ),
        (
            "Maintenance",
            "fp256:87556743a59ff9f204c647d675f49a6e02da3e810378947e4205baccd335d341",
        ),
    ];
    for ((name, command), (expected_name, expected_digest)) in
        remaining_commands().into_iter().zip(expected)
    {
        assert_eq!(name, expected_name);
        assert_eq!(
            request_fingerprint(&command, SNAPSHOT_ID),
            expected_digest,
            "fingerprint drift for {name}"
        );
    }
}

/// Verifies the `PreCheck` fingerprint golden covers timeline, unicode, and both modes.
/// 验证 `PreCheck` 指纹黄金覆盖时间线、Unicode 与两种模式。
#[test]
fn precheck_fingerprint_matches_golden() {
    // The identity fields (operation id, idempotency key, observed time) must not
    // participate in the material, so this exact digest freezes the cross-language contract.
    // 身份字段（操作标识、幂等键、观测时间）不参与材料，因此该精确摘要冻结跨语言契约。
    assert_eq!(
        request_fingerprint(&precheck_command(), SNAPSHOT_ID),
        "fp256:daaafbb51ca35add8c6a0cd032328ab733e0e78e5262a99e78f38efc78d063ce"
    );
}

/// Verifies identical semantic payloads with different identity fields share one fingerprint.
/// 验证不同身份字段但相同语义载荷共享同一指纹。
#[test]
fn fingerprint_ignores_identity_and_observation_time() {
    let first = request_fingerprint(&precheck_command(), SNAPSHOT_ID);
    let retry = CoreCommand::PreCheck(PreCheckCommand {
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new("op-golden-retry")
                .expect("operation identity must be valid"),
            observed_at_ns: 2,
            ..identity(MutationSourceKind::HostRpc)
        },
        ..*match precheck_command() {
            CoreCommand::PreCheck(payload) => Box::new(payload),
            _ => unreachable!("golden command is always PreCheck"),
        }
    });
    assert_eq!(request_fingerprint(&retry, SNAPSHOT_ID), first);
}

/// Verifies the fingerprint changes when semantic payload fields or the snapshot change.
/// 验证语义载荷字段或快照变化时指纹随之变化。
#[test]
fn fingerprint_changes_with_semantic_payload() {
    let original = request_fingerprint(&precheck_command(), SNAPSHOT_ID);
    let changed = CoreCommand::PreCheck(PreCheckCommand {
        max_memories: 6,
        ..*match precheck_command() {
            CoreCommand::PreCheck(payload) => Box::new(payload),
            _ => unreachable!("golden command is always PreCheck"),
        }
    });
    assert_ne!(request_fingerprint(&changed, SNAPSHOT_ID), original);
    let CoreCommand::PreCheck(mut boundary_changed) = precheck_command() else {
        unreachable!("golden command is always PreCheck");
    };
    boundary_changed.candidate_scope.origin_session_source_id = None;
    assert_ne!(
        request_fingerprint(&CoreCommand::PreCheck(boundary_changed), SNAPSHOT_ID),
        original
    );
    assert_ne!(
        request_fingerprint(&precheck_command(), "snapshot-other"),
        original
    );
}

/// Freezes golden digests for representative commands of the remaining families.
/// 为其余命令族冻结代表性黄金摘要。
#[test]
fn representative_commands_match_golden() {
    assert_eq!(
        request_fingerprint(&analyze_command(), SNAPSHOT_ID),
        "fp256:fa8e4d53acaf54573640c7967cbbe72403e708b360938d987ce2f42ad5d0db8f"
    );

    let search = CoreCommand::MemorySearch(MemorySearchCommand {
        subject_id: subject(),
        scope_access: scope_access(),
        query: "golden query".to_owned(),
        limit: 7,
        vector_space_id: VectorSpaceId::new("vector-golden")
            .expect("vector identity must be valid"),
        boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
            candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                scope_ids: scope_access().visible_scopes,
                origin_session_source_id: None,
            }),
            session_recall_boundary: Some(
                vulcan_memorymesh_core_contracts::SessionRecallBoundary {
                    session_source_id: SessionSourceId::new("session-golden")
                        .expect("session identity must be valid"),
                    mode: SessionRecallMode::CompactAware,
                },
            ),
        },
        now_ns: 2_000,
    });
    assert_eq!(
        request_fingerprint(&search, SNAPSHOT_ID),
        "fp256:3b1e95cdf867c071bb4fd764a7ae28927e18f99e5b63c6fa5c4002ece89df7db"
    );
    let mut boundary_changed = search.clone();
    let CoreCommand::MemorySearch(payload) = &mut boundary_changed else {
        unreachable!("representative command is always MemorySearch");
    };
    payload.boundary.session_recall_boundary = None;
    assert_ne!(
        request_fingerprint(&boundary_changed, SNAPSHOT_ID),
        request_fingerprint(&search, SNAPSHOT_ID)
    );

    let compact = CoreCommand::ChatCompact(ChatCompactCommand {
        subject_id: subject(),
        scope_access: scope_access(),
        session_source_id: SessionSourceId::new("session-golden")
            .expect("session identity must be valid"),
        compacted_at_ns: 3_000,
        mutation_identity: identity(MutationSourceKind::BackgroundWorker),
    });
    assert_eq!(
        request_fingerprint(&compact, SNAPSHOT_ID),
        "fp256:5a176e28b69a245dd7f35166037f1cff48d903fcf472b402018dcbe9008c2019"
    );

    let write = CoreCommand::MemoryWrite(MemoryWriteCommand {
        subject_id: subject(),
        scope_access: scope_access(),
        replacement_scope: fixture_replacement_scope(&scope_access()),
        session_source_id: SessionSourceId::new("session-golden")
            .expect("session identity must be valid"),
        vector_space_id: VectorSpaceId::new("vector-golden")
            .expect("vector identity must be valid"),
        item: MemoryWriteBatchItem {
            target_scope_id: ScopeId::new("scope-golden").expect("scope identity must be valid"),
            scope_class: MemoryScopeClass::Environment,
            category: MemoryCategory::General,
            summary: "golden summary".to_owned(),
            details: "golden content".to_owned(),
            priority: Some(50),
            memory_level: Some(MemoryLevel::Stable),
            expires_at_ns: Some(10_000),
            tags: vec!["a".to_owned(), "b".to_owned()],
        },
        occurred_at_ns: 4_000,
        mutation_identity: identity(MutationSourceKind::Import),
    });
    assert_eq!(
        request_fingerprint(&write, SNAPSHOT_ID),
        "fp256:770c7b3adfc9bd4adad18025cc24ca9d5bdc1c0b51e5af005c3f7fd6ae76b6dd"
    );

    let retention = CoreCommand::Retention(RetentionCommand {
        subject_id: subject(),
        scope_access: scope_access(),
        now_ns: 6_000,
        policy: RetentionPolicy {
            schema_version: 1,
            turn_hot_window_size: 15,
            session_idle_ms: 1_800_000,
            trash_retention_ms: 86_400_000,
            protected_priority_floor: 80,
            protected_memory_level_floor: 4,
            skip_protected_shared_memories: false,
            memory_recycle_limit: 100,
            cold_turn_scan_limit: 50,
            cold_turn_job_limit: 16,
            idle_session_limit: 32,
            vector_gc_limit: 100,
            trash_purge_limit: 64,
        },
        mutation_identity: identity(MutationSourceKind::HostRpc),
    });
    assert_eq!(
        request_fingerprint(&retention, SNAPSHOT_ID),
        "fp256:95ec55e11abbf261dc8dfe2ca63800c5674c161bf225ce3abe4b7832ba984ea7"
    );
}

/// Verifies every analysis recovery or profile-binding field participates in the fingerprint.
/// 验证每个分析恢复字段与画像绑定字段均参与指纹计算。
#[test]
fn analysis_fingerprint_covers_recovery_and_profile_bindings() {
    let original = request_fingerprint(&analyze_command(), SNAPSHOT_ID);
    let CoreCommand::PostActionAnalyzePendingTurn(base) = analyze_command() else {
        unreachable!("golden command is always PostActionAnalyzePendingTurn")
    };
    let changed_watermark =
        CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
            direct_write_observed_after_ns: 778,
            ..base.clone()
        });
    assert_ne!(
        request_fingerprint(&changed_watermark, SNAPSHOT_ID),
        original
    );
    let mut changed_cutoff = base.clone();
    changed_cutoff.analysis_cutoff_ns += 1;
    assert_ne!(
        request_fingerprint(
            &CoreCommand::PostActionAnalyzePendingTurn(changed_cutoff),
            SNAPSHOT_ID,
        ),
        original
    );
    let mut changed_observation = base.clone();
    changed_observation.mutation_identity.observed_at_ns += 1;
    assert_eq!(
        request_fingerprint(
            &CoreCommand::PostActionAnalyzePendingTurn(changed_observation),
            SNAPSHOT_ID,
        ),
        original
    );
    let mut changed_sections = base;
    changed_sections.profile_sections[0].display_label = "workspace".to_owned();
    assert_ne!(
        request_fingerprint(
            &CoreCommand::PostActionAnalyzePendingTurn(changed_sections),
            SNAPSHOT_ID,
        ),
        original
    );
}
