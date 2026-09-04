//! Canonical command codec round-trip tests over every public command variant.
//! 覆盖每个公共命令分支的规范命令编解码往返测试。

#![allow(
    clippy::expect_used,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::redundant_clone
)]

use vulcan_memorymesh_core_contracts::{
    ChatCompactCommand, CheckpointContractSet, CheckpointState, CommandCheckpoint, CommandHandle,
    CommandId, ConfigSnapshotId, CoreCommand, IdempotencyKey, MaintenanceAction, MemoryCategory,
    MemoryDeleteBatchCommand, MemoryDeleteBatchItem, MemoryDeleteCommand,
    MemoryDetailsBatchCommand, MemoryDetailsCommand, MemoryLevel, MemoryScopeClass,
    MemorySearchBatchCommand, MemorySearchCommand, MemoryWriteBatchCommand, MemoryWriteBatchItem,
    MemoryWriteCommand, MutationIdentity, MutationSourceKind, OperationId,
    PostActionAnalyzePendingTurnCommand, PostActionIntakeCommand, PreCheckCommand,
    PreCheckTurnContentType, PreCheckTurnContext, ProfileInstructionCommand, ProfileQueryCommand,
    ProfileView, ProfileViewMode, ProfileViewSection, RecallMode, RetentionCommand,
    RetentionPolicy, Role, ScopeAccess, ScopeId, SectionRole, SessionRecallMode, SessionSourceId,
    SubjectId, TimelineItem, TurnId, VectorSpaceId, VersionedMemoryRef, checkpoint_payload_digest,
    decode_checkpoint, decode_command, encode_checkpoint, encode_command,
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

/// Builds one shared identity reused by every mutating command.
/// 构建每个变更命令复用的共享身份。
fn identity() -> MutationIdentity {
    MutationIdentity {
        operation_id: OperationId::new("op-roundtrip").expect("valid operation"),
        idempotency_key: IdempotencyKey::new("roundtrip-key").expect("valid key"),
        request_fingerprint:
            "fp256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        source_kind: MutationSourceKind::HostRpc,
        request_source_id: "roundtrip-test".to_owned(),
        observed_at_ns: 1234,
    }
}

/// Builds one shared scope access.
/// 构建共享作用域访问。
fn scope() -> ScopeAccess {
    ScopeAccess {
        write_scope: ScopeId::new("scope-roundtrip").expect("valid scope"),
        visible_scopes: vec![ScopeId::new("scope-roundtrip").expect("valid scope")],
    }
}

/// Asserts one command survives a binary round trip byte-for-byte on fields.
/// 断言一个命令在字段级经历二进制往返后保持一致。
fn assert_round_trip(command: CoreCommand) {
    let bytes = encode_command(&command);
    let decoded = decode_command(&bytes).expect("decoded command");
    assert_eq!(
        format!("{command:?}"),
        format!("{decoded:?}"),
        "round-trip drift for kind {}",
        command.kind()
    );
}

/// A versioned envelope prevents old unframed commands from being reinterpreted after authorization fields change.
/// 带版本信封防止授权字段改变后旧无信封命令被重新解释。
#[test]
fn canonical_command_envelope_rejects_pre_scope_material() {
    let command = CoreCommand::MemoryDetails(MemoryDetailsCommand {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: scope(),
        memory_id: vulcan_memorymesh_core_contracts::MemoryId::new("memory").expect("memory"),
    });
    let mut encoded = encode_command(&command);
    assert_eq!(&encoded[..4], b"VMCI");
    assert!(decode_command(&encoded[8..]).is_err());
    encoded[4..8].copy_from_slice(&1_u32.to_le_bytes());
    assert!(decode_command(&encoded).is_err());
}

/// Freezes all sixteen public commands through the canonical checkpoint codec.
/// 将全部十六条公共命令冻结通过规范检查点编解码器。
#[test]
fn all_sixteen_commands_round_trip() {
    let subject = SubjectId::new("subject-roundtrip").expect("valid subject");
    let session = SessionSourceId::new("session-roundtrip").expect("valid session");
    let vector = VectorSpaceId::new("vector-roundtrip").expect("valid vector space");
    let identity = identity();
    let timeline = vec![TimelineItem {
        role: Role::User,
        content: "roundtrip 内容".to_owned(),
        occurred_at_ns: 100,
    }];

    assert_round_trip(CoreCommand::PreCheck(PreCheckCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        session_source_id: session.clone(),
        vector_space_id: vector.clone(),
        candidate_scope: vulcan_memorymesh_core_contracts::MemoryCandidateScope {
            scope_ids: scope().visible_scopes,
            origin_session_source_id: Some(session.clone()),
        },
        user_content: "中文内容 Ünïcodé ✓".to_owned(),
        recent_turns: vec![PreCheckTurnContext {
            turn_id: TurnId::new("turn-precheck-context").expect("valid turn identity"),
            content_type: PreCheckTurnContentType::RawTurn,
            content: r#"{"user":"roundtrip 内容","timeline":[],"assistant":""}"#.to_owned(),
        }],
        recall_mode: RecallMode::Force,
        session_recall_mode: SessionRecallMode::CompactAware,
        max_search_queries: 3,
        max_memories: 5,
        now_ns: 1000,
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::PostActionIntake(PostActionIntakeCommand {
        input_kind: vulcan_memorymesh_core_contracts::PostActionInputKind::SingleTurn,
        subject_id: subject.clone(),
        scope_access: scope(),
        session_source_id: session.clone(),
        vector_space_id: vector.clone(),
        timeline: timeline.clone(),
        completed_at_ns: 200,
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::PostActionAnalyzePendingTurn(
        PostActionAnalyzePendingTurnCommand {
            subject_id: subject.clone(),
            scope_access: scope(),
            replacement_scope: fixture_replacement_scope(&scope()),
            profile_sections: vec![ProfileViewSection {
                scope_id: ScopeId::new("scope-roundtrip").expect("valid scope"),
                role: SectionRole::Subject,
                display_label: "Subject".to_owned(),
            }],
            session_source_id: session.clone(),
            vector_space_id: vector.clone(),
            turn_id: TurnId::new("turn-roundtrip").expect("valid turn"),
            analysis_generation: 2,
            analysis_cutoff_ns: 1_234,
            direct_write_observed_after_ns: 150,
            mutation_identity: identity.clone(),
        },
    ));

    assert_round_trip(CoreCommand::ChatCompact(ChatCompactCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        session_source_id: session.clone(),
        compacted_at_ns: 300,
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::MemorySearch(MemorySearchCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        query: "golden query".to_owned(),
        limit: 7,
        vector_space_id: vector.clone(),
        boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
            candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                scope_ids: scope().visible_scopes,
                origin_session_source_id: None,
            }),
            session_recall_boundary: Some(
                vulcan_memorymesh_core_contracts::SessionRecallBoundary {
                    session_source_id: session.clone(),
                    mode: SessionRecallMode::CompactAware,
                },
            ),
        },
        now_ns: 400,
    }));

    assert_round_trip(CoreCommand::MemoryDetails(MemoryDetailsCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        memory_id: vulcan_memorymesh_core_contracts::MemoryId::new("memory-roundtrip")
            .expect("valid memory"),
    }));

    assert_round_trip(CoreCommand::MemoryWrite(MemoryWriteCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        replacement_scope: fixture_replacement_scope(&scope()),
        session_source_id: session.clone(),
        vector_space_id: vector.clone(),
        item: MemoryWriteBatchItem {
            target_scope_id: ScopeId::new("scope-roundtrip").expect("valid scope"),
            scope_class: MemoryScopeClass::Environment,
            category: MemoryCategory::ProjectContext,
            summary: "golden summary".to_owned(),
            details: "golden content".to_owned(),
            priority: Some(50),
            memory_level: Some(MemoryLevel::Stable),
            expires_at_ns: Some(10_000),
            tags: vec!["a".to_owned(), "b".to_owned()],
        },
        occurred_at_ns: 500,
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::MemoryDelete(MemoryDeleteCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        memory_id: vulcan_memorymesh_core_contracts::MemoryId::new("memory-roundtrip")
            .expect("valid memory"),
        expected_version: Some(3),
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::ProfileQuery(ProfileQueryCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        view: ProfileView {
            mode: ProfileViewMode::Full,
            display_timezone: "Asia/Shanghai".to_owned(),
            sections: vec![ProfileViewSection {
                scope_id: ScopeId::new("scope-roundtrip").expect("valid scope"),
                role: SectionRole::Subject,
                display_label: "Subject".to_owned(),
            }],
            include_explanations: true,
        },
        evaluated_at_ns: 1_700_000_000_500,
    }));

    assert_round_trip(CoreCommand::ProfileInstruction(ProfileInstructionCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        target_section: ProfileViewSection {
            scope_id: ScopeId::new("scope-roundtrip").expect("valid scope"),
            role: SectionRole::Subject,
            display_label: "Subject".to_owned(),
        },
        instruction: "保持简洁".to_owned(),
        occurred_at_ns: 600,
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::Retention(RetentionCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        now_ns: 700,
        policy: RetentionPolicy {
            schema_version: 1,
            turn_hot_window_size: 15,
            session_idle_ms: 1_800_000,
            trash_retention_ms: 86_400_000,
            protected_priority_floor: 80,
            protected_memory_level_floor: 4,
            skip_protected_shared_memories: false,
            profile_expiry_limit: 256,
            memory_recycle_limit: 100,
            cold_turn_scan_limit: 50,
            cold_turn_job_limit: 16,
            idle_session_limit: 32,
            vector_gc_limit: 100,
            trash_purge_limit: 64,
        },
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::MemorySearchBatch(MemorySearchBatchCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        queries: vec!["q1".to_owned(), "q2".to_owned()],
        top_k: 4,
        vector_space_id: vector.clone(),
        boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
            candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                scope_ids: scope().visible_scopes,
                origin_session_source_id: None,
            }),
            session_recall_boundary: Some(
                vulcan_memorymesh_core_contracts::SessionRecallBoundary {
                    session_source_id: session.clone(),
                    mode: SessionRecallMode::Full,
                },
            ),
        },
        now_ns: 800,
    }));

    assert_round_trip(CoreCommand::MemoryDetailsBatch(MemoryDetailsBatchCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        memory_ids: vec![
            vulcan_memorymesh_core_contracts::MemoryId::new("memory-a").expect("valid memory"),
            vulcan_memorymesh_core_contracts::MemoryId::new("memory-b").expect("valid memory"),
        ],
        turn_ids: vec![TurnId::new("turn-a").expect("valid turn")],
    }));

    assert_round_trip(CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        replacement_scope: fixture_replacement_scope(&scope()),
        session_source_id: session.clone(),
        vector_space_id: vector.clone(),
        authorized_write_scopes: vec![ScopeId::new("scope-roundtrip").expect("valid scope")],
        occurred_at_ns: 900,
        items: vec![MemoryWriteBatchItem {
            target_scope_id: ScopeId::new("scope-roundtrip").expect("valid scope"),
            scope_class: MemoryScopeClass::Environment,
            category: MemoryCategory::General,
            summary: "batch summary".to_owned(),
            details: "batch details".to_owned(),
            priority: Some(50),
            memory_level: Some(MemoryLevel::Stable),
            expires_at_ns: Some(9999),
            tags: vec!["batch".to_owned()],
        }],
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::MemoryDeleteBatch(MemoryDeleteBatchCommand {
        subject_id: subject.clone(),
        scope_access: scope(),
        reason: "roundtrip cleanup".to_owned(),
        items: vec![MemoryDeleteBatchItem {
            memory_id: vulcan_memorymesh_core_contracts::MemoryId::new("batch-memory")
                .expect("valid memory"),
            expected_version: Some(1),
        }],
        mutation_identity: identity.clone(),
    }));

    assert_round_trip(CoreCommand::Maintenance(
        vulcan_memorymesh_core_contracts::MaintenanceCommand {
            subject_id: subject.clone(),
            scope_access: scope(),
            occurred_at_ns: 1000,
            action: MaintenanceAction::RebuildVectors {
                memory_ids: vec![
                    vulcan_memorymesh_core_contracts::MemoryId::new("memory-a")
                        .expect("valid memory"),
                    vulcan_memorymesh_core_contracts::MemoryId::new("memory-b")
                        .expect("valid memory"),
                ],
                vector_space_id: VectorSpaceId::new("vector-rebuild").expect("valid vector space"),
            },
            mutation_identity: identity.clone(),
        },
    ));

    assert_round_trip(CoreCommand::Maintenance(
        vulcan_memorymesh_core_contracts::MaintenanceCommand {
            subject_id: subject,
            scope_access: scope(),
            occurred_at_ns: 1100,
            action: MaintenanceAction::RestoreRecycled(vec![VersionedMemoryRef {
                memory_id: vulcan_memorymesh_core_contracts::MemoryId::new("memory-a")
                    .expect("valid memory"),
                expected_version: 2,
            }]),
            mutation_identity: identity,
        },
    ));
}

/// Rejects structurally invalid command materials with a stable format error.
/// 拒绝结构无效的命令材料并返回稳定格式错误。
#[test]
fn truncated_command_material_is_rejected() {
    assert!(decode_command(&[1, 2, 3]).is_err());
    assert!(decode_command(&[]).is_err());
}

/// Builds one minimal canonical checkpoint for codec-integrity tests.
/// 构建一个用于编解码完整性测试的最小规范检查点。
fn checkpoint_fixture() -> CommandCheckpoint {
    let mut checkpoint = CommandCheckpoint {
        format_version: vulcan_memorymesh_core_contracts::CHECKPOINT_FORMAT_VERSION,
        handle: CommandHandle {
            command_id: CommandId::new("command-checkpoint").expect("valid command identity"),
            config_snapshot_id: ConfigSnapshotId::new("snapshot-checkpoint")
                .expect("valid snapshot identity"),
        },
        command_input: encode_command(&CoreCommand::MemoryDetails(MemoryDetailsCommand {
            subject_id: SubjectId::new("subject-checkpoint").expect("valid subject"),
            scope_access: scope(),
            memory_id: vulcan_memorymesh_core_contracts::MemoryId::new("memory-checkpoint")
                .expect("valid memory"),
        })),
        mutation_identity: None,
        state: CheckpointState::Ready,
        contracts: CheckpointContractSet {
            structured_output_contracts: vec!["contract\u{1f}with-separator".to_owned()],
            score_contracts: Vec::new(),
        },
        received_summary: vec!["summary\u{1f}with-separator".to_owned()],
        continuation: None,
        payload_digest: String::new(),
    };
    checkpoint.payload_digest =
        checkpoint_payload_digest(&checkpoint).expect("checkpoint digest must encode");
    checkpoint
}

/// Preserves list elements losslessly and rejects invalid flags or trailing bytes.
/// 无损保留列表元素，并拒绝非法标志或尾随字节。
#[test]
fn checkpoint_container_is_unambiguous_and_strict() {
    let checkpoint = checkpoint_fixture();
    let bytes = encode_checkpoint(&checkpoint).expect("checkpoint must encode");
    let decoded = decode_checkpoint(&bytes).expect("checkpoint must decode");
    assert_eq!(decoded, checkpoint);

    let mut invalid_flag = bytes.clone();
    *invalid_flag
        .last_mut()
        .expect("continuation flag must exist") = 2;
    assert!(decode_checkpoint(&invalid_flag).is_err());

    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_checkpoint(&trailing).is_err());
}

/// Rejects every older format even with a freshly computed digest instead of interpreting its private layout as current.
/// 即使摘要重新计算也拒绝全部旧格式，不能把旧私有布局按当前格式解释；无返回值。
#[test]
fn previous_checkpoint_format_is_explicitly_rejected() {
    for version in 1..vulcan_memorymesh_core_contracts::CHECKPOINT_FORMAT_VERSION {
        let mut checkpoint = checkpoint_fixture();
        checkpoint.format_version = version;
        checkpoint.payload_digest =
            checkpoint_payload_digest(&checkpoint).expect("old version digest");
        let bytes = encode_checkpoint(&checkpoint).expect("old version container");
        assert_eq!(
            decode_checkpoint(&bytes)
                .expect_err("old version rejected")
                .code,
            vulcan_memorymesh_core_contracts::ErrorCode::CheckpointIncompatible
        );
    }
}

/// Binds handle, exact snapshot, identity metadata, contracts, summaries, and continuation bytes.
/// 绑定句柄、精确快照、身份元数据、契约、摘要与续体字节。
#[test]
fn checkpoint_digest_binds_every_canonical_field() {
    // Describes one independent canonical-field mutation and its assertion label.
    // 描述一个独立规范字段变更及其断言标签。
    type FieldMutation = (&'static str, fn(&mut CommandCheckpoint));
    let checkpoint = checkpoint_fixture();
    let original = checkpoint_payload_digest(&checkpoint).expect("checkpoint digest must encode");

    let mut changed = checkpoint.clone();
    changed.handle.config_snapshot_id =
        ConfigSnapshotId::new("snapshot-other").expect("valid snapshot identity");
    assert_ne!(
        checkpoint_payload_digest(&changed).expect("changed checkpoint must encode"),
        original
    );

    changed = checkpoint.clone();
    changed.received_summary = vec!["summary".to_owned(), "with-separator".to_owned()];
    assert_ne!(
        checkpoint_payload_digest(&changed).expect("changed checkpoint must encode"),
        original
    );

    // Test every identity/continuation component independently, not only the presence of its container.
    // 独立检验每个身份/续体组件，而非仅检验其容器是否存在。
    let mut complete = checkpoint;
    complete.mutation_identity = Some(identity());
    complete.contracts.score_contracts = vec![vulcan_memorymesh_core_contracts::ScoreContractRef {
        id: "producer-one".to_owned(),
        version: 1,
    }];
    complete.continuation = Some(vulcan_memorymesh_core_contracts::CheckpointContinuation {
        flow_state: vec![1, 2, 3],
        pending_effect: Some(vec![4, 5]),
        next_effect_sequence: 7,
        pending_mutation_idempotency_key: Some(IdempotencyKey::new("key-one").expect("valid key")),
        degraded: false,
    });
    let original = checkpoint_payload_digest(&complete).expect("complete digest");
    let mutations: &[FieldMutation] = &[
        ("format_version", |c| c.format_version += 1),
        ("command_id", |c| {
            c.handle.command_id = CommandId::new("other").expect("valid ID");
        }),
        ("snapshot", |c| {
            c.handle.config_snapshot_id = ConfigSnapshotId::new("other").expect("valid ID");
        }),
        ("command_input", |c| c.command_input.push(0)),
        ("state", |c| c.state = CheckpointState::Terminal),
        ("structured_contracts", |c| {
            c.contracts
                .structured_output_contracts
                .push("other".to_owned());
        }),
        ("score_id", |c| c.contracts.score_contracts[0].id.push('x')),
        ("score_version", |c| {
            c.contracts.score_contracts[0].version += 1;
        }),
        ("summary", |c| c.received_summary.push("other".to_owned())),
        ("identity_presence", |c| c.mutation_identity = None),
        ("operation", |c| {
            c.mutation_identity
                .as_mut()
                .expect("fixture identity")
                .operation_id = OperationId::new("other").expect("valid ID");
        }),
        ("idempotency_key", |c| {
            c.mutation_identity
                .as_mut()
                .expect("fixture identity")
                .idempotency_key = IdempotencyKey::new("other").expect("valid ID");
        }),
        ("fingerprint", |c| {
            c.mutation_identity
                .as_mut()
                .expect("fixture identity")
                .request_fingerprint
                .push('x');
        }),
        ("source_kind", |c| {
            c.mutation_identity
                .as_mut()
                .expect("fixture identity")
                .source_kind = MutationSourceKind::Import;
        }),
        ("source_id", |c| {
            c.mutation_identity
                .as_mut()
                .expect("fixture identity")
                .request_source_id
                .push('x');
        }),
        ("observed_at", |c| {
            c.mutation_identity
                .as_mut()
                .expect("fixture identity")
                .observed_at_ns += 1;
        }),
        ("continuation_presence", |c| c.continuation = None),
        ("flow", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .flow_state
                .push(0);
        }),
        ("effect_presence", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .pending_effect = None;
        }),
        ("effect", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .pending_effect
                .as_mut()
                .expect("fixture effect")
                .push(0);
        }),
        ("sequence", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .next_effect_sequence += 1;
        }),
        ("pending_key_presence", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .pending_mutation_idempotency_key = None;
        }),
        ("pending_key", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .pending_mutation_idempotency_key =
                Some(IdempotencyKey::new("other").expect("valid key"));
        }),
        ("degraded", |c| {
            c.continuation
                .as_mut()
                .expect("fixture continuation")
                .degraded = true;
        }),
    ];
    for (name, mutate) in mutations {
        let mut changed = complete.clone();
        mutate(&mut changed);
        assert_ne!(
            checkpoint_payload_digest(&changed).expect("changed digest"),
            original,
            "unbound field: {name}"
        );
    }
    complete.payload_digest = "digest is deliberately excluded from itself".to_owned();
    assert_eq!(
        checkpoint_payload_digest(&complete).expect("unsigned digest"),
        original
    );
}
