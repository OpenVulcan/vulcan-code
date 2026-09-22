//! Durable failure settlement before initial child storage exists.
//! 初始子存储存在之前的可靠失败结算。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskExecutionIdentity;
use vulcan_session_core::v1::{RequestStreamId, ToolCallId};

mod resumption;

/// Success publishes the stored source before handing the canonical child to the real admission boundary.
/// 成功路径先发布持久来源，再将规范子任务交到真实准入边界。
/// Returns nothing; repeated admission preserves one root history and one registry owner.
/// 不返回值；重复准入保留单份根历史及唯一注册表所有者。
#[test]
fn initial_child_publication_success_reuses_exact_canonical_owner() {
    let (_temporary, parent, initial, record, profile) =
        startup_recovery::native_execution::tests::reserved_creation_source();
    let registry = SessionRuntimeRegistry::default();
    let revision = parent.read(|document| document.durable_revision()).unwrap();
    let child_id = Uuid::parse_str(&record.child.session_id).unwrap();
    let mut first = None;
    for _ in 0..2 {
        publication::publish(&registry, &parent, &parent, &record, |child| {
            // Listener admission observes both files and the already registered exact handle.
            // 监听准入同时观察两份文件及已注册的精确句柄。
            let parent_disk = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
                .unwrap()
                .document;
            assert_eq!(
                parent_disk.collaboration_state().child_tasks[&record.execution_id],
                record
            );
            assert_eq!(
                parent_disk.metadata.child_creation_intents[&record.execution_id].profile,
                profile
            );
            assert!(Arc::ptr_eq(&registry.get(child_id).unwrap(), &child));
            let child_disk = load_session_v1_for_runtime(&child.storage_locator().unwrap())
                .unwrap()
                .document;
            assert_eq!(
                child_disk.metadata.child_execution_profile.as_ref(),
                Some(&profile)
            );
            assert_eq!(
                serde_json::to_value(child_disk.events()).unwrap(),
                serde_json::to_value(initial.events()).unwrap()
            );
            if let Some(first) = &first {
                assert!(Arc::ptr_eq(first, &child));
            }
            first = Some(child);
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision
    );
}

/// Rejected listener admission closes the original child even if the listener already closed it.
/// 即使监听器已经闭合原子任务，拒绝监听准入仍能正确完成该任务闭合。
/// Returns nothing; both rejection orders preserve the original business identity on disk.
/// 不返回值；两种拒绝顺序都保留磁盘中的原业务身份。
#[test]
fn initial_child_publication_listener_rejection_closes_original_turn() {
    for listener_closes_first in [false, true] {
        let (_temporary, parent, _initial, record, profile) =
            startup_recovery::native_execution::tests::reserved_creation_source();
        let registry = SessionRuntimeRegistry::default();
        let failure = publication::publish(&registry, &parent, &parent, &record, |child| {
            if listener_closes_first {
                let owner = crate::agent_stream::startup_rejection::capture_owner(
                    &child,
                    record.execution_id.as_str(),
                )
                .unwrap();
                crate::agent_stream::startup_rejection::close_rejected_child(
                    &child,
                    owner,
                    "listener rejected",
                )
                .unwrap();
            }
            Err(String::from("listener rejected"))
        })
        .unwrap_err();
        assert_eq!(failure, "listener rejected");
        let child = registry
            .get(Uuid::parse_str(&record.child.session_id).unwrap())
            .unwrap();
        let disk = load_session_v1_for_runtime(&child.storage_locator().unwrap())
            .unwrap()
            .document;
        assert_eq!(
            disk.metadata.child_execution_profile.as_ref(),
            Some(&profile)
        );
        assert!(disk.active_turn().is_none_or(|turn| turn.closed));
        assert!(disk.active_api().is_none());
        // Parent failure settlement remains owned by the outer creation transaction.
        // 父失败结算仍由外层创建事务拥有。
        assert_eq!(
            parent
                .read(
                    |document| document.collaboration_state().child_tasks[&record.execution_id]
                        .state
                )
                .unwrap(),
            vulcan_session_core::v1::CollaborationChildTaskState::Reserved
        );
    }
}

/// Closed registry admission prevents first-file publication, and corrupt existing directories are preserved.
/// 关闭的注册表准入阻止首次文件发布，并保留损坏的已有目录。
/// Returns nothing; failure never enters listener handoff or changes the durable parent record.
/// 不返回值；失败绝不进入监听交接，也不改变可靠父记录。
#[test]
fn initial_child_publication_storage_and_shutdown_fail_before_handoff() {
    for closed_registry in [false, true] {
        let (_temporary, parent, _initial, record, _profile) =
            startup_recovery::native_execution::tests::reserved_creation_source();
        let registry = SessionRuntimeRegistry::default();
        let locator = locate_embedded_task_session_v1(
            &parent.storage_locator().unwrap(),
            &record.child.session_id,
        )
        .unwrap();
        if closed_registry {
            registry.begin_shutdown().unwrap();
        } else {
            // An existing incomplete directory is damage, not an invitation to overwrite it.
            // 已有的不完整目录属于损坏，不是允许覆盖的信号。
            std::fs::create_dir_all(locator.session_directory()).unwrap();
        }
        assert!(
            publication::publish(&registry, &parent, &parent, &record, |_| {
                panic!("rejected publication must not admit a listener")
            })
            .is_err()
        );
        assert!(
            registry
                .get(Uuid::parse_str(&record.child.session_id).unwrap())
                .is_err()
        );
        if closed_registry {
            assert!(!locator.session_directory().try_exists().unwrap());
        } else {
            assert_eq!(
                std::fs::read_dir(locator.session_directory())
                    .unwrap()
                    .count(),
                0
            );
        }
        let parent_disk = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
            .unwrap()
            .document;
        assert_eq!(
            parent_disk.collaboration_state().child_tasks[&record.execution_id],
            record
        );
    }
}

/// Changed discovery cannot publish a child even when the original creation intent remains valid.
/// 即使原创建意图仍合法，变化的发现记录也不能发布子任务。
/// Returns nothing; the failed proof leaves storage and the registry empty for that child.
/// 不返回值；证明失败使该子任务的存储与注册表均保持空缺。
#[test]
fn initial_child_publication_rejects_changed_reservation() {
    let (_temporary, parent, _initial, mut record, _profile) =
        startup_recovery::native_execution::tests::reserved_creation_source();
    let registry = SessionRuntimeRegistry::default();
    let locator = locate_embedded_task_session_v1(
        &parent.storage_locator().unwrap(),
        &record.child.session_id,
    )
    .unwrap();
    record.source_tool_name = "changed-after-reservation".into();
    assert!(
        publication::publish(&registry, &parent, &parent, &record, |_| panic!(
            "stale owner"
        ))
        .is_err()
    );
    assert!(!locator.session_directory().try_exists().unwrap());
}

/// An admitted initial creation can fail and release its parent obligation without inventing child files.
/// 已准入初始创建可以在不伪造子文件的情况下失败并释放父义务。
/// Returns nothing; disk reload proves exact identity, one failure revision and safe repeated cleanup.
/// 不返回值；磁盘重载证明精确身份、单次失败修订及安全重复清理。
#[test]
fn initial_child_creation_failure_settles_without_child_file() {
    let (_temporary, parent, _child, record, _profile) =
        startup_recovery::native_execution::tests::reserved_creation_source();
    let locator = locate_embedded_task_session_v1(
        &parent.storage_locator().unwrap(),
        &record.child.session_id,
    )
    .unwrap();
    let generation = parent
        .read(|document| document.segment_generation())
        .unwrap();
    let revision = parent.read(|document| document.durable_revision()).unwrap();
    let reservation = ChildTaskReservationRequest {
        invocation: VulcanToolInvocationContext {
            session_id: parent.session_id().to_string(),
            request_stream_id: RequestStreamId::try_new("creation-failure-stream").unwrap(),
            api_call_id: ApiCallId::try_new(1).unwrap(),
            tool_call_id: ToolCallId::try_new(1).unwrap(),
            tool_seq: Seq::try_new(3).unwrap(),
            controller_tool_call_id: "creation-failure-call".into(),
            provider_tool_call_id: None,
        },
        operation_id: record.reservation_operation_id.clone(),
        execution: ChildTaskExecutionIdentity {
            task_session_id: Uuid::parse_str(&record.child.session_id).unwrap(),
            execution_id: record.execution_id.as_str().into(),
            task_id: record.task_id.clone(),
        },
        reserved_at_ms: record.reserved_at_ms,
    };
    // A conflict must not gain cleanup authority over this admitted reservation.
    // 冲突调用不得取得已准入预留的清理权威。
    let mut conflict = reservation.clone();
    conflict.operation_id = CollaborationOperationId::try_new("foreign-reservation").unwrap();
    assert!(fail_reserved_creation(&parent, generation, &conflict).is_err());
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision
    );
    fail_reserved_creation(&parent, generation, &reservation).unwrap();
    let persisted = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
        .unwrap()
        .document;
    let failed = &persisted.collaboration_state().child_tasks[&record.execution_id];
    assert_eq!(
        failed.state,
        vulcan_session_core::v1::CollaborationChildTaskState::Terminal
    );
    assert_eq!(
        failed.terminal_outcome,
        Some(CollaborationChildTaskTerminalOutcome::Failed)
    );
    assert_eq!(failed.parent_turn_id, record.parent_turn_id);
    assert_eq!(failed.active_at_ms, None);
    assert_eq!(persisted.durable_revision(), revision + 1);
    assert!(!locator.session_directory().try_exists().unwrap());
    fail_reserved_creation(&parent, generation, &reservation).unwrap();
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision + 1
    );
    assert!(fail_reserved_creation(&parent, generation + 1, &reservation).is_err());
}
