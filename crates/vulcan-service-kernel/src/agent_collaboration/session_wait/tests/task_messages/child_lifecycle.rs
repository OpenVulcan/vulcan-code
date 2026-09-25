//! Durable child callbacks retain their original authentication after the parent API ends.
//! 父 API 结束后，持久子任务回调仍保留原始认证。

use super::*;
mod startup_abort;
mod terminal_retry;
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    TaskV1ExecutionFacts, collaboration_lifecycle::*,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskTerminalOutcome, ReserveChildTaskInput,
};

/// Closes a finished child through the production host path and verifies its cold V1 target stays sealed.
/// 通过生产宿主路径关闭已结束子代理，并验证冷加载后的 V1 目标仍封闭。
/// Returns no value; failures identify a missing close admission or a reopened question target.
/// 不返回值；失败表明关闭准入缺失或问题目标重新开放。
#[tokio::test]
async fn finished_child_close_preserves_listener_and_seals_cold_target() {
    use vulcan_workbench_model::{SubagentTaskPanelState, SubagentTaskRunState};

    let (_storage, invocation, _target, _context) = message_fixture(false).await;
    let child_id = Uuid::new_v4();
    let execution = CollaborationExecutionId::try_new("close-cold-child-execution").unwrap();
    let child_handle = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: invocation.source_session_id,
            parent_session_id: invocation.source_session_id,
            task_id: child_id,
            execution_id: execution.as_str().to_owned(),
            prompt: String::from("close child work"),
            display_title: String::from("close worker"),
            is_resumed: false,
            admitted_at_ms: 5,
        },
    )
    .unwrap();
    let child = participants::participant_for_handle(&invocation, child_id, &child_handle, None)
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
                    operation_id: CollaborationOperationId::try_new("close-child-reserve").unwrap(),
                    execution_id: execution.clone(),
                    task_id: child.task_id.clone().unwrap(),
                    parent_turn_id: TurnId::try_new(invocation.source_turn_id).unwrap(),
                    child,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    crate::agent_stream::non_success::publish_non_success(
        &invocation.source_handle,
        &child_handle,
        execution.as_str(),
        &vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: invocation.source_session_id.into(),
            parent_turn_id: None,
            child_thread_id: child_id.into(),
            kind: vulcan_session_boundary::SubagentMailboxEventKind::Failed,
            timestamp_ms: 7,
            payload: json!({"status": "failed", "error_text": "Original child failure"}),
        },
        7,
    )
    .unwrap();
    assert_ne!(
        child_handle
            .read(|document| document.collaboration_run_control())
            .unwrap(),
        CollaborationRunControl::Stopped
    );
    let runtime = {
        let mut guard = invocation.state.lock().unwrap();
        crate::tests::test_support::seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            invocation.source_session_id,
            &[SubagentTaskPanelState {
                task_id: child_id,
                parent_session_id: invocation.source_session_id,
                public_task_id: String::from("close-worker"),
                subagent_label: String::from("close-worker"),
                description: None,
                run_state: SubagentTaskRunState::Failed,
                result_text: String::from("failed"),
                conversation_state: None,
            }],
        );
        guard
            .detached_subagent_task_listeners
            .insert(child_id, execution.as_str().to_owned());
        Arc::clone(&guard.agent_runtime.as_ref().unwrap().task_runtime)
    };
    let child_locator = child_handle.storage_locator().unwrap();
    let closed = crate::agent_collaboration::close_child_agent_for_parent(
        &invocation.state,
        &runtime,
        invocation.source_session_id,
        json!({"target": "close-worker"}),
        None,
    )
    .await
    .unwrap();
    assert_eq!(closed.model_output["closed"], true);
    assert!(Arc::ptr_eq(
        &invocation.registry.get(child_id).unwrap(),
        &child_handle
    ));
    {
        let mut guard = invocation.state.lock().unwrap();
        assert!(crate::clear_webui_detached_listener_execution_if_current(
            &mut guard,
            child_id,
            execution.as_str(),
        ));
    }
    crate::agent_collaboration::release_closed_child_handle(&invocation.state, child_id)
        .await
        .unwrap();
    assert!(invocation.registry.get(child_id).is_err());
    let reloaded = load_session_v1_for_runtime(&child_locator).unwrap();
    assert_eq!(
        reloaded.document.collaboration_run_control(),
        CollaborationRunControl::Stopped
    );
    let rejected = ask_question(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation,
        child_id,
        String::from("Can a closed child still answer?"),
    )
    .await
    .unwrap_err();
    assert!(rejected.contains("stopped"));
}

/// Real-storage child reports enter the original parent's request through the production projector once.
/// 真实存储子报告通过生产投影器恰好一次进入原父请求。
/// Returns no value; assertions cover concurrent replay, cold reload and completion-gate ownership.
/// 不返回值；断言覆盖并发重放、冷加载和完成门归属。
#[tokio::test]
async fn child_non_success_service_projects_exact_durable_report_once() {
    // Exercise both pre-Provider failure and active-API failure/stop through the real publisher.
    // 通过真实发布器覆盖 Provider 前失败以及活动 API 失败和停止。
    for (status, active_api) in [("failed", false), ("failed", true), ("stopped", true)] {
        // Retain the real workspace, authenticated source invocation and canonical storage owners.
        // 保留真实工作区、已认证来源调用及规范存储所有者。
        let (_root, invocation, _target, _context) = message_fixture(false).await;
        // Child identity is allocated once and reused by its canonical embedded storage.
        // 子身份仅分配一次，并由规范嵌入存储复用。
        let child_id = Uuid::new_v4();
        // The runtime execution must match the actual embedded root input action.
        // 运行时执行必须匹配实际嵌入根输入动作。
        let execution = CollaborationExecutionId::try_new("child-report-projection-run").unwrap();
        // Use the production child-document creator, including its actual root ownership metadata.
        // 使用生产子文档创建器，包括实际根归属元数据。
        let child_handle = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
            &invocation.registry,
            TaskV1ExecutionFacts {
                root_session_id: invocation.source_session_id,
                parent_session_id: invocation.source_session_id,
                task_id: child_id,
                execution_id: execution.as_str().to_owned(),
                prompt: String::from("child work"),
                display_title: String::from("child"),
                is_resumed: false,
                admitted_at_ms: 5,
            },
        )
        .unwrap();
        // Resolve the real participant through the same production locator used by host admission.
        // 通过宿主准入所用同一生产定位器解析真实参与者。
        let child =
            participants::participant_for_handle(&invocation, child_id, &child_handle, None)
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
                        operation_id: CollaborationOperationId::try_new(
                            "report-projection-reserve",
                        )
                        .unwrap(),
                        execution_id: execution.clone(),
                        task_id: child.task_id.clone().unwrap(),
                        parent_turn_id: TurnId::try_new(invocation.source_turn_id).unwrap(),
                        child,
                        reserved_at_ms: 5,
                    })
                },
            )
            .unwrap();
        invocation
            .source_handle
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.activate_child_task(
                        vulcan_session_core::v1::collaboration::ActivateChildTaskInput {
                            operation_id: CollaborationOperationId::try_new("report-activate")
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
        let stopped_question = if status == "stopped" {
            let asked = ask_question(
                &crate::ServiceEventEmitter::new(vec![]),
                &invocation,
                child_id,
                String::from("Can you report before the stop?"),
            )
            .await
            .unwrap();
            let question_id =
                CollaborationQuestionId::try_new(asked["question_id"].as_str().unwrap()).unwrap();
            let delivery_id = invocation
                .source_handle
                .read(|document| {
                    document
                        .question_snapshot(&question_id)
                        .unwrap()
                        .delivery_id
                        .clone()
                })
                .unwrap();
            Some(delivery_id)
        } else {
            None
        };
        if active_api {
            start_api_at(&child_handle, 6);
        }
        // Typed host feedback is the only source of failure versus explicit-stop classification.
        // 强类型宿主反馈是失败与明确停止分类的唯一来源。
        let mailbox = vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: invocation.source_session_id.into(),
            parent_turn_id: None,
            child_thread_id: child_id.into(),
            kind: if status == "failed" {
                vulcan_session_boundary::SubagentMailboxEventKind::Failed
            } else {
                vulcan_session_boundary::SubagentMailboxEventKind::Stopped
            },
            timestamp_ms: 7,
            payload: json!({"status": status, "error_text": "Original durable failure"}),
        };
        crate::agent_stream::non_success::publish_non_success(
            &invocation.source_handle,
            &child_handle,
            execution.as_str(),
            &mailbox,
            7,
        )
        .unwrap();
        // Repeating the host terminal preserves both documents and their original report revision.
        // 重复宿主终态保持双方文档及原报告修订不变。
        let revisions = (
            invocation
                .source_handle
                .read(|doc| doc.memory_revision())
                .unwrap(),
            child_handle.read(|doc| doc.memory_revision()).unwrap(),
        );
        crate::agent_stream::non_success::publish_non_success(
            &invocation.source_handle,
            &child_handle,
            execution.as_str(),
            &mailbox,
            8,
        )
        .unwrap();
        assert_eq!(
            revisions,
            (
                invocation
                    .source_handle
                    .read(|doc| doc.memory_revision())
                    .unwrap(),
                child_handle.read(|doc| doc.memory_revision()).unwrap()
            )
        );
        assert!(
            child_handle
                .read(|doc| doc.active_api().is_none() && doc.active_turn().unwrap().closed)
                .unwrap()
        );
        if status == "stopped" {
            assert_eq!(
                child_handle
                    .read(|document| document.collaboration_run_control())
                    .unwrap(),
                CollaborationRunControl::Stopped,
                "the stopped child must durably reject later collaboration messages"
            );
            let delivery_id = stopped_question.as_ref().unwrap();
            assert_eq!(
                child_handle
                    .read(|document| document
                        .inbound_delivery_snapshot(delivery_id)
                        .unwrap()
                        .state)
                    .unwrap(),
                CollaborationDeliveryState::Failed
            );
        }
        // Replay through the actual detached listener to verify routing and post-durability mailbox publication.
        // 经实际分离监听器重放，以验证路由及持久化后的邮箱发布。
        replay_non_success_through_listener(
            &invocation,
            &child_handle,
            execution.as_str(),
            status,
            &mailbox,
        )
        .await;
        if let Some(delivery_id) = stopped_question.as_ref() {
            assert_eq!(
                invocation
                    .source_handle
                    .read(|document| document.delivery_snapshot(delivery_id).unwrap().state)
                    .unwrap(),
                CollaborationDeliveryState::Failed,
                "the detached stop listener must return the failure to the question owner"
            );
        }
        invocation
            .source_handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| document.interrupt_active_api(8),
            )
            .unwrap();
        // Concurrent callers serialize under the actual lifecycle gate and cannot insert two copies.
        // 并发调用方在实际生命周期门下串行，不能插入两份副本。
        let revision = invocation
            .source_handle
            .read(|document| document.memory_revision())
            .unwrap();
        // Both futures use the real request-boundary candidate collector and durable writer.
        // 两个 Future 均使用真实请求边界候选收集器和可靠写入器。
        let (first, second) = tokio::join!(
            project_collaboration_before_api(&invocation.state, &invocation.source_handle, 9),
            project_collaboration_before_api(&invocation.state, &invocation.source_handle, 9),
        );
        first.unwrap();
        second.unwrap();
        assert_eq!(
            invocation
                .source_handle
                .read(|document| document.memory_revision())
                .unwrap(),
            revision + 1
        );
        assert!(matches!(
            invocation
                .source_handle
                .complete_successful_turn(10)
                .await
                .unwrap()
                .value,
            crate::session_runtime_registry_v1::SuccessfulTurnSettlement::Collaboration
        ));
        // Verify the exact stored User message after reloading actual parent storage.
        // 重载实际父存储后核验精确已存 User 消息。
        let loaded =
            load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
                .unwrap()
                .document;
        // Projection authority remains bound to the original owned child and original parent turn.
        // 投影权威始终绑定原所属子任务和原父回合。
        let projection = loaded.collaboration_state().child_tasks[&execution]
            .non_success_projection
            .as_ref()
            .unwrap();
        assert_eq!(projection.parent_turn_id.value(), invocation.source_turn_id);
        assert!(loaded.events().iter().any(|event| event.seq == projection.inputs[0].input_seq
        && matches!(&event.event, EventBody::UserInput(input)
            if (status == "stopped" || input.text.contains("Original durable failure")) && input.text.contains("kind=\"task_result\""))));
        assert_eq!(
            loaded.collaboration_state().child_tasks[&execution]
                .non_success_result
                .as_ref()
                .unwrap()
                .proof
                .failure_notes
                .is_empty(),
            status == "stopped"
        );
        assert!(loaded.pending_collaboration_continuation().is_some());
        // A subsequent actual business API is the first operation allowed to consume these inputs.
        // 后续实际业务 API 才是首个允许消费这些输入的操作。
        start_api_at(&invocation.source_handle, 11);
        assert!(
            invocation
                .source_handle
                .read(|document| document.pending_collaboration_continuation())
                .unwrap()
                .is_none()
        );
    }
}

/// Replays a real host terminal through the production listener without dispatching a Provider.
/// 经生产监听器重放真实宿主终态，不分发 Provider。
/// Inputs retain exact canonical owners and feedback; assertions verify cleanup and the published mailbox.
/// 输入保留精确规范归属及反馈；断言验证清理与已发布邮箱。
async fn replay_non_success_through_listener(
    invocation: &ValidatedInvocation,
    child: &Arc<SessionHandle>,
    execution: &str,
    status: &str,
    mailbox: &vulcan_session_boundary::SubagentMailboxEvent,
) {
    // Use the production runtime gate and an isolated in-memory mailbox.
    // 使用生产运行时门及隔离内存邮箱。
    let runtime = crate::tests::test_support::test_agent_runtime_bridge();
    runtime
        .task_runtime
        .expect_durable_task_return(child.session_id(), execution)
        .unwrap();
    // Retain the canonical resource catalog and prevent this offline test from restarting the parent.
    // 保留规范资源目录，并阻止离线测试重新启动父会话。
    let catalog = {
        // The guard only mutates process-local listener/UI state, never durable document ownership.
        // 此守卫仅修改进程内监听器和界面状态，绝不修改持久文档归属。
        let mut guard = invocation.state.lock().unwrap();
        guard.agent_runtime = Some(runtime.clone());
        guard
            .detached_subagent_task_listeners
            .insert(child.session_id(), execution.to_owned());
        guard
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .session_run_phases
            .insert(
                invocation.source_session_id,
                crate::SessionRunPhase::Stopped,
            );
        guard.project_resource_catalog_registry.clone()
    };
    // This sink intentionally has no final assistant, so accidental success routing fails.
    // 此接收器刻意没有最终助手正文，因此误走成功路由会失败。
    let mut sink = crate::agent_stream::ProviderV1StreamSink::new(child.clone(), catalog);
    // Retain the same telemetry owner used by real detached execution.
    // 保留真实分离执行使用的同类遥测所有者。
    let mut telemetry = crate::agent_stream::ProviderTelemetryHookGuard::new_with_persistence(
        invocation.state.clone(),
        None,
    );
    // The event uses the actual host payload schema, including exact listener execution identity.
    // 事件采用实际宿主载荷结构，包括精确监听器执行身份。
    let event = vulcan_agent_runtime::agent_runtime::events::RuntimeEvent::new(
        vulcan_agent_runtime::agent_runtime::events::RuntimeEventKind::DelegateFinished,
        "Child terminal",
        status,
    )
    .with_metadata(Some(json!({"kind": "subagent_task", "payload": {
        "task_id": "report-child", "subagent_task_id": child.session_id().to_string(),
        "execution_id": execution, "execution_status": status, "subagent": "report-child",
        "result": "Original durable failure", "subagent_mailbox_event": mailbox,
    }})));
    crate::agent_stream::handle_webui_detached_subagent_task_event(
        &vulcan_service_api::ServiceEventEmitter::new(vec![]), &invocation.state, &invocation.controller_runtime,
        &runtime, child.session_id(), execution, child, &mut telemetry, &mut sink,
        vulcan_agent_runtime::agent_runtime::task_runtime::DetachedTaskRuntimeEvent::ParentRuntime { parent_session_id: invocation.source_session_id, event },
    ).await.unwrap();
    assert!(
        !invocation
            .state
            .lock()
            .unwrap()
            .detached_subagent_task_listeners
            .contains_key(&child.session_id())
    );
    assert_eq!(
        runtime
            .task_runtime
            .take_subagent_events_for_parent(invocation.source_session_id.into())
            .unwrap(),
        vec![mailbox.clone()]
    );
}

/// Verifies actual host callbacks after API interruption and their persisted state after cold reload.
/// 验证 API 中断后的实际宿主回调，以及冷加载后的持久状态。
#[tokio::test]
async fn child_lifecycle_callbacks_survive_source_api_and_reject_forged_origin() {
    let (_root, invocation, _target, context) = message_fixture(false).await;
    let child_id = Uuid::new_v4();
    let execution_id = String::from("durable-child-run");
    let child_handle = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &invocation.registry,
        TaskV1ExecutionFacts {
            root_session_id: invocation.source_session_id,
            parent_session_id: invocation.source_session_id,
            task_id: child_id,
            execution_id: execution_id.clone(),
            prompt: String::from("child work"),
            display_title: String::from("child"),
            is_resumed: false,
            admitted_at_ms: 5,
        },
    )
    .unwrap();
    let child = participants::participant_for_handle(&invocation, child_id, &child_handle, None)
        .await
        .unwrap();
    let execution = ChildTaskExecutionIdentity {
        task_session_id: child_id,
        execution_id,
        task_id: child.task_id.clone().unwrap(),
    };
    let operation_id = CollaborationOperationId::try_new(invocation.operation_id.clone()).unwrap();
    let state_weak = Arc::downgrade(&invocation.state);
    // The fixture uses a real prepared ToolCall; its exact authenticated identity is persisted.
    // 夹具使用真实已准备 ToolCall；持久保存它的精确已认证身份。
    let validated =
        validate_host_invocation(&state_weak, &context, &operation_id, None, None).unwrap();
    invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_task(ReserveChildTaskInput {
                    source_tool_name: validated.source_tool_id.clone(),
                    execution_id: CollaborationExecutionId::try_new(execution.execution_id.clone())
                        .unwrap(),
                    operation_id: operation_id.clone(),
                    task_id: execution.task_id.clone(),
                    parent_turn_id: TurnId::try_new(validated.source_turn_id).unwrap(),
                    child,
                    reserved_at_ms: 5,
                })
            },
        )
        .unwrap();
    invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| document.interrupt_active_api(6),
        )
        .unwrap();
    assert!(validate_host_invocation(&state_weak, &context, &operation_id, None, None).is_err());
    let port = SessionV1NativeAgentCollaborationRuntime {
        state: state_weak,
        app: crate::ServiceEventEmitter::new(vec![]),
    };
    let activation = ChildTaskActivationRequest {
        invocation: context.clone(),
        operation_id: CollaborationOperationId::try_new(format!(
            "{}:activate",
            operation_id.as_str()
        ))
        .unwrap(),
        execution: execution.clone(),
        active_at_ms: 7,
    };
    let active = port.activate_child_task(activation).await.unwrap();
    assert_eq!(active.execution_id.as_str(), execution.execution_id);
    let terminal = ChildTaskTerminalRequest {
        invocation: context,
        operation_id: CollaborationOperationId::try_new(format!(
            "{}:terminal:failed",
            operation_id.as_str()
        ))
        .unwrap(),
        execution: execution.clone(),
        outcome: CollaborationChildTaskTerminalOutcome::Failed,
        terminal_at_ms: 8,
    };
    // Physical success cannot bypass the exact durable return publisher.
    // 物理成功不能绕过精确持久回流发布器。
    let mut premature_success = terminal.clone();
    premature_success.outcome = CollaborationChildTaskTerminalOutcome::Completed;
    assert!(
        port.terminal_child_task(premature_success)
            .await
            .unwrap_err()
            .contains("exact durable return")
    );
    let mut forged = terminal.clone();
    forged.invocation.controller_tool_call_id = String::from("different-source");
    assert!(
        port.terminal_child_task(forged)
            .await
            .unwrap_err()
            .contains("original reservation")
    );
    let receipt = port.terminal_child_task(terminal.clone()).await.unwrap();
    assert_eq!(receipt.execution_id.as_str(), execution.execution_id);
    let retry = port.terminal_child_task(terminal).await.unwrap();
    assert!(!retry.changed);
    assert_eq!(retry.document_revision, receipt.document_revision);
    // Canonical disk loading must preserve the original source fingerprint and completed obligation.
    // 规范磁盘加载必须保留原始来源指纹及已完成义务。
    let restored = vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(
        &invocation.source_handle.storage_locator().unwrap(),
    )
    .unwrap()
    .document;
    let record = &restored.collaboration_state().child_tasks[&receipt.execution_id];
    assert_eq!(record.reservation_operation_id, operation_id);
    assert!(
        restored
            .completion_obligations(record.parent_turn_id)
            .open_child_task_ids
            .is_empty()
    );
}
