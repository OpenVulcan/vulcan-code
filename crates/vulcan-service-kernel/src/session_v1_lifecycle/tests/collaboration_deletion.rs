//! Real root deletion must retain unresolved collaboration in embedded descendants.
//! 真实根删除必须保留嵌入后代中的未结协作。

use super::*;
use vulcan_session_core::v1::collaboration::*;
use vulcan_session_core::v1::ids::SubagentTaskId;

/// Creates and completes a real embedded child of `root_id` without adding a parent task record.
/// 为 `root_id` 创建并完成真实嵌入子会话，不添加父任务记录。
/// Returns its canonical handle so callers can inspect the physical owner and stale writer fencing.
/// 返回规范句柄，供调用方检查物理所有者与旧写入者隔离。
fn completed_unreferenced_child(
    registry: &SessionRuntimeRegistry,
    root_id: SessionId,
    child_id: SessionId,
) -> Arc<SessionHandle> {
    // An absent parent task record models physical children retained after a business generation reset.
    // 缺失父任务记录模拟业务代次重置后保留的物理子会话。
    let child = create_or_load_embedded_task_session_v1(
        registry,
        TaskV1ExecutionFacts {
            root_session_id: root_id,
            parent_session_id: root_id,
            task_id: child_id,
            execution_id: "deletion-unreferenced-child".into(),
            prompt: "Review the project".into(),
            display_title: "Unreferenced child".into(),
            is_resumed: false,
            admitted_at_ms: 1_787_788_800_020,
        },
    )
    .unwrap();
    close_fixture_turn(&child, 1_787_788_800_024);
    child
}

/// Deletes a settled physical child absent from parent records and rejects writes through its old handle.
/// 删除父记录未引用的已结物理子会话，并拒绝通过旧句柄写入。
/// Verifies real directory removal, SQLite removal and both warm/cold registry admission fencing.
/// 验证真实目录移除、SQLite 移除以及驻留和冷态注册表准入隔离。
#[tokio::test]
async fn root_deletion_seals_unreferenced_child_and_removes_exact_tree() {
    // Each fixture owns an isolated SQLite host and storage root.
    // 每个夹具持有隔离的 SQLite 宿主和存储根。
    let fixture = LifecycleStorageFixture::start().await;
    // Keep the same canonical registry to test retained stale handles.
    // 保留同一规范注册表，以测试留存的旧句柄。
    let registry = SessionRuntimeRegistry::default();
    // Immutable indexed owner identity.
    // 不可变索引所有者身份。
    let root_id = SessionId::new_v4();
    // Immutable embedded owner identity.
    // 不可变嵌入所有者身份。
    let child_id = SessionId::new_v4();
    // Publish real owners through production constructors.
    // 通过生产构造器发布真实所有者。
    let root = create_fixture_session(&fixture, &registry, root_id).await;
    // This physical child is deliberately absent from the root collaboration task records.
    // 此物理子会话特意不在根协作任务记录中。
    let child = completed_unreferenced_child(&registry, root_id, child_id);
    close_fixture_turn(&root, 1_787_788_800_026);
    // Keep exact locators without reconstructing storage paths.
    // 保留精确定位器，不重建存储路径。
    let root_locator = root.storage_locator().unwrap();
    // The embedded path must also remain absent after rejecting stale writes.
    // 拒绝旧写入后，嵌入路径也必须保持不存在。
    let child_locator = child.storage_locator().unwrap();
    delete_session_lifecycle_v1(&fixture.controller, &registry, root_id)
        .await
        .unwrap();
    assert!(!root_locator.session_directory().exists());
    assert!(!child_locator.session_directory().exists());
    assert!(
        list_active_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .unwrap()
            .is_empty()
    );
    for handle in [&root, &child] {
        assert!(matches!(
            handle.mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |_| Ok(())
            ),
            Err(SessionRuntimeError::SessionSealed(id)) if id == handle.session_id()
        ));
        assert!(matches!(registry.get(handle.session_id()),
            Err(SessionRuntimeError::SessionSealed(id)) if id == handle.session_id()));
        assert!(matches!(registry.get_or_load(handle.session_id(), || {
            panic!("Deleted owners must not enter the cold loader")
        }), Err(SessionRuntimeError::SessionSealed(id)) if id == handle.session_id()));
    }
    assert!(!root_locator.session_directory().exists());
    fixture.shutdown().await;
}

/// Refuses recursive root deletion when an existing embedded owner's authoritative metadata is corrupt.
/// 已有嵌入所有者的权威元数据损坏时，拒绝递归删除根。
/// Checks that errors preserve the root index, both directories and the ability to mutate the root.
/// 检查错误保留根索引、双方目录以及根修改能力。
#[tokio::test]
async fn root_deletion_rejects_corrupt_unreferenced_child_metadata() {
    // Actual storage remains isolated within the fixture lifetime.
    // 实际存储在夹具生命周期内保持隔离。
    let fixture = LifecycleStorageFixture::start().await;
    // Canonical owner registry used by the production deletion path.
    // 生产删除路径使用的规范所有者注册表。
    let registry = SessionRuntimeRegistry::default();
    // Independent immutable root identity.
    // 独立不可变根身份。
    let root_id = SessionId::new_v4();
    // Canonical indexed owner.
    // 规范索引所有者。
    let root = create_fixture_session(&fixture, &registry, root_id).await;
    // The corrupted child is physically owned even though current parent records omit it.
    // 即使当前父记录省略，损坏子会话仍属于物理归属范围。
    let child = completed_unreferenced_child(&registry, root_id, SessionId::new_v4());
    close_fixture_turn(&root, 1_787_788_800_026);
    // Corrupt the current metadata fixture, matching the existing lifecycle corruption test.
    // 损坏当前元数据夹具，与既有生命周期损坏测试使用相同布局。
    let child_locator = child.storage_locator().unwrap();
    std::fs::write(
        child_locator.session_directory().join("metadata.json"),
        b"invalid json",
    )
    .unwrap();
    assert!(matches!(
        delete_session_lifecycle_v1(&fixture.controller, &registry, root_id).await,
        Err(SessionV1LifecycleError::Storage(_))
    ));
    assert!(child_locator.session_directory().exists());
    assert_eq!(
        list_active_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .unwrap()
            .len(),
        1
    );
    root.mutate_durable(
        SessionMutationAdmission::ControlPlane,
        SessionFlushReason::MetadataChanged,
        |document| document.set_archived_at(Some(1_787_788_800_030), 1_787_788_800_030),
    )
    .unwrap();
    fixture.shutdown().await;
}

/// Closes both businesses but leaves a child's question pending, then attempts cold root deletion.
/// 关闭双方业务但保留子问题待处理，然后尝试冷态根删除。
/// Uses actual SQLite, embedded files and domain transitions; no runtime task registry is seeded.
/// 使用实际 SQLite、嵌入文件及领域转换，不播种运行时任务注册表。
#[tokio::test]
async fn cold_root_deletion_preserves_completed_child_with_pending_question() {
    // The fixture owns every path and database touched by the destructive operation.
    // 夹具持有破坏性操作涉及的全部路径与数据库。
    let fixture = LifecycleStorageFixture::start().await;
    // Publish ordinary indexed and embedded owners using the production lifecycle constructors.
    // 使用生产生命周期构造器发布普通索引及嵌入所有者。
    let registry = SessionRuntimeRegistry::default();
    // Root and child identities are independent and immutable.
    // 根与子身份独立且不可变。
    let root_id = SessionId::new_v4();
    // This child has no independent SQLite row.
    // 此子会话没有独立 SQLite 行。
    let child_id = SessionId::new_v4();
    // Canonical root owns the exact embedded locator.
    // 规范根持有精确嵌入定位器。
    let root = create_fixture_session(&fixture, &registry, root_id).await;
    // Use an actual child creation path instead of copying metadata into a guessed directory.
    // 使用实际子创建路径，不向猜测目录复制元数据。
    let child = create_or_load_embedded_task_session_v1(
        &registry,
        TaskV1ExecutionFacts {
            root_session_id: root_id,
            parent_session_id: root_id,
            task_id: child_id,
            execution_id: "deletion-child-business".into(),
            prompt: "Review the project".into(),
            display_title: "Deletion child".into(),
            is_resumed: false,
            admitted_at_ms: 1_787_788_800_020,
        },
    )
    .unwrap();
    // Capture real allocated turn and generation identities before recording communication.
    // 记录通信前捕获实际分配的回合及代次身份。
    let root_participant = root
        .read(|document| {
            CollaborationParticipant::new(
                root_id.to_string(),
                root_id.to_string(),
                "project-explicit",
                document.segment_generation(),
                Some(document.active_turn().unwrap().turn_id),
                None,
            )
            .unwrap()
        })
        .unwrap();
    // The source question belongs to the child's actual open business turn.
    // 来源问题属于子会话实际开放业务回合。
    let child_participant = child
        .read(|document| {
            CollaborationParticipant::new(
                child_id.to_string(),
                root_id.to_string(),
                "project-explicit",
                document.segment_generation(),
                Some(document.active_turn().unwrap().turn_id),
                Some(SubagentTaskId::try_new(child_id.to_string()).unwrap()),
            )
            .unwrap()
        })
        .unwrap();
    root.mutate_durable(
        SessionMutationAdmission::ControlPlane,
        SessionFlushReason::MetadataChanged,
        |document| {
            document.reserve_child_task(ReserveChildTaskInput {
                source_tool_name: "spawn_agent".into(),
                execution_id: CollaborationExecutionId::try_new("deletion-child-business").unwrap(),
                operation_id: CollaborationOperationId::try_new("deletion-reserve").unwrap(),
                task_id: SubagentTaskId::try_new(child_id.to_string()).unwrap(),
                parent_turn_id: root_participant.turn_id.unwrap(),
                child: child_participant.clone(),
                reserved_at_ms: 1_787_788_800_021,
            })
        },
    )
    .unwrap();
    root.mutate_durable(
        SessionMutationAdmission::ControlPlane,
        SessionFlushReason::MetadataChanged,
        |document| {
            document.activate_child_task(ActivateChildTaskInput {
                execution_id: CollaborationExecutionId::try_new("deletion-child-business").unwrap(),
                operation_id: CollaborationOperationId::try_new("deletion-activate").unwrap(),
                task_id: SubagentTaskId::try_new(child_id.to_string()).unwrap(),
                active_at_ms: 1_787_788_800_022,
            })
        },
    )
    .unwrap();
    child
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.create_question(CreateQuestionInput {
                    operation_id: CollaborationOperationId::try_new("deletion-question-create")
                        .unwrap(),
                    question_id: CollaborationQuestionId::try_new("deletion-question").unwrap(),
                    delivery_id: CollaborationDeliveryId::try_new("deletion-question-delivery")
                        .unwrap(),
                    source: child_participant.clone(),
                    target: root_participant.clone(),
                    body: "Please confirm the finding when available.".into(),
                    execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                    created_at_ms: 1_787_788_800_023,
                })
            },
        )
        .unwrap();
    close_fixture_turn(&child, 1_787_788_800_024);
    root.mutate_durable(
        SessionMutationAdmission::ControlPlane,
        SessionFlushReason::MetadataChanged,
        |document| {
            document.terminal_child_task(TerminalChildTaskInput {
                execution_id: CollaborationExecutionId::try_new("deletion-child-business").unwrap(),
                operation_id: CollaborationOperationId::try_new("deletion-terminal").unwrap(),
                task_id: SubagentTaskId::try_new(child_id.to_string()).unwrap(),
                outcome: CollaborationChildTaskTerminalOutcome::Completed,
                terminal_at_ms: 1_787_788_800_025,
            })
        },
    )
    .unwrap();
    close_fixture_turn(&root, 1_787_788_800_026);
    assert!(
        !root
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    assert!(
        child
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    // Preserve exact paths to verify rejection does not remove either owner.
    // 保留精确路径，验证拒绝不会移除任一所有者。
    let child_locator = child.storage_locator().unwrap();
    archive_session_lifecycle_v1(&fixture.controller, &registry, root_id, 1_787_788_800_027)
        .await
        .unwrap();
    // A fresh registry proves protection does not rely on warm task or document caches.
    // 全新注册表证明保护不依赖驻留任务或文档缓存。
    let cold_registry = SessionRuntimeRegistry::default();
    assert!(matches!(
        delete_session_lifecycle_v1(&fixture.controller, &cold_registry, root_id).await,
        Err(SessionV1LifecycleError::UnsettledCollaboration(id)) if id == child_id
    ));
    assert!(
        load_session_v1_full(&child_locator)
            .unwrap()
            .document
            .has_unsettled_collaboration_for_edit_or_delete()
    );
    assert_eq!(
        list_archived_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .unwrap()
            .len(),
        1
    );
    fixture.shutdown().await;
}
