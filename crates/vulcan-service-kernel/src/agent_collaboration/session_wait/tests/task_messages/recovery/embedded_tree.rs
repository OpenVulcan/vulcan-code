//! Cold recovery of actual embedded descendants from durable parent reservations.
//! 根据可靠父任务预留冷恢复实际嵌入后代。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts;
use vulcan_session_core::v1::collaboration::{CollaborationExecutionId, ReserveChildTaskInput};

/// Repairs descendant receipts without requiring a Hook for the historical main task.
/// 修复后代回执，不要求历史主任务具有 Hook。
/// `invocation` supplies the original root; any repair error fails instead of being ignored.
/// `invocation` 提供原根；任何修复错误都导致失败，不能忽略。
async fn recover_tree_with_missing_main_hook(invocation: &ValidatedInvocation) {
    // Child result repair does not need or synthesize a main-business Hook or dispatch owner.
    // 子结果修复不需要、也不合成主业务 Hook 或派发所有者。
    startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [invocation.source_session_id].into_iter().collect(),
    )
    .await
    .unwrap();
    assert!(
        !invocation
            .state
            .lock()
            .unwrap()
            .active_stream_tasks
            .contains_key(&invocation.source_session_id),
        "repairing child receipts must not start the historical parent task"
    );
}

/// Creates one real embedded child and durably reserves it in the exact supplied parent.
/// 创建一个真实嵌入子会话，并在精确指定父会话中可靠预留。
/// `invocation` owns the indexed root; `parent` owns this edge; `label` supplies execution identity.
/// `invocation` 拥有索引根；`parent` 拥有此边；`label` 提供执行身份。
/// Returns the canonical child without starting any provider or background execution.
/// 返回规范子句柄，不启动任何供应商或后台执行。
async fn reserved_child(
    invocation: &ValidatedInvocation,
    parent: &Arc<SessionHandle>,
    label: &str,
) -> Arc<SessionHandle> {
    // Allocate a physical identity independently of its immutable execution key.
    // 独立于不可变执行键分配物理身份。
    let child_id = Uuid::new_v4();
    // The real creator derives every depth from the indexed root's locator.
    // 真实创建器从索引根定位器派生每个深度。
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: invocation.source_session_id,
            parent_session_id: parent.session_id(),
            task_id: child_id,
            execution_id: label.to_owned(),
            prompt: String::from("Embedded recovery task"),
            display_title: label.to_owned(),
            is_resumed: false,
            admitted_at_ms: 5,
        },
    )
    .unwrap();
    // Use the production scope resolver so project and root ownership match normal admission.
    // 使用生产范围解析器，使项目和根归属匹配正常准入。
    let identity = participants::participant_for_handle(invocation, child_id, &child, None)
        .await
        .unwrap();
    // The parent turn comes from its own business input, including when the parent is embedded.
    // 父回合来自它自己的业务输入，包括父节点也是嵌入会话的情况。
    let turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    source_tool_name: String::from("spawn_agent"),
                    operation_id: CollaborationOperationId::try_new(format!("reserve-{label}"))
                        .unwrap(),
                    execution_id: CollaborationExecutionId::try_new(label).unwrap(),
                    task_id: identity.task_id.clone().unwrap(),
                    parent_turn_id: turn,
                    child: identity,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    child
}

/// A cold root scan repairs a grandchild outbox using durable two-level ownership and real files.
/// 冷根扫描使用可靠两层归属及真实文件修复孙代理出站箱。
/// Assertions prove cold handle replacement, exact original delivery and replay idempotence.
/// 断言证明冷句柄替换、精确原始投递与重放幂等。
#[tokio::test]
async fn session_collaboration_startup_repairs_embedded_grandchild_outbox() {
    // Retain actual indexed storage while discarding every original registry owner below.
    // 保留实际索引存储，随后丢弃每个原注册表所有者。
    let (_root, invocation, target, _) = message_fixture(false).await;
    // Neither child has an independent SQLite session row.
    // 两个子节点均没有独立 SQLite 会话行。
    let child = reserved_child(&invocation, &invocation.source_handle, "cold-child").await;
    let grandchild = reserved_child(&invocation, &child, "cold-grandchild").await;
    // Recovery authority uses the exact grandchild handle and its original business turn.
    // 恢复权威使用精确孙句柄及其原业务回合。
    let mut grandchild_invocation = recovery_invocation(
        invocation.state.clone(),
        invocation.controller_runtime.clone(),
        invocation.registry.clone(),
        grandchild.session_id(),
        grandchild.clone(),
    )
    .unwrap();
    grandchild_invocation.source_turn_id = grandchild
        .read(|document| document.active_turn().unwrap().turn_id.value())
        .unwrap();
    // A pending steer has no scheduler side effect, so no real model runtime participates.
    // 待处理插队没有调度副作用，因此不涉及真实模型运行时。
    let message = record_pending_task(
        &grandchild_invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "grandchild",
    )
    .await;
    // Replacing the registry forces discovery and full validation from disk.
    // 替换注册表强制从磁盘进行发现与完整校验。
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    recover_tree_with_missing_main_hook(&invocation).await;
    // Both direct and nested owners must be different canonical objects after the cold scan.
    // 冷扫描后直接与嵌套所有者都必须是不同的规范对象。
    let recovered_child = registry.get(child.session_id()).unwrap();
    let recovered_grandchild = registry.get(grandchild.session_id()).unwrap();
    assert!(!Arc::ptr_eq(&child, &recovered_child));
    assert!(!Arc::ptr_eq(&grandchild, &recovered_grandchild));
    // Verify both durable sides, rather than trusting a live tool result or event notification.
    // 验证双方可靠记录，不依赖活动工具结果或事件通知。
    let source = load_session_v1_for_runtime(&grandchild.storage_locator().unwrap())
        .unwrap()
        .document;
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let acceptance = &receiver.collaboration_state().inbound_task_messages[&message.delivery_id];
    assert_eq!(acceptance.message, message);
    assert_eq!(
        source.collaboration_state().outbound_task_messages[&message.delivery_id]
            .acceptance
            .as_ref(),
        Some(acceptance)
    );
    // A second startup scan sees the acknowledgement and must not rewrite either journal.
    // 第二次启动扫描看到确认后，不得重写任一日志。
    let revisions = (source.durable_revision(), receiver.durable_revision());
    recover_tree_with_missing_main_hook(&invocation).await;
    assert_eq!(
        registry
            .get(grandchild.session_id())
            .unwrap()
            .read(|document| document.durable_revision())
            .unwrap(),
        revisions.0
    );
    assert_eq!(
        registry
            .get(target.session_id())
            .unwrap()
            .read(|document| document.durable_revision())
            .unwrap(),
        revisions.1
    );
}

/// Pending native reservations are inspected without inventing a result for an unfinished child.
/// 检查待处理原生预留，不为尚未完成的子任务制造结果。
#[tokio::test]
async fn session_collaboration_startup_inspects_unfinished_embedded_execution() {
    // A real reserved child exists on disk, but has no message outbox recovery work.
    // 磁盘上存在真实预留子会话，但没有消息出站恢复待办。
    let (_root, invocation, _target, _) = message_fixture(false).await;
    let child = reserved_child(&invocation, &invocation.source_handle, "cold-empty").await;
    // No canonical handle may be borrowed from the original registry.
    // 不能从原注册表借用任何规范句柄。
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    recover_tree_with_missing_main_hook(&invocation).await;
    assert!(registry.get(child.session_id()).is_ok());
    registry
        .get(invocation.source_session_id)
        .unwrap()
        .read(|document| {
            assert_eq!(
                document
                    .collaboration_state()
                    .child_tasks
                    .values()
                    .next()
                    .unwrap()
                    .state,
                vulcan_session_core::v1::collaboration::CollaborationChildTaskState::Reserved
            );
            assert!(
                !document
                    .events()
                    .iter()
                    .any(|event| matches!(event.event, EventBody::SubagentReturn(_)))
            );
        })
        .unwrap();
}

/// Explicit stop survives a lost terminal callback without taking work from a live listener.
/// 明确停止在终态回调丢失后仍可恢复，且不抢占活动监听器的工作。
/// Real cold storage must close the original child and release its parent's exact completion duty once.
/// 真实冷存储必须闭合原子回合，并仅一次解除父会话的精确完成义务。
#[tokio::test]
async fn session_collaboration_startup_settles_persisted_child_stop() {
    use vulcan_session_core::v1::collaboration::{
        CollaborationChildTaskState, CollaborationChildTaskTerminalOutcome, SetRunControlInput,
    };
    for (control, stop_parent) in [
        (CollaborationRunControl::Stopped, false),
        (CollaborationRunControl::MainAndSubagents, false),
        (CollaborationRunControl::MainAndSubagents, true),
    ] {
        // Retain independent stores for each explicit stopping policy.
        // 为每种明确停止策略保留独立存储。
        let (_root, invocation, _target, _) = message_fixture(false).await;
        // This execution has a real root and parent reservation, but no terminal boundary.
        // 此执行具有真实根输入和父预留，但尚无终态边界。
        let child = reserved_child(
            &invocation,
            &invocation.source_handle,
            "stop-before-terminal",
        )
        .await;
        // The durable control write represents the last successful action before process exit.
        // 可靠控制写入代表进程退出前最后一次成功动作。
        // Ancestor-only control models exit before stop propagation reached the child.
        // 仅祖先控制模拟停止尚未传播到子会话就已退出。
        let stop_owner = if stop_parent {
            &invocation.source_handle
        } else {
            &child
        };
        stop_owner
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("durable-stop-intent").unwrap(),
                state: control,
                changed_at_ms: 6,
            })
            .unwrap();
        // An existing listener must retain authority to report completed, failed or stopped.
        // 已有监听器必须保留报告完成、失败或停止的权威。
        invocation
            .state
            .lock()
            .unwrap()
            .detached_subagent_task_listeners
            .insert(child.session_id(), String::from("stop-before-terminal"));
        // The live parent API already owns its turn, so no cold main admission is attempted.
        // 活动父 API 已拥有其回合，因此不尝试主执行冷准入。
        startup_recovery::recover_indexed_collaboration(
            &crate::ServiceEventEmitter::new(vec![]),
            &invocation.state,
            [invocation.source_session_id].into_iter().collect(),
        )
        .await
        .unwrap();
        assert!(
            child
                .read(|document| !document.active_turn().unwrap().closed)
                .unwrap()
        );
        // Simulate process exit by discarding listener ownership and every canonical handle.
        // 通过丢弃监听归属和全部规范句柄模拟进程退出。
        let registry = Arc::new(SessionRuntimeRegistry::default());
        {
            // Replace both volatile owners under one service lock.
            // 在同一服务锁内替换两个易失所有者。
            let mut guard = invocation.state.lock().unwrap();
            guard.detached_subagent_task_listeners.clear();
            guard.session_runtime_registry = registry.clone();
        }
        if stop_parent {
            startup_recovery::recover_indexed_collaboration(
                &crate::ServiceEventEmitter::new(vec![]),
                &invocation.state,
                [invocation.source_session_id].into_iter().collect(),
            )
            .await
            .unwrap();
        } else {
            recover_tree_with_missing_main_hook(&invocation).await;
        }
        // Read both sides from disk to exclude an in-memory-only repair.
        // 从磁盘读取双方，排除仅内存修复。
        let source = load_session_v1_for_runtime(&child.storage_locator().unwrap())
            .unwrap()
            .document;
        // The parent result must preserve its original execution identity and stopped classification.
        // 父结果必须保留原执行身份和停止分类。
        let parent =
            load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
                .unwrap()
                .document;
        // Select by stable identity rather than map order.
        // 根据稳定身份选择，而非映射顺序。
        let execution = CollaborationExecutionId::try_new("stop-before-terminal").unwrap();
        // Source closure is required as well as the parent-owned receipt.
        // 除父侧回执外还必须闭合来源。
        let record = &parent.collaboration_state().child_tasks[&execution];
        assert_eq!(record.state, CollaborationChildTaskState::Terminal);
        assert_eq!(
            record.terminal_outcome,
            Some(CollaborationChildTaskTerminalOutcome::Stopped)
        );
        assert!(source.active_turn().is_none_or(|turn| turn.closed));
        assert!(
            parent
                .completion_obligations(record.parent_turn_id)
                .open_child_task_ids
                .is_empty()
        );
        // Repeated scans must not duplicate source boundaries or parent terminal receipts.
        // 重复扫描不得复制来源边界或父终态回执。
        let revisions = (source.durable_revision(), parent.durable_revision());
        if stop_parent {
            startup_recovery::recover_indexed_collaboration(
                &crate::ServiceEventEmitter::new(vec![]),
                &invocation.state,
                [invocation.source_session_id].into_iter().collect(),
            )
            .await
            .unwrap();
        } else {
            recover_tree_with_missing_main_hook(&invocation).await;
        }
        assert_eq!(
            registry
                .get(child.session_id())
                .unwrap()
                .read(|document| document.durable_revision())
                .unwrap(),
            revisions.0
        );
        assert_eq!(
            registry
                .get(invocation.source_session_id)
                .unwrap()
                .read(|document| document.durable_revision())
                .unwrap(),
            revisions.1
        );
    }
}

/// Cold startup restores direct and nested successful results from their actual original execution.
/// 冷启动从实际原执行恢复直接及嵌套成功结果。
/// No stream receipt, runtime task record or provider request is available to the recovery scan.
/// 恢复扫描没有流回执、运行时任务记录或供应商请求可用。
#[tokio::test]
async fn session_collaboration_startup_recovers_native_completed_child() {
    use vulcan_session_core::v1::collaboration::{
        ActivateChildTaskInput, CollaborationChildTaskState,
    };
    for nested in [false, true] {
        // Each scenario retains actual physical stores and begins with a published root reservation.
        // 每种场景保留实际物理存储，并以已发布根预留开始。
        let (_root, invocation, _target, _) = message_fixture(false).await;
        let direct = reserved_child(&invocation, &invocation.source_handle, "recover-direct").await;
        let (parent, child, execution_label) = if nested {
            let leaf = reserved_child(&invocation, &direct, "recover-nested").await;
            (direct, leaf, "recover-nested")
        } else {
            (invocation.source_handle.clone(), direct, "recover-direct")
        };
        // The source execution is activated before its business API, matching production ordering.
        // 来源执行在业务 API 前激活，与生产顺序一致。
        let execution = CollaborationExecutionId::try_new(execution_label).unwrap();
        parent
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.activate_child_task(ActivateChildTaskInput {
                        operation_id: CollaborationOperationId::try_new(format!(
                            "activate-{execution_label}"
                        ))
                        .unwrap(),
                        execution_id: execution.clone(),
                        task_id: vulcan_session_core::v1::ids::SubagentTaskId::try_new(
                            child.session_id().to_string(),
                        )
                        .unwrap(),
                        active_at_ms: 6,
                    })
                },
            )
            .unwrap();
        // Persist an actual completed API, assistant text and business terminal before losing all handles.
        // 丢失所有句柄前，持久化实际完成 API、助手正文及业务终态。
        super::super::results::finish_business_response(&child);
        assert_eq!(
            child
                .read(|document| document.collaboration_run_control())
                .unwrap(),
            CollaborationRunControl::Running
        );
        assert!(matches!(
            child.complete_successful_turn(10).await.unwrap().value,
            crate::session_runtime_registry_v1::SuccessfulTurnSettlement::Completed(_)
        ));
        let registry = Arc::new(SessionRuntimeRegistry::default());
        invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
        recover_tree_with_missing_main_hook(&invocation).await;
        // The original direct parent owns the result even when it is itself an embedded child.
        // 即使直接父节点自身也是嵌入子会话，结果仍归原直接父所有。
        let recovered = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
            .unwrap()
            .document;
        assert_eq!(
            recovered.collaboration_state().child_tasks[&execution].state,
            CollaborationChildTaskState::Terminal
        );
        let returned = recovered
            .events()
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::SubagentReturn(returned) => Some(returned),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(returned.len(), 1);
        assert_eq!(returned[0].content, "Business response");
        assert_eq!(returned[0].child_root_action_id.as_str(), execution_label);
        assert_eq!(returned[0].child_segment_generation, 1);
        assert!(
            recovered
                .pending_subagent_return_dispatch()
                .unwrap()
                .is_some()
        );
        // Repeated startup cannot create another terminal or another provider-visible result.
        // 重复启动不能创建另一个终态或另一个供应商可见结果。
        let revision = recovered.durable_revision();
        recover_tree_with_missing_main_hook(&invocation).await;
        assert_eq!(
            registry
                .get(parent.session_id())
                .unwrap()
                .read(|document| document.durable_revision())
                .unwrap(),
            revision
        );
    }
}

/// Cold startup preserves failed versus stopped results for direct and nested children.
/// 冷启动为直接及嵌套子节点保留失败与停止结果的区别。
/// Returns nothing; real stores prove terminal ownership, pending delivery and idempotent retries.
/// 不返回值；真实存储证明终态归属、待投递状态及幂等重试。
#[tokio::test]
async fn session_collaboration_startup_recovers_native_non_success_child() {
    use vulcan_session_core::v1::collaboration::{
        ActivateChildTaskInput, CollaborationChildTaskState, CollaborationChildTaskTerminalOutcome,
    };
    for nested in [false, true] {
        for failed in [false, true] {
            // Each combination uses independent physical storage and authentic reservations.
            // 每种组合使用独立物理存储及真实预留。
            let (_root, invocation, _target, _) = message_fixture(false).await;
            // Keep the root locator authoritative even when the result owner is embedded.
            // 即使结果所有者为嵌入节点，仍保持根定位器为权威。
            let direct =
                reserved_child(&invocation, &invocation.source_handle, "terminal-direct").await;
            // Select the actual direct parent of the execution under test.
            // 选择待验证执行的实际直接父节点。
            let (parent, child, label) = if nested {
                let leaf = reserved_child(&invocation, &direct, "terminal-nested").await;
                (direct, leaf, "terminal-nested")
            } else {
                (invocation.source_handle.clone(), direct, "terminal-direct")
            };
            // Failure can precede activation; a stop applies to an already active task.
            // 失败可以早于激活；停止适用于已活动任务。
            let execution = CollaborationExecutionId::try_new(label).unwrap();
            if !failed {
                parent
                    .mutate_durable(
                        SessionMutationAdmission::ControlPlane,
                        SessionFlushReason::MetadataChanged,
                        |document| {
                            document.activate_child_task(ActivateChildTaskInput {
                                operation_id: CollaborationOperationId::try_new(format!(
                                    "activate-{label}"
                                ))
                                .unwrap(),
                                execution_id: execution.clone(),
                                task_id: vulcan_session_core::v1::ids::SubagentTaskId::try_new(
                                    child.session_id().to_string(),
                                )
                                .unwrap(),
                                active_at_ms: 6,
                            })
                        },
                    )
                    .unwrap();
            }
            child
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        if failed {
                            document.fail_execution_boundary_and_close_turn(
                                "cold_startup_failure",
                                Some("original failure"),
                                10,
                            )?;
                        } else {
                            document.close_turn(
                                vulcan_session_core::v1::event::TurnOutcome::Stopped,
                                1,
                                10,
                            )?;
                        }
                        Ok(())
                    },
                )
                .unwrap();
            // Drop every canonical owner so repair can depend only on durable source facts.
            // 丢弃所有规范所有者，使修复只能依赖可靠来源事实。
            let registry = Arc::new(SessionRuntimeRegistry::default());
            invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
            recover_tree_with_missing_main_hook(&invocation).await;
            // Inspect actual disk publication in the original direct parent.
            // 检查原直接父节点中的实际磁盘发布。
            let recovered = load_session_v1_for_runtime(&parent.storage_locator().unwrap())
                .unwrap()
                .document;
            // The stored record carries both its terminal and source-backed result.
            // 存储记录同时携带其终态及来源支持的结果。
            let record = &recovered.collaboration_state().child_tasks[&execution];
            assert_eq!(record.state, CollaborationChildTaskState::Terminal);
            assert_eq!(
                record.non_success_result.as_ref().unwrap().proof.outcome,
                if failed {
                    CollaborationChildTaskTerminalOutcome::Failed
                } else {
                    CollaborationChildTaskTerminalOutcome::Stopped
                }
            );
            assert!(recovered.pending_collaboration_continuation().is_some());
            // Repeating startup must not allocate another parent result revision.
            // 重复启动不能分配另一个父结果修订。
            let revision = recovered.durable_revision();
            recover_tree_with_missing_main_hook(&invocation).await;
            assert_eq!(
                registry
                    .get(parent.session_id())
                    .unwrap()
                    .read(|document| document.durable_revision())
                    .unwrap(),
                revision
            );
        }
    }
}

/// A missing embedded store is reported while root and healthy sibling messages still repair.
/// 报告缺失嵌入存储，同时修复根和正常兄弟节点的消息。
/// The fixture moves only its own temporary child directory; no production data participates.
/// 夹具只移动自己的临时子目录；不涉及生产数据。
#[tokio::test]
async fn session_collaboration_startup_isolates_missing_embedded_child() {
    // All three owners and the receiver use actual temporary session storage.
    // 三个所有者及接收者均使用实际临时会话存储。
    let (_root, invocation, target, _) = message_fixture(false).await;
    let missing = reserved_child(&invocation, &invocation.source_handle, "missing-child").await;
    let healthy = reserved_child(&invocation, &invocation.source_handle, "healthy-child").await;
    // Root work must survive a child metadata failure too.
    // 根节点待办也必须在子元数据失败后继续处理。
    let root_message = record_pending_task(
        &invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "root-with-missing-child",
    )
    .await;
    // The healthy sibling has its own immutable message source and delivery identity.
    // 正常兄弟节点具有自己的不可变消息来源与投递身份。
    let mut healthy_invocation = recovery_invocation(
        invocation.state.clone(),
        invocation.controller_runtime.clone(),
        invocation.registry.clone(),
        healthy.session_id(),
        healthy.clone(),
    )
    .unwrap();
    healthy_invocation.source_turn_id = healthy
        .read(|document| document.active_turn().unwrap().turn_id.value())
        .unwrap();
    let sibling_message = record_pending_task(
        &healthy_invocation,
        &target,
        CollaborationTaskMessageKind::TaskSteer,
        "healthy-sibling",
    )
    .await;
    // Confirm both resolved paths remain inside this fixture's root before moving the directory.
    // 移动目录前确认两个解析路径都位于此夹具根内部。
    let directory = missing.storage_locator().unwrap().session_directory();
    let unavailable = directory.with_extension("unavailable");
    let root_directory = invocation.source_locator.session_directory();
    assert!(directory.starts_with(&root_directory));
    assert!(unavailable.starts_with(&root_directory));
    std::fs::rename(&directory, &unavailable).unwrap();
    // No live registry handle is available to mask the missing physical storage.
    // 没有可掩盖物理存储缺失的活动注册表句柄。
    let registry = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = registry.clone();
    let failure = startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [invocation.source_session_id].into_iter().collect(),
    )
    .await
    .unwrap_err();
    assert!(failure.contains(&missing.session_id().to_string()));
    assert!(registry.get(missing.session_id()).is_err());
    // Inspect the receiver's actual journal for both independently repaired messages.
    // 检查接收方实际日志中的两条独立修复消息。
    let receiver = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    for message in [root_message, sibling_message] {
        assert_eq!(
            receiver.collaboration_state().inbound_task_messages[&message.delivery_id].message,
            message
        );
    }
}
