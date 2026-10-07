//! Initial and resumed creation intent persistence and failure-atomic ownership tests.
//! 初始及续跑创建意图持久化与失败原子归属验证。

/// Builds successor settings from the original captured runtime fields without inferring any identity.
/// 从原捕获运行时字段构造后继设置，不推断任何身份。
/// `profile` is the original capture; returns the same route with a distinct execution and prompt.
/// `profile` 为原捕获；返回相同路由及不同的执行和提示。
fn resumption_preparation(profile: ChildExecutionProfile) -> ChildExecutionPreparation {
    let mut invocation_config = profile.invocation_config;
    invocation_config.prompt = "Continue the original child work".into();
    ChildExecutionPreparation {
        task_id: uuid::Uuid::parse_str(profile.task_id.as_str()).unwrap(),
        root_session_id: uuid::Uuid::parse_str(&profile.root_session_id).unwrap(),
        parent_session_id: uuid::Uuid::parse_str(&profile.parent_session_id).unwrap(),
        execution_id: "child-successor-execution".into(),
        runtime_identity: profile.runtime_identity,
        invocation_config,
        resolved_config: profile.resolved_config,
        effective_config: profile.effective_config,
        runtime_policy: profile.runtime_policy,
        execution_agent_id: profile.execution_agent_id,
        connection_id: profile.connection_id,
        temperature: profile.temperature,
        admitted_tools: profile.admitted_tools,
        request_overrides: profile.request_overrides,
        fixed_agent_prompt: profile.fixed_agent_prompt,
        prompt_context: profile.prompt_context,
    }
}

/// Parent-owned resumption survives reload and publishes the real successor in one child revision.
/// 父侧续跑来源可重载，并在一个子修订中发布真实后继。
/// Returns nothing; original history is retained and exact retries do not modify either document.
/// 不返回值；原历史被保留，精确重试不修改任一文档。
#[test]
fn child_resumption_intent_preserves_history_and_exact_replay() {
    let (mut parent, mut child, mut reservation, initial) = creation_fixture();
    let original = child.clone();
    let (successor, intent) = child
        .prepare_child_resumption(resumption_preparation(initial.profile), 10)
        .unwrap();
    assert_eq!(child, original);
    assert_eq!(
        intent.origin,
        ChildCreationOrigin::Resumption {
            base_document_revision: original.memory_revision(),
        }
    );
    assert!(intent.rebuild_initial_document().is_err());
    assert_eq!(successor.memory_revision(), original.memory_revision() + 1);
    assert_ne!(
        intent.profile.business_turn_id,
        original.active_turn().unwrap().turn_id
    );
    assert!(successor.events().starts_with(original.events()));
    reservation.execution_id =
        CollaborationExecutionId::try_new(&intent.profile.business_execution_id).unwrap();
    reservation.operation_id = CollaborationOperationId::try_new("resume-reservation").unwrap();
    reservation.reserved_at_ms = 10;
    let parent_before = parent.memory_revision();
    parent
        .reserve_child_creation(reservation.clone(), intent.clone())
        .unwrap();
    assert_eq!(parent.memory_revision(), parent_before + 1);
    let parent = reload(&parent).unwrap();
    let durable_intent = &parent.metadata.child_creation_intents[&reservation.execution_id];
    assert_eq!(durable_intent, &intent);
    assert!(child.publish_child_resumption(durable_intent).unwrap());
    assert_eq!(child, successor);
    let published = reload(&child).unwrap();
    assert_eq!(published.events(), successor.events());
    let unchanged = child.clone();
    assert!(!child.publish_child_resumption(durable_intent).unwrap());
    assert_eq!(child, unchanged);
    let mut encoded = serde_json::to_value(&intent).unwrap();
    encoded.as_object_mut().unwrap().remove("origin");
    assert!(serde_json::from_value::<ChildCreationIntent>(encoded).is_err());
}

/// Wrong source revisions, allocated coordinates and origins never stop or overwrite the predecessor.
/// 错误来源修订、已分配坐标和模式绝不停止或覆盖前序。
/// Returns nothing; rejected publication and preparation preserve all source metadata and events.
/// 不返回值；被拒绝的发布与准备保留全部来源元数据及事件。
#[test]
fn child_resumption_intent_rejects_changed_sources_atomically() {
    let (_, mut child, _, initial) = creation_fixture();
    let preparation = resumption_preparation(initial.profile.clone());
    let (_, intent) = child
        .prepare_child_resumption(preparation.clone(), 10)
        .unwrap();
    let original = child.clone();
    let mut invalid_turn = intent.clone();
    invalid_turn.profile.business_turn_id = TurnId::try_new(999).unwrap();
    let mut invalid_revision = intent.clone();
    invalid_revision.origin = ChildCreationOrigin::Resumption {
        base_document_revision: child.memory_revision() + 1,
    };
    for invalid in [initial, invalid_turn, invalid_revision] {
        assert!(child.publish_child_resumption(&invalid).is_err());
        assert_eq!(child, original);
    }
    let mut invalid_preparation = preparation;
    invalid_preparation.invocation_config.prompt.clear();
    assert!(
        child
            .prepare_child_resumption(invalid_preparation, 10)
            .is_err()
    );
    assert_eq!(child, original);
    child
        .set_collaboration_run_control(crate::v1::collaboration::SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop-before-resume-publication")
                .unwrap(),
            state: crate::v1::collaboration::CollaborationRunControl::Stopped,
            changed_at_ms: 11,
        })
        .unwrap();
    let stopped = child.clone();
    assert!(child.publish_child_resumption(&intent).is_err());
    assert_eq!(child, stopped);
}

use super::*;
use crate::v1::child_execution::{ChildCreationIntent, ChildCreationOrigin};
use crate::v1::collaboration::{
    ClearGenerationOwnershipInput, CollaborationExecutionId, CollaborationOperationId,
    CollaborationParticipant, ReserveChildTaskInput,
};

/// Creates real parent/child allocators and the exact reservation without publishing either.
/// 创建真实父子分配器及精确预留，不发布任何一方。
/// Returns the parent, child prototype, matching reservation and immutable creation source.
/// 返回父文档、子原型、匹配预留和不可变创建来源。
fn creation_fixture() -> (
    SessionDocument,
    SessionDocument,
    ReserveChildTaskInput,
    ChildCreationIntent,
) {
    // Settings are populated by the existing capture fixture; physical identities are unique here.
    // 设置由已有捕获夹具完整提供；物理身份在此独立分配。
    let (_, settings) = fixture();
    let preparation = ChildExecutionPreparation {
        task_id: uuid::Uuid::new_v4(),
        root_session_id: uuid::Uuid::new_v4(),
        parent_session_id: uuid::Uuid::new_v4(),
        execution_id: settings.business_execution_id,
        runtime_identity: settings.runtime_identity,
        invocation_config: settings.invocation_config,
        resolved_config: settings.resolved_config,
        effective_config: settings.effective_config,
        runtime_policy: settings.runtime_policy,
        execution_agent_id: settings.execution_agent_id,
        connection_id: settings.connection_id,
        temperature: settings.temperature,
        admitted_tools: settings.admitted_tools,
        request_overrides: settings.request_overrides,
        fixed_agent_prompt: settings.fixed_agent_prompt,
        prompt_context: settings.prompt_context,
    };
    // Both turns are allocated by actual root submission, not synthetic numeric IDs.
    // 两个回合均由实际根提交分配，不使用合成数字 ID。
    let mut parent = SessionDocument::new(
        preparation.parent_session_id.to_string(),
        "project",
        "Parent",
        1,
    );
    let parent_turn = parent
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("parent-work").unwrap(),
                "Inspect project",
                vec![],
            ),
            2,
        )
        .unwrap()
        .into_value();
    let (child, intent) =
        SessionDocument::prepare_initial_child_creation(preparation, "project", 3).unwrap();
    let reservation = ReserveChildTaskInput {
        source_tool_name: "spawn_agent".into(),
        execution_id: CollaborationExecutionId::try_new(&intent.profile.business_execution_id)
            .unwrap(),
        operation_id: CollaborationOperationId::try_new("creation-reservation").unwrap(),
        task_id: intent.profile.task_id.clone(),
        parent_turn_id: parent_turn,
        child: CollaborationParticipant::new(
            intent.profile.task_id.as_str(),
            &intent.profile.root_session_id,
            "project",
            child.segment_generation(),
            None,
            Some(intent.profile.task_id.clone()),
        )
        .unwrap(),
        reserved_at_ms: 4,
    };
    (parent, child, reservation, intent)
}

/// Rehydrates the exact persisted representation through the production validation entry.
/// 通过生产校验入口重建精确持久表示。
/// `document` supplies storage parts; returns strict reconstruction or its domain error.
/// `document` 提供存储组成；返回严格重建结果或领域错误。
fn reload(document: &SessionDocument) -> Result<SessionDocument, DomainError> {
    SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
}

/// Parent creation publication is one revision and restores the exact child before any child file exists.
/// 父创建发布仅占一个修订，并在任何子文件存在前恢复精确子文档。
/// Returns nothing; checks obligations, retries, strict serialization and complete document reconstruction.
/// 不返回值；检查完成约束、重试、严格序列化和完整文档重建。
#[test]
fn child_creation_intent_publishes_with_reservation_and_rebuilds() {
    let (mut parent, child, reservation, intent) = creation_fixture();
    let revision = parent.memory_revision();
    let event_count = parent.events().len();
    parent
        .reserve_child_creation(reservation.clone(), intent.clone())
        .unwrap();
    assert_eq!(parent.memory_revision(), revision + 1);
    assert_eq!(parent.events().len(), event_count + 1);
    assert_eq!(
        parent
            .completion_obligations(reservation.parent_turn_id)
            .open_child_task_ids,
        vec![reservation.task_id.clone()]
    );
    // Exact retry changes neither the reserved obligation nor its creation settings.
    // 精确重试既不改变已预留义务，也不改变创建设置。
    let before_retry = parent.clone();
    assert!(matches!(
        parent
            .reserve_child_creation(reservation.clone(), intent.clone())
            .unwrap(),
        IdempotentMutation::Idempotent(_)
    ));
    assert_eq!(parent, before_retry);
    let candidate = parent.prepare_flush_candidates().unwrap().unwrap();
    assert_eq!(candidate.revision, revision + 1);
    assert_eq!(
        candidate
            .metadata
            .child_creation_intents
            .get(&reservation.execution_id),
        Some(&intent)
    );
    parent.advance_durable(candidate.revision).unwrap();
    let reloaded = reload(&parent).unwrap();
    let restored_child = reloaded.metadata.child_creation_intents[&reservation.execution_id]
        .rebuild_initial_document()
        .unwrap();
    assert_eq!(restored_child, child);
    // The new contract requires the creation collection; no missing-field compatibility is admitted.
    // 新契约要求创建集合存在，不接受缺失字段兼容。
    let mut missing = serde_json::to_value(&parent.metadata).unwrap();
    missing
        .as_object_mut()
        .unwrap()
        .remove("child_creation_intents");
    assert!(serde_json::from_value::<crate::v1::config::MetadataDocument>(missing).is_err());
}

/// Invalid allocation and foreign parent bindings roll back the entire composite reservation.
/// 无效分配及外来父绑定会回滚整个复合预留。
/// Returns nothing; later configuration replacement and retrofitted plain reservations are also rejected.
/// 不返回值；同时拒绝后续配置替换及为普通预留事后补造创建来源。
#[test]
fn child_creation_intent_rejects_conflicts_without_partial_reservation() {
    let (mut parent, _, reservation, intent) = creation_fixture();
    let untouched = parent.clone();
    for invalid in [
        ChildCreationIntent {
            profile: ChildExecutionProfile {
                parent_session_id: uuid::Uuid::new_v4().to_string(),
                ..intent.profile.clone()
            },
            ..intent.clone()
        },
        ChildCreationIntent {
            profile: ChildExecutionProfile {
                segment_generation: intent.profile.segment_generation + 1,
                ..intent.profile.clone()
            },
            ..intent.clone()
        },
        ChildCreationIntent {
            profile: ChildExecutionProfile {
                business_turn_id: TurnId::try_new(999).unwrap(),
                ..intent.profile.clone()
            },
            ..intent.clone()
        },
        ChildCreationIntent {
            project_tag: "foreign-project".into(),
            ..intent.clone()
        },
    ] {
        assert!(
            parent
                .reserve_child_creation(reservation.clone(), invalid)
                .is_err()
        );
        assert_eq!(parent, untouched);
    }
    parent
        .reserve_child_creation(reservation.clone(), intent.clone())
        .unwrap();
    let captured = parent.clone();
    let mut changed = intent.clone();
    changed.profile.connection_id = "another-connection".into();
    assert!(
        parent
            .reserve_child_creation(reservation.clone(), changed)
            .is_err()
    );
    assert_eq!(parent, captured);
    let mut plain = untouched;
    plain.reserve_child_task(reservation.clone()).unwrap();
    let before_retrofit = plain.clone();
    assert!(plain.reserve_child_creation(reservation, intent).is_err());
    assert_eq!(plain, before_retrofit);
}

/// Cold loading rejects orphan sources and generation clearing removes source and obligation together.
/// 冷加载拒绝孤立来源，代际清理一起移除来源和义务。
/// Returns nothing; verifies failed decoding and the single-revision reset through durable reload.
/// 不返回值；通过可靠重载验证解码拒绝及单修订重置。
#[test]
fn child_creation_intent_reload_and_generation_reset_preserve_ownership() {
    let (mut parent, _, reservation, intent) = creation_fixture();
    let mut orphan = parent.clone();
    orphan
        .metadata
        .child_creation_intents
        .insert(reservation.execution_id.clone(), intent.clone());
    assert!(reload(&orphan).is_err());
    parent
        .reserve_child_creation(reservation.clone(), intent)
        .unwrap();
    let mut forged = parent.clone();
    forged
        .metadata
        .child_creation_intents
        .get_mut(&reservation.execution_id)
        .unwrap()
        .profile
        .parent_session_id = uuid::Uuid::new_v4().to_string();
    assert!(reload(&forged).is_err());
    let revision = parent.memory_revision();
    parent
        .clear_executable_collaboration_ownership(ClearGenerationOwnershipInput {
            operation_id: CollaborationOperationId::try_new("reset-creation-owner").unwrap(),
            reset_at_ms: 5,
        })
        .unwrap();
    assert_eq!(parent.memory_revision(), revision + 1);
    assert!(parent.metadata.child_creation_intents.is_empty());
    assert!(
        parent
            .completion_obligations(reservation.parent_turn_id)
            .open_child_task_ids
            .is_empty()
    );
    let candidate = parent.prepare_flush_candidates().unwrap().unwrap();
    parent.advance_durable(candidate.revision).unwrap();
    assert!(
        reload(&parent)
            .unwrap()
            .metadata
            .child_creation_intents
            .is_empty()
    );
}
