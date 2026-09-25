//! Durable admission tests for captured but unactivated child executions.
//! 已捕获但尚未激活子执行的可靠准入验证。

use super::*;
use vulcan_session_core::v1::ClientActionId;
use vulcan_session_core::v1::child_execution::{
    ChildExecutionPreparation, ChildExecutionProfile, ChildExecutionPromptContext,
    ChildExecutionRuntimeIdentity,
};
use vulcan_session_core::v1::document::ClientInputSubmission;
use vulcan_session_core::{
    ExecutionRuntimePolicy, TaskInvocationConfig, TaskSessionResolvedConfig,
};
use vulcan_workspace_store::workspace_store::{
    CreateEmbeddedTaskSessionV1Request, create_embedded_task_session_v1,
    create_session_v1_storage_fixture,
};

/// A real absent child is rebuilt from its original intent and an existing publication is untouched.
/// 真实缺失的子任务由原意图重建，已有发布保持不变。
/// Returns nothing; disk reload proves original identity, exact initial history and idempotency.
/// 不返回值；磁盘重载证明原身份、精确初始历史及幂等性。
#[test]
fn missing_initial_child_publication_preserves_original_source() {
    use super::super::tree::creation::publish_missing;
    let (_temporary, parent, expected, record, profile) = reserved_creation_source();
    let root_locator = parent.storage_locator().unwrap();
    let locator = locate_embedded_task_session_v1(&root_locator, &record.child.session_id).unwrap();
    let generation = parent
        .read(|document| document.segment_generation())
        .unwrap();
    let revision = parent.read(|document| document.durable_revision()).unwrap();
    assert!(locator.read_collaboration_recovery_snapshot().is_err());
    let snapshot = publish_missing(&parent, &parent, generation, generation, &locator, &record)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.segment_generation, profile.segment_generation);
    let first = load_session_v1_for_runtime(&locator).unwrap().document;
    assert_eq!(
        first.metadata.child_execution_profile.as_ref(),
        Some(&profile)
    );
    assert_eq!(
        first.active_turn().unwrap().turn_id,
        profile.business_turn_id
    );
    assert_eq!(
        serde_json::to_value(first.events()).unwrap(),
        serde_json::to_value(expected.events()).unwrap()
    );
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision
    );
    assert!(
        publish_missing(&parent, &parent, generation, generation, &locator, &record)
            .unwrap()
            .is_some()
    );
    let repeated = load_session_v1_for_runtime(&locator).unwrap().document;
    assert_eq!(
        serde_json::to_value(&first.metadata).unwrap(),
        serde_json::to_value(&repeated.metadata).unwrap()
    );
    assert_eq!(
        serde_json::to_value(first.events()).unwrap(),
        serde_json::to_value(repeated.events()).unwrap()
    );
}

/// Stale discovery and explicit stop never create missing child storage.
/// 过期发现和明确停止绝不创建缺失子存储。
/// Returns nothing; real storage absence is checked after each rejected admission.
/// 不返回值；每次准入拒绝后检查真实存储仍不存在。
#[test]
fn missing_initial_child_publication_rejects_stale_owner_and_stop() {
    use super::super::tree::creation::publish_missing;
    let (_temporary, parent, _expected, record, _profile) = reserved_creation_source();
    let locator = locate_embedded_task_session_v1(
        &parent.storage_locator().unwrap(),
        &record.child.session_id,
    )
    .unwrap();
    let generation = parent
        .read(|document| document.segment_generation())
        .unwrap();
    assert!(
        publish_missing(
            &parent,
            &parent,
            generation + 1,
            generation,
            &locator,
            &record
        )
        .is_err()
    );
    let mut changed = record.clone();
    changed.source_tool_name = "different-tool".into();
    assert!(publish_missing(&parent, &parent, generation, generation, &locator, &changed).is_err());
    assert!(locator.read_collaboration_recovery_snapshot().is_err());
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.set_collaboration_run_control(
                    vulcan_session_core::v1::collaboration::SetRunControlInput {
                        operation_id: CollaborationOperationId::try_new(
                            "stop-before-child-publication",
                        )
                        .unwrap(),
                        state: CollaborationRunControl::MainAndSubagents,
                        changed_at_ms: 6,
                    },
                )
            },
        )
        .unwrap();
    assert!(
        publish_missing(&parent, &parent, generation, generation, &locator, &record)
            .unwrap()
            .is_none()
    );
    assert!(locator.read_collaboration_recovery_snapshot().is_err());
    // Cancellation before first publication still releases the durable parent obligation.
    // 首次发布前取消仍须解除可靠父义务。
    let restored = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
        .unwrap()
        .document;
    // The original reservation is terminal without a fabricated activation or child response.
    // 原预留终结，但不制造激活或子回复。
    let stopped = &restored.collaboration_state().child_tasks[&record.execution_id];
    assert_eq!(stopped.state, CollaborationChildTaskState::Terminal);
    assert_eq!(
        stopped.terminal_outcome,
        Some(
            vulcan_session_core::v1::collaboration::CollaborationChildTaskTerminalOutcome::Stopped,
        )
    );
    assert!(stopped.active_at_ms.is_none());
    assert!(stopped.non_success_result.is_none());
    assert!(
        restored
            .completion_obligations(record.parent_turn_id)
            .open_child_task_ids
            .is_empty()
    );
}

/// Creates a durable parent reservation before publishing its fully captured initial child.
/// 在发布完整捕获的初始子任务之前创建可靠父预留。
/// Returns storage ownership, parent handle, unpublished child, reservation and profile.
/// 返回存储所有者、父句柄、尚未发布子文档、预留及配置。
pub(in crate::agent_collaboration) fn reserved_creation_source() -> (
    tempfile::TempDir,
    Arc<SessionHandle>,
    SessionDocument,
    CollaborationChildTaskRecord,
    ChildExecutionProfile,
) {
    reserved_creation_source_with_preparation(|_| {})
}

/// Applies `configure` before Core captures the initial profile; returns the original creation fixture.
/// 在 Core 捕获初始配置之前应用 `configure`；返回原始创建夹具。
pub(in crate::agent_collaboration) fn reserved_creation_source_with_preparation(
    configure: impl FnOnce(&mut ChildExecutionPreparation),
) -> (
    tempfile::TempDir,
    Arc<SessionHandle>,
    SessionDocument,
    CollaborationChildTaskRecord,
    ChildExecutionProfile,
) {
    // The indexed root owns all embedded storage; the child UUID is independently allocated.
    // 索引根拥有全部嵌入存储；子 UUID 独立分配。
    let temporary = tempfile::tempdir().unwrap();
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    let root_locator =
        create_session_v1_storage_fixture(temporary.path(), &parent_id.to_string(), 1).unwrap();
    let parent_document = load_session_v1_for_runtime(&root_locator).unwrap().document;
    let parent = SessionHandle::new(parent_id, root_locator.clone(), parent_document);
    let parent_turn = parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.submit_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("parent-business").unwrap(),
                        "Review project",
                        vec![],
                    ),
                    2,
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    // The domain allocator freezes actual child coordinates before the parent records creation.
    // 领域分配器在父侧记录创建之前冻结实际子坐标。
    let mut preparation = ChildExecutionPreparation {
        runtime_identity: ChildExecutionRuntimeIdentity {
            public_task_id: "prepared-reviewer".into(),
            display_title: "Review".into(),
            description: None,
            subagent: None,
            depth: 1,
        },
        invocation_config: TaskInvocationConfig::main("Review code"),
        resolved_config: TaskSessionResolvedConfig {
            requested_config: Default::default(),
            effective_config: Default::default(),
            functional_system_prompt_layers: vec![],
            runtime_policy: ExecutionRuntimePolicy::task_session(),
        },
        task_id: child_id,
        execution_id: "original-child-execution".into(),
        root_session_id: parent_id,
        parent_session_id: parent_id,
        effective_config: Default::default(),
        runtime_policy: ExecutionRuntimePolicy::task_session(),
        execution_agent_id: Some("main".into()),
        connection_id: "captured-test-connection".into(),
        temperature: Some(0.25),
        admitted_tools: Default::default(),
        request_overrides: Default::default(),
        fixed_agent_prompt: None,
        prompt_context: ChildExecutionPromptContext {
            identity: "prepared-reviewer".into(),
            delegated_task: "Review code".into(),
            delegation: None,
            functional_system_prompt: None,
        },
    };
    configure(&mut preparation);
    let (child_document, intent) =
        SessionDocument::prepare_initial_child_creation(preparation, root_locator.project_id(), 3)
            .unwrap();
    let profile = intent.profile.clone();
    let record = parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_creation(
                    ReserveChildTaskInput {
                        source_tool_name: "spawn_agent".into(),
                        execution_id: CollaborationExecutionId::try_new(
                            &profile.business_execution_id,
                        )
                        .unwrap(),
                        operation_id: CollaborationOperationId::try_new(
                            "original-child-reservation",
                        )
                        .unwrap(),
                        task_id: profile.task_id.clone(),
                        parent_turn_id: parent_turn,
                        child: CollaborationParticipant::new(
                            child_id.to_string(),
                            parent_id.to_string(),
                            root_locator.project_id(),
                            child_document.segment_generation(),
                            None,
                            Some(profile.task_id.clone()),
                        )
                        .unwrap(),
                        reserved_at_ms: 4,
                    },
                    intent,
                )
            },
        )
        .unwrap()
        .value
        .into_value()
        .task;
    (temporary, parent, child_document, record, profile)
}

/// Publishes the fixture's initial child through the production first-file writer.
/// 通过生产首次文件写入器发布夹具的初始子任务。
/// Returns the fixture storage, parent and child handles, reservation and captured profile.
/// 返回夹具存储、父子句柄、预留及捕获配置。
fn ready_reserved_child() -> (
    tempfile::TempDir,
    Arc<SessionHandle>,
    Arc<SessionHandle>,
    CollaborationChildTaskRecord,
    ChildExecutionProfile,
) {
    let (temporary, parent, mut child_document, record, profile) = reserved_creation_source();
    let root_locator = parent.storage_locator().unwrap();
    let child_id = Uuid::parse_str(&record.child.session_id).unwrap();
    // Publish the complete child profile using the same first-file writer as production.
    // 使用与生产相同的首次文件写入器发布完整子配置。
    let candidate = child_document.prepare_flush_candidates().unwrap().unwrap();
    let created = create_embedded_task_session_v1(
        &root_locator,
        &CreateEmbeddedTaskSessionV1Request {
            metadata: &candidate.metadata,
            runtime_lock: &candidate.runtime_lock,
            compactions: &candidate.compactions,
            events: &candidate.tail_events,
        },
    )
    .unwrap();
    child_document
        .apply_published_storage_metadata(&created.metadata)
        .unwrap();
    child_document.advance_durable(candidate.revision).unwrap();
    let child = SessionHandle::new(child_id, created.locator, child_document);
    (temporary, parent, child, record, profile)
}

/// Cold admission activates the original reservation and preserves every child business event.
/// 冷准入激活原预留，同时保留每个子业务事件。
/// Returns nothing; actual disk reload proves activation identity, single revision and replay behavior.
/// 不返回值；实际磁盘重载证明激活身份、单修订及重放行为。
#[test]
fn prepared_reserved_child_admission_publishes_original_activation() {
    let (_temporary, parent, child, record, profile) = ready_reserved_child();
    let child_before = load_session_v1_for_runtime(&child.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        source_for_dispatch(&parent, &child, &record)
            .unwrap()
            .is_some()
    );
    let revision = parent.read(|document| document.durable_revision()).unwrap();
    assert!(
        admit_prepared_source(&parent, &child, &record, &profile, 5)
            .unwrap()
            .is_some()
    );
    let parent_after = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
        .unwrap()
        .document;
    let active = &parent_after.collaboration_state().child_tasks[&record.execution_id];
    assert_eq!(active.state, CollaborationChildTaskState::Active);
    assert_eq!(active.parent_turn_id, record.parent_turn_id);
    assert_eq!(
        active.reservation_operation_id,
        record.reservation_operation_id
    );
    assert_eq!(parent_after.durable_revision(), revision + 1);
    assert!(parent_after.collaboration_state().operations.contains_key(
        &CollaborationOperationId::try_new("original-child-reservation:activate").unwrap(),
    ));
    // An already active source proceeds without a second activation revision.
    // 已经活动的来源继续准入，不产生第二个激活修订。
    assert!(
        admit_prepared_source(&parent, &child, active, &profile, 6)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision + 1
    );
    let child_after = load_session_v1_for_runtime(&child.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        serde_json::to_value(&child_after.metadata).unwrap(),
        serde_json::to_value(&child_before.metadata).unwrap()
    );
    assert_eq!(
        serde_json::to_value(child_after.events()).unwrap(),
        serde_json::to_value(child_before.events()).unwrap()
    );
}

/// Changed preparation and user stop cannot publish an activation from stale recovery work.
/// 改变的准备来源及用户停止不能让过期恢复工作发布激活。
/// Returns nothing; checks both rejected and skipped admission against the real durable parent.
/// 不返回值；对真实可靠父文档检查拒绝及跳过的准入。
#[test]
fn prepared_reserved_child_admission_rejects_changed_profile_and_stop() {
    let (_temporary, parent, child, record, profile) = ready_reserved_child();
    let revision = parent.read(|document| document.durable_revision()).unwrap();
    let mut stale = profile.clone();
    stale.connection_id = "changed-after-preparation".into();
    assert!(admit_prepared_source(&parent, &child, &record, &stale, 5).is_err());
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision
    );
    parent
        .set_collaboration_run_control_durable(
            vulcan_session_core::v1::collaboration::SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("stop-before-recovery-admission")
                    .unwrap(),
                state: CollaborationRunControl::MainAndSubagents,
                changed_at_ms: 6,
            },
        )
        .unwrap();
    let stopped_revision = parent.read(|document| document.durable_revision()).unwrap();
    assert!(
        admit_prepared_source(&parent, &child, &record, &profile, 7)
            .unwrap()
            .is_none()
    );
    let persisted = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(persisted.durable_revision(), stopped_revision);
    assert_eq!(
        persisted.collaboration_state().child_tasks[&record.execution_id].state,
        CollaborationChildTaskState::Reserved
    );
}
