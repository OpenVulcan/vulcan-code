//! Real-storage startup aborts distinguish unaccepted children from owned reservations.
//! 真实存储启动中止区分尚未接纳子任务与所属预留。

use super::*;
use crate::agent_stream::{startup_abort::handle_startup_abort, startup_rejection::capture_owner};
use vulcan_agent_runtime::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};

/// All admission stages close only their original child and publish or withdraw the exact return gate.
/// 所有准入阶段仅关闭原子任务，并发布或撤销精确回流门。
/// Returns no value; asserts persistence, idempotence and parent completion ownership without a Provider.
/// 不返回值；不使用 Provider，断言持久化、幂等性及父完成归属。
#[tokio::test]
async fn explicit_startup_abort_settles_each_admission_stage() {
    for stage in ["unreserved", "reserved", "active"] {
        // The fixture retains actual canonical parent storage and its original tool invocation.
        // 夹具保留实际规范父存储及其原始工具调用。
        let (_root, invocation, _target, _context) = message_fixture(false).await;
        // Each stage receives an independent reusable physical child.
        // 每个阶段获得独立的可复用物理子任务。
        let child_id = Uuid::new_v4();
        // The protocol execution is also the actual root-input identity in embedded storage.
        // 协议执行也是嵌入存储中的实际根输入身份。
        let execution = CollaborationExecutionId::try_new("aborted-startup").unwrap();
        // Production creation establishes the exact embedded child directory and root turn.
        // 生产创建器建立精确嵌入子目录及根回合。
        let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
            &invocation.registry,
            TaskV1ExecutionFacts {
                root_session_id: invocation.source_session_id,
                parent_session_id: invocation.source_session_id,
                task_id: child_id,
                execution_id: execution.as_str().to_owned(),
                prompt: "child work".into(),
                display_title: "child".into(),
                is_resumed: false,
                admitted_at_ms: 5,
            },
        )
        .unwrap();
        // Freeze ownership at the same point as real listener admission, before the abort occurs.
        // 在与真实监听器准入相同的位置冻结归属，此时尚未发生中止。
        let owner = capture_owner(&child, execution.as_str()).unwrap();
        if stage != "unreserved" {
            // Resolve participant identity from the real storage locator instead of constructing a guessed path.
            // 从实际存储定位器解析参与者身份，不构造猜测路径。
            let participant =
                participants::participant_for_handle(&invocation, child_id, &child, None)
                    .await
                    .unwrap();
            invocation
                .source_handle
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.reserve_child_task(ReserveChildTaskInput {
                            source_tool_name: invocation.source_tool_id.clone(),
                            operation_id: CollaborationOperationId::try_new("startup-reserve")
                                .unwrap(),
                            execution_id: execution.clone(),
                            task_id: participant.task_id.clone().unwrap(),
                            parent_turn_id: TurnId::try_new(invocation.source_turn_id).unwrap(),
                            child: participant,
                            reserved_at_ms: 5,
                        })
                    },
                )
                .unwrap();
        }
        if stage == "active" {
            invocation
                .source_handle
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.activate_child_task(
                            vulcan_session_core::v1::collaboration::ActivateChildTaskInput {
                                operation_id: CollaborationOperationId::try_new("startup-active")
                                    .unwrap(),
                                execution_id: execution.clone(),
                                task_id: vulcan_session_core::v1::ids::SubagentTaskId::try_new(
                                    child_id.to_string(),
                                )
                                .unwrap(),
                                active_at_ms: 6,
                            },
                        )
                    },
                )
                .unwrap();
        }
        // No Provider or active runtime lease exists after the startup guard emits its explicit abort.
        // 启动守卫发出明确中止后，不再存在 Provider 或活动运行时租约。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        runtime
            .task_runtime
            .expect_durable_task_return(child_id, execution.as_str())
            .unwrap();
        {
            // Register the same listener generation and UI runtime used by production cleanup.
            // 登记生产清理使用的同一监听器代次及界面运行时。
            let mut guard = invocation.state.lock().unwrap();
            guard.agent_runtime = Some(runtime.clone());
            guard
                .detached_subagent_task_listeners
                .insert(child_id, execution.as_str().to_owned());
        }
        // Preserve the exact original host error in the typed feedback body.
        // 在强类型反馈正文中保留精确原始宿主错误。
        let mailbox = vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: invocation.source_session_id.into(),
            parent_turn_id: None,
            child_thread_id: child_id.into(),
            kind: vulcan_session_boundary::SubagentMailboxEventKind::Failed,
            timestamp_ms: 7,
            payload: json!({"status": "failed", "execution_id": execution.as_str(), "error_text": "original startup failure"}),
        };
        // Use the actual runtime metadata contract consumed by the service terminal validator.
        // 使用服务终态校验器消费的实际运行时元数据契约。
        let event = RuntimeEvent::new(RuntimeEventKind::DelegateFinished, "Startup failed", "original startup failure")
            .with_metadata(Some(json!({"kind": "subagent_task", "payload": {
                "subagent_task_id": child_id.to_string(), "execution_id": execution.as_str(), "subagent_mailbox_event": mailbox,
            }})));
        // Failed publication must not affect either document when the event identity is wrong.
        // 事件身份错误时，失败发布不得影响任一文档。
        let mut wrong_event = event.clone();
        wrong_event.metadata.as_mut().unwrap()["payload"]["execution_id"] =
            json!("another-startup");
        // Retain source revision to prove rejection occurs before child closure.
        // 保留来源修订，证明拒绝发生在子回合闭合之前。
        let before = child.read(|doc| doc.memory_revision()).unwrap();
        assert!(
            handle_startup_abort(
                &vulcan_service_api::ServiceEventEmitter::new(vec![]),
                &invocation.state,
                &runtime,
                invocation.source_session_id,
                child_id,
                execution.as_str(),
                &child,
                owner,
                &wrong_event
            )
            .is_err()
        );
        assert_eq!(child.read(|doc| doc.memory_revision()).unwrap(), before);
        handle_startup_abort(
            &vulcan_service_api::ServiceEventEmitter::new(vec![]),
            &invocation.state,
            &runtime,
            invocation.source_session_id,
            child_id,
            execution.as_str(),
            &child,
            owner,
            &event,
        )
        .unwrap();
        // Cold load proves closure and its original diagnostic were actually published.
        // 冷加载证明闭合及其原始诊断确实已经发布。
        let loaded_child = load_session_v1_for_runtime(&child.storage_locator().unwrap())
            .unwrap()
            .document;
        assert!(loaded_child.active_turn().is_none());
        assert!(
            loaded_child
                .events()
                .iter()
                .any(|entry| matches!(&entry.event, EventBody::DisplayNote(note)
            if note.redacted_detail.as_deref() == Some("original startup failure")))
        );
        // Parent result exists exactly when a durable owned reservation had already been accepted.
        // 仅在已接纳可靠所属预留时，父结果才存在。
        let loaded_parent =
            load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
                .unwrap()
                .document;
        if stage == "unreserved" {
            assert!(
                !loaded_parent
                    .collaboration_state()
                    .child_tasks
                    .contains_key(&execution)
            );
            assert!(
                !runtime
                    .task_runtime
                    .has_subagent_events_for_parent(invocation.source_session_id.into())
                    .unwrap()
            );
            assert!(
                runtime
                    .task_runtime
                    .publish_durable_task_return(execution.as_str(), mailbox.clone())
                    .is_err()
            );
        } else {
            assert!(
                loaded_parent.collaboration_state().child_tasks[&execution]
                    .non_success_result
                    .is_some()
            );
            assert!(loaded_parent.pending_collaboration_continuation().is_some());
            assert_eq!(
                runtime
                    .task_runtime
                    .take_subagent_events_for_parent(invocation.source_session_id.into())
                    .unwrap(),
                vec![mailbox]
            );
        }
        assert!(
            !invocation
                .state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .contains_key(&child_id)
        );
        // Re-delivery after listener retirement is a no-op, never another parent result.
        // 监听器退休后的重复投递为空操作，绝不产生第二份父结果。
        handle_startup_abort(
            &vulcan_service_api::ServiceEventEmitter::new(vec![]),
            &invocation.state,
            &runtime,
            invocation.source_session_id,
            child_id,
            execution.as_str(),
            &child,
            owner,
            &event,
        )
        .unwrap();
        assert_eq!(
            invocation
                .source_handle
                .read(|doc| doc.memory_revision())
                .unwrap(),
            loaded_parent.memory_revision()
        );
    }
}
