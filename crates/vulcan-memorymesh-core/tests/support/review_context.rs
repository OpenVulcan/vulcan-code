//! Tests actual public restore against well-hashed but source-inconsistent L2 requests.
//! 对实际公共恢复入口测试摘要正确但L2来源不一致的请求。

use super::*;
use vulcan_memorymesh_core::{
    PostActionInputKind, PostActionReviewContext, checkpoint_payload_digest, decode_checkpoint,
    encode_checkpoint,
};

/// Builds a valid explicit-write item without optional lifecycle overrides.
/// 构造没有可选生命周期覆盖的有效显式写入项；参数为精确作用域，返回夹具项目。
fn item(scope: ScopeId) -> MemoryWriteBatchItem {
    MemoryWriteBatchItem {
        target_scope_id: scope,
        scope_class: MemoryScopeClass::Environment,
        category: MemoryCategory::ArchitectureDecision,
        summary: "source context fact".to_owned(),
        details: "source context fact".to_owned(),
        priority: None,
        memory_level: None,
        expires_at_ns: None,
        tags: vec![],
    }
}

/// Starts the declared source through normal public APIs and stops at its actual L2 effect.
/// 通过正常公共API启动声明来源并停在实际L2效果；参数为来源场景，返回Core、句柄及完整等待步骤。
fn waiting(
    scenario: &str,
) -> (
    MemoryMeshCore,
    vulcan_memorymesh_core::CommandHandle,
    CommandStep,
) {
    let core = MemoryMeshCore::new().expect("Core");
    let mut host = DeterministicHost::new();
    let access = ScopeAccess {
        write_scope: ScopeId::new("review-scope").expect("scope"),
        visible_scopes: vec![ScopeId::new("review-scope").expect("scope")],
    };
    let subject = SubjectId::new("review-subject").expect("subject");
    let session = SessionSourceId::new("review-session").expect("session");
    let space = VectorSpaceId::new("review-vector").expect("vector");
    let command = if scenario == "analyze" {
        let intake = with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionIntake(PostActionIntakeCommand {
                    input_kind: PostActionInputKind::Timeline,
                    subject_id: subject.clone(),
                    scope_access: access.clone(),
                    session_source_id: session.clone(),
                    vector_space_id: space.clone(),
                    timeline: vec![TimelineItem {
                        role: Role::User,
                        content: "Remember the explicit architecture rule".to_owned(),
                        occurred_at_ns: 100,
                    }],
                    completed_at_ns: 200,
                    mutation_identity: identity,
                })
            },
            "review-intake",
            200,
        );
        let CommandResult::PostActionAccepted(accepted) =
            core.run_to_completion(intake, &mut host).expect("intake")
        else {
            panic!("accepted turn")
        };
        with_identity(
            &core,
            |identity| {
                CoreCommand::PostActionAnalyzePendingTurn(PostActionAnalyzePendingTurnCommand {
                    subject_id: subject.clone(),
                    scope_access: access.clone(),
                    replacement_scope: fixture_replacement_scope(&access),
                    profile_sections: vec![ProfileViewSection {
                        scope_id: access.write_scope.clone(),
                        role: SectionRole::Subject,
                        display_label: "Subject".to_owned(),
                    }],
                    session_source_id: session.clone(),
                    vector_space_id: space.clone(),
                    turn_id: accepted.turn_id.expect("turn"),
                    analysis_generation: 1,
                    analysis_cutoff_ns: 900,
                    direct_write_observed_after_ns: 0,
                    mutation_identity: identity,
                })
            },
            "review-analyze",
            900,
        )
    } else if scenario == "single" {
        with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWrite(MemoryWriteCommand {
                    replacement_scope: fixture_replacement_scope(&access),
                    subject_id: subject.clone(),
                    scope_access: access.clone(),
                    session_source_id: session.clone(),
                    vector_space_id: space.clone(),
                    item: item(access.write_scope.clone()),
                    occurred_at_ns: 900,
                    mutation_identity: identity,
                })
            },
            "review-single",
            900,
        )
    } else {
        assert_eq!(scenario, "batch");
        with_identity(
            &core,
            |identity| {
                CoreCommand::MemoryWriteBatch(MemoryWriteBatchCommand {
                    replacement_scope: fixture_replacement_scope(&access),
                    subject_id: subject,
                    scope_access: access.clone(),
                    authorized_write_scopes: vec![access.write_scope.clone()],
                    session_source_id: session,
                    vector_space_id: space,
                    items: vec![item(access.write_scope.clone())],
                    occurred_at_ns: 900,
                    mutation_identity: identity,
                })
            },
            "review-batch",
            900,
        )
    };
    let (handle, mut step) = core.start(command).expect("start");
    for _ in 0..100 {
        let CommandStep::Effects(effects) = &step else {
            panic!("expected L2")
        };
        assert_eq!(effects.len(), 1);
        let request = effects[0].clone();
        if matches!(&request.effect, HostEffect::Llm(effect) if effect.purpose == LlmPurpose::PostActionReview)
        {
            return (core, handle, step);
        }
        step = core
            .resume(&handle, host.execute_effect(request).expect("host"))
            .expect("resume");
    }
    panic!("L2 not reached");
}

/// Rejects drift after recomputing the exact binary payload digest; a fresh instance accepts the unchanged control first.
/// 在重新计算精确二进制摘要后仍拒绝漂移；新实例先接受未修改的对照检查点。
#[test]
fn rehashed_review_source_drift_is_rejected() {
    let codec = bincode::config::standard()
        .with_little_endian()
        .with_fixed_int_encoding()
        .with_limit::<16_777_216>();
    for scenario in ["single", "batch", "analyze"] {
        let (core, handle, original_step) = waiting(scenario);
        let bytes = core.export_checkpoint(&handle).expect("export");
        let fresh = MemoryMeshCore::new().expect("fresh Core");
        let (restored, step) = fresh.restore_checkpoint(&bytes).expect("unchanged restore");
        assert_eq!(step, original_step);
        fresh.cancel(&restored).expect("cancel control");
        fresh.release_command(&restored).expect("release control");
        for change in 0..if scenario == "analyze" { 5 } else { 2 } {
            let mut checkpoint = decode_checkpoint(&bytes).expect("decode checkpoint");
            let continuation = checkpoint.continuation.as_mut().expect("continuation");
            let original = continuation.pending_effect.as_ref().expect("effect bytes");
            let (mut request, consumed): (EffectRequest, usize) =
                bincode::serde::decode_from_slice(original, codec).expect("decode effect");
            assert_eq!(consumed, original.len());
            let HostEffect::Llm(effect) = &mut request.effect else {
                panic!("LLM")
            };
            let LlmInput::PostActionReview {
                context,
                memory_candidates,
                ..
            } = &mut effect.input
            else {
                panic!("L2")
            };
            if scenario == "analyze" {
                let PostActionReviewContext::Turn {
                    target_turn,
                    user_input_kind,
                    profile_sections,
                } = context
                else {
                    panic!("turn source")
                };
                match change {
                    0 => target_turn.id = TurnId::new("foreign-turn").expect("turn"),
                    1 => target_turn.created_at_ns += 1,
                    2 => {
                        *user_input_kind = if *user_input_kind == TurnInputKind::Question {
                            TurnInputKind::Statement
                        } else {
                            TurnInputKind::Question
                        }
                    }
                    3 => {
                        profile_sections[0].scope_id =
                            ScopeId::new("foreign-scope").expect("scope");
                    }
                    4 => memory_candidates[0].candidate.summary.push_str(" changed"),
                    _ => unreachable!(),
                }
            } else if change == 0 {
                let PostActionReviewContext::DirectWrite { occurred_at_ns } = context else {
                    panic!("direct source")
                };
                *occurred_at_ns += 1;
            } else {
                memory_candidates[0].candidate.summary.push_str(" changed");
            }
            continuation.pending_effect =
                Some(bincode::serde::encode_to_vec(&request, codec).expect("encode effect"));
            checkpoint.payload_digest = checkpoint_payload_digest(&checkpoint).expect("rehash");
            let forged = encode_checkpoint(&checkpoint).expect("encode checkpoint");
            let isolated = MemoryMeshCore::new().expect("isolated Core");
            let error = isolated
                .restore_checkpoint(&forged)
                .expect_err("source drift must fail");
            assert_eq!(error.code, ErrorCode::CheckpointIncompatible);
        }
        core.cancel(&handle).expect("cancel");
        core.release_command(&handle).expect("release");
    }
}
