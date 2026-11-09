//! Published child results stop retrying a continuation whose original parent turn has ended.
//! 已发布子结果不再重试原父回合已结束的续跑。

use super::*;
use crate::agent_stream::{ProviderTelemetryHookGuard, retry, startup_rejection::capture_owner};
use vulcan_agent_runtime::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    DetachedTaskRuntimeEvent, TaskV1ExecutionFacts,
};
use vulcan_session_core::v1::collaboration::{
    ActivateChildTaskInput, CollaborationChildTaskState, CollaborationChildTaskTerminalOutcome,
    CollaborationExecutionId, CollaborationOperationId, CollaborationParticipant,
    CollaborationRunControl, ReserveChildTaskInput, SetRunControlInput,
};
use vulcan_session_core::v1::event::TurnOutcome;

/// Exercises publication failure-to-dispatch, then original-parent closure through the real retry driver.
/// 经由真实重试驱动器验证结果发布后的分发失败，以及随后原父回合闭合。
#[tokio::test]
async fn published_child_terminal_retires_after_parent_turn_ends() {
    // Stop can revoke the open turn, close it, or be followed by another owner with or without an API.
    // 停止可撤销开放回合、关闭回合，或后接有或没有活动 API 的其他所有者。
    for (close_original, replacement_api) in [
        (false, None),
        (true, None),
        (true, Some(false)),
        (true, Some(true)),
    ] {
        let parent_id = SessionId::new_v4();
        let storage = crate::tests::test_support::TestHostRuntimeRoot::new(parent_id);
        let controller = storage.attached_host_runtime_handle().await;
        let parent = SessionHandle::new(
            parent_id,
            storage.v1_storage_locator(),
            vulcan_workspace_store::workspace_store::load_session_v1_full(
                &storage.v1_storage_locator(),
            )
            .unwrap()
            .document,
        );
        let original_turn = parent
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("retired-parent-root").unwrap(),
                    "Parent work",
                    Vec::new(),
                ),
                2,
            )
            .unwrap()
            .value;
        let mut bridge =
            crate::tests::test_support::test_service_state_with_parent_session(parent_id);
        bridge.controller_runtime = Some(controller.clone());
        bridge
            .session_runtime_registry
            .register(parent.clone())
            .unwrap();
        let child_id = SessionId::new_v4();
        let execution = CollaborationExecutionId::try_new("retired-parent-child").unwrap();
        let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
            &bridge.session_runtime_registry,
            TaskV1ExecutionFacts {
                root_session_id: parent_id,
                parent_session_id: parent_id,
                task_id: child_id,
                execution_id: execution.as_str().into(),
                prompt: "Child work".into(),
                display_title: "Child".into(),
                is_resumed: false,
                admitted_at_ms: 3,
            },
        )
        .unwrap();
        let startup_owner = capture_owner(&child, execution.as_str()).unwrap();
        parent
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.apply_atomic_transition(|document| {
                        let task_id = SubagentTaskId::try_new(child_id.to_string())?;
                        document.reserve_child_task(ReserveChildTaskInput {
                            operation_id: CollaborationOperationId::try_new(
                                "retired-parent-reserve",
                            )?,
                            source_tool_name: "spawn_agent".into(),
                            execution_id: execution.clone(),
                            task_id: task_id.clone(),
                            parent_turn_id: original_turn,
                            child: CollaborationParticipant::new(
                                child_id.to_string(),
                                parent_id.to_string(),
                                storage.v1_storage_locator().project_id(),
                                1,
                                None,
                                Some(task_id.clone()),
                            )?,
                            reserved_at_ms: 3,
                        })?;
                        document.activate_child_task(ActivateChildTaskInput {
                            operation_id: CollaborationOperationId::try_new(
                                "retired-parent-activate",
                            )?,
                            execution_id: execution.clone(),
                            task_id,
                            active_at_ms: 4,
                        })
                    })
                },
            )
            .unwrap();
        let mut sink = ProviderV1StreamSink::new(
            child.clone(),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&child, &mut sink);
        sink.append_assistant_delta("retained-result-marker", 5)
            .unwrap();
        sink.accept_terminal(terminal_facts(0)).await.unwrap();
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        runtime
            .task_runtime
            .expect_durable_task_return(child_id, execution.as_str())
            .unwrap();
        bridge.agent_runtime = Some(runtime.clone());
        bridge
            .detached_subagent_task_listeners
            .insert(child_id, execution.as_str().into());
        let state = Arc::new(std::sync::Mutex::new(bridge));
        let mailbox = vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: parent_id.into(),
            parent_turn_id: None,
            child_thread_id: child_id.into(),
            kind: vulcan_session_boundary::SubagentMailboxEventKind::Completed,
            timestamp_ms: 8,
            payload: json!({"status":"completed", "execution_id":execution.as_str(), "result_text":"retained-result-marker"}),
        };
        let terminal = DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: parent_id,
            event: RuntimeEvent::new(
                RuntimeEventKind::DelegateFinished,
                "Completed",
                "retained-result-marker",
            )
            .with_metadata(Some(json!({"kind":"subagent_task", "payload":{
                "subagent_task_id":child_id.to_string(), "execution_id":execution.as_str(),
                "subagent_mailbox_event":mailbox
            }}))),
        };
        let emitter = crate::ServiceEventEmitter::new(Vec::new());
        let mut telemetry = ProviderTelemetryHookGuard::new_with_persistence(state.clone(), None);
        // Missing original Hook deterministically rejects dispatch without contacting a provider.
        // 缺失原 Hook 确定性地拒绝分发，不接触供应商。
        for _ in 0..2 {
            let error = crate::agent_stream::handle_webui_detached_subagent_task_event(
                &emitter,
                &state,
                &controller,
                &runtime,
                child_id,
                execution.as_str(),
                &child,
                &mut telemetry,
                &mut sink,
                terminal.clone(),
            )
            .await
            .unwrap_err();
            assert!(error.contains("initial_business.prior_execution"));
            assert!(
                state
                    .lock()
                    .unwrap()
                    .detached_subagent_task_listeners
                    .contains_key(&child_id)
            );
        }
        let published = parent
            .read(|document| {
                let task = &document.collaboration_state().child_tasks[&execution];
                assert_eq!(task.state, CollaborationChildTaskState::Terminal);
                assert_eq!(
                    task.terminal_outcome,
                    Some(CollaborationChildTaskTerminalOutcome::Completed)
                );
                let returns = document
                    .events()
                    .iter()
                    .filter(|event| matches!(event.event, EventBody::SubagentReturn(_)))
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(returns.len(), 1);
                assert!(
                    !document.subagent_return_dispatch_started(
                        &document
                            .pending_subagent_return_dispatch()
                            .unwrap()
                            .unwrap()
                    )
                );
                returns
            })
            .unwrap();
        if close_original {
            parent
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::TurnCompleted,
                    |document| document.close_turn(TurnOutcome::Stopped, 1, 10),
                )
                .unwrap();
        } else {
            // Keep the UI phase unchanged so the canonical stop, not its retained projection, wins.
            // 保持 UI 阶段不变，使规范停止而非保留投影取得权威。
            parent
                .set_collaboration_run_control_durable(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("stop-original-parent")
                        .unwrap(),
                    state: CollaborationRunControl::Stopped,
                    changed_at_ms: 10,
                })
                .unwrap();
        }
        if let Some(with_api) = replacement_api {
            parent
                .submit_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("replacement-root").unwrap(),
                        "New work",
                        Vec::new(),
                    ),
                    11,
                )
                .unwrap();
            if with_api {
                let mut successor_sink = ProviderV1StreamSink::new(
                    parent.clone(),
                    Arc::new(ProjectResourceCatalogRegistry::default()),
                );
                let mut facts = request_facts();
                facts.prepared_at_ms = 12;
                prepare_sink_request_with_facts(&parent, &mut successor_sink, facts);
            }
        }
        let before_retry = parent.read(Clone::clone).unwrap();
        // EOF must retire the old listener instead of retrying forever against the new owner.
        // 流结束必须退休旧监听器，不能针对新所有者无限重试。
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            retry::drive(
                &emitter,
                &state,
                &controller,
                &runtime,
                parent_id,
                child_id,
                execution.as_str(),
                &child,
                startup_owner,
                futures_util::stream::iter(vec![terminal]),
                &mut telemetry,
                &mut sink,
            ),
        )
        .await
        .unwrap();
        assert!(
            !state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .contains_key(&child_id)
        );
        assert!(state.lock().unwrap().active_stream_tasks.is_empty());
        let after_retry = parent.read(Clone::clone).unwrap();
        assert_eq!(after_retry.metadata, before_retry.metadata);
        assert_eq!(after_retry.events(), before_retry.events());
        assert_eq!(after_retry.active_turn(), before_retry.active_turn());
        assert_eq!(after_retry.active_api(), before_retry.active_api());
        assert_eq!(
            after_retry.memory_revision(),
            before_retry.memory_revision()
        );
        assert_eq!(
            after_retry.durable_revision(),
            before_retry.durable_revision()
        );
        let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(
            &storage.v1_storage_locator(),
        )
        .unwrap()
        .document;
        assert_eq!(
            loaded
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventBody::SubagentReturn(_)))
                .cloned()
                .collect::<Vec<_>>(),
            published
        );
        assert_eq!(
            loaded.collaboration_state().child_tasks[&execution].terminal_outcome,
            Some(CollaborationChildTaskTerminalOutcome::Completed)
        );
        assert_eq!(
            runtime
                .task_runtime
                .take_subagent_events_for_parent(parent_id.into())
                .unwrap(),
            vec![mailbox]
        );
    }
}
