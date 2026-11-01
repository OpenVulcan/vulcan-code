//! Queued child execution chains settle original ownership before returning their final answer.
//! 排队子执行链在返回最终回答前结算原始归属。

use super::*;
use vulcan_agent_runtime::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    DetachedTaskRuntimeEvent, TaskV1ExecutionFacts,
};
use vulcan_session_core::v1::collaboration::{
    ActivateChildTaskInput, CollaborationChildTaskState, CollaborationExecutionId,
    CollaborationOperationId, CollaborationParticipant, ReserveChildTaskInput,
};
use vulcan_session_core::v1::event::EventBody;

/// Selects live terminal delivery, startup cancellation or process-loss recovery.
/// 选择实时终态投递、启动取消或进程丢失恢复。
#[derive(Clone, Copy, PartialEq, Eq)]
enum QueuedDelivery {
    /// Normal runtime terminal feedback.
    /// 正常运行时终态反馈。
    Live,
    /// Cancellation while startup admission may still be publishing.
    /// 启动准入可能仍在发布时取消。
    StartupAbort,
    /// A cold process has no runtime lease or captured preparation.
    /// 冷进程没有运行时租约或已捕获准备。
    ProcessExit,
}

/// Runs three real child turns; `published_successor` reproduces historical prefix overlap before final delivery.
/// 运行三个真实子回合；`published_successor` 在最终投递前复现历史前缀重叠。
/// Asserts original/final content, exact reservation settlement, cold loading and eventual parent completion.
/// 断言原始及最终正文、精确预留结算、冷加载及父回合最终完成。
async fn queued_child_chain(
    published_successor: bool,
    replace_listener: bool,
    stop_parent: bool,
    terminal_kind: Option<vulcan_session_boundary::SubagentMailboxEventKind>,
    delivery: QueuedDelivery,
) {
    // Both pre-provider failures leave the last successor in Reserved state.
    // 两种供应商启动前失败均让最后后继保持 Reserved 状态。
    let startup_abort = delivery != QueuedDelivery::Live;
    // Distinguish the successful admission case from failure and cancellation coverage.
    // 区分成功准入用例与失败、取消覆盖。
    let non_success = terminal_kind
        .is_some_and(|kind| kind != vulcan_session_boundary::SubagentMailboxEventKind::Completed);
    // The indexed fixture owns isolated real storage for both parent and embedded child.
    // 索引夹具拥有父会话及嵌入子会话的隔离真实存储。
    let parent_id = SessionId::new_v4();
    // Keep the indexed database and session directory alive for every production callback.
    // 在全部生产回调期间保持索引数据库及会话目录存活。
    let storage = crate::tests::test_support::TestHostRuntimeRoot::new(parent_id);
    // Attach the production controller without launching any model.
    // 附加生产控制器，不启动任何模型。
    let controller = storage.attached_host_runtime_handle().await;
    // All parent writes go through the canonical real file writer.
    // 全部父写入经由规范真实文件写入器。
    let parent = SessionHandle::new(
        parent_id,
        storage.v1_storage_locator(),
        vulcan_workspace_store::workspace_store::load_session_v1_full(
            &storage.v1_storage_locator(),
        )
        .unwrap()
        .document,
    );
    parent
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queue-parent").unwrap(),
                "parent task",
                vec![],
            ),
            2,
        )
        .unwrap();
    // Retain the exact business turn whose completion must eventually be released.
    // 保留最终必须解除完成约束的精确业务回合。
    let parent_turn = parent
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    // The service registry is authoritative for recovery and live result publication.
    // 服务注册表是恢复及实时结果发布的权威。
    let mut bridge = crate::tests::test_support::test_service_state_with_parent_session(parent_id);
    bridge.controller_runtime = Some(controller.clone());
    bridge
        .session_runtime_registry
        .register(parent.clone())
        .unwrap();
    // This physical child is reused by three distinct execution IDs.
    // 三个不同执行标识复用该物理子会话。
    let child_id = SessionId::new_v4();
    // Only the original spawn owns the historical parent reservation, matching queued-message production.
    // 只有最初派生拥有历史父预留，与排队消息生产路径一致。
    let original = CollaborationExecutionId::try_new("queue-original").unwrap();
    // Create the source through the actual embedded admission path.
    // 经由实际嵌入准入路径创建来源。
    let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
        &bridge.session_runtime_registry,
        TaskV1ExecutionFacts {
            root_session_id: parent_id,
            parent_session_id: parent_id,
            task_id: child_id,
            execution_id: original.as_str().into(),
            prompt: "original task".into(),
            display_title: "queue worker".into(),
            is_resumed: false,
            admitted_at_ms: 3,
        },
    )
    .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.apply_atomic_transition(|document| {
                    document.reserve_child_task(ReserveChildTaskInput {
                        operation_id: CollaborationOperationId::try_new("queue-reserve")?,
                        source_tool_name: "spawn_agent".into(),
                        execution_id: original.clone(),
                        task_id: SubagentTaskId::try_new(child_id.to_string())?,
                        parent_turn_id: parent_turn,
                        child: CollaborationParticipant::new(
                            child_id.to_string(),
                            parent_id.to_string(),
                            storage.v1_storage_locator().project_id(),
                            1,
                            None,
                            Some(SubagentTaskId::try_new(child_id.to_string())?),
                        )?,
                        reserved_at_ms: 3,
                    })?;
                    document.activate_child_task(ActivateChildTaskInput {
                        operation_id: CollaborationOperationId::try_new("queue-activate")?,
                        execution_id: original.clone(),
                        task_id: SubagentTaskId::try_new(child_id.to_string())?,
                        active_at_ms: 4,
                    })
                })
            },
        )
        .unwrap();
    // A real sink commits the original completion without delivering its suppressed parent notification.
    // 真实接收器提交原始完成，不投递被抑制的父通知。
    let mut sink = ProviderV1StreamSink::new(
        child.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&child, &mut sink);
    sink.append_assistant_delta("original-result-marker", 5)
        .unwrap();
    sink.accept_terminal(terminal_facts(0)).await.unwrap();
    for execution in ["queue-middle", "queue-final"] {
        // Each queued message starts a distinct root, as the production inbox resumption does.
        // 每条排队消息开启不同根，与生产收件箱续跑一致。
        // Freeze the same admission facts used by the production start barrier.
        // 冻结生产启动屏障使用的相同准入事实。
        let facts = TaskV1ExecutionFacts {
            root_session_id: parent_id,
            parent_session_id: parent_id,
            task_id: child_id,
            execution_id: execution.into(),
            prompt: execution.into(),
            display_title: "queue worker".into(),
            is_resumed: true,
            admitted_at_ms: 20,
        };
        crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
            &bridge.session_runtime_registry,
            facts.clone(),
        )
        .unwrap();
        if terminal_kind.is_some() {
            // Rejection rolls back predecessor settlement and successor admission together.
            // 拒绝时一并回滚前驱结算及后继准入。
            let revision = parent.read(|document| document.durable_revision()).unwrap();
            assert!(
                crate::agent_stream::queued_admission::publish(
                    &parent,
                    &child,
                    &facts,
                    1,
                    parent_turn,
                    || Err(
                        vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                            field: "test.replaced_start_owner"
                        }
                    )
                )
                .is_err()
            );
            assert_eq!(
                parent.read(|document| document.durable_revision()).unwrap(),
                revision
            );
            // A changed source is rejected without settling the original obligation.
            // 来源改变时拒绝准入，不结算原始义务。
            let mut wrong_source = facts.clone();
            wrong_source.prompt.push_str("-changed");
            assert!(
                crate::agent_stream::queued_admission::publish(
                    &parent,
                    &child,
                    &wrong_source,
                    1,
                    parent_turn,
                    || Ok(())
                )
                .is_err()
            );
            assert_eq!(
                parent.read(|document| document.durable_revision()).unwrap(),
                revision
            );
            // An explicit stop rejects the whole handoff without releasing predecessor obligations.
            // 显式停止拒绝整个交接，不释放前驱义务。
            parent
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.set_collaboration_run_control(
                            vulcan_session_core::v1::collaboration::SetRunControlInput {
                                operation_id: CollaborationOperationId::try_new(format!(
                                    "queue-stop-{execution}"
                                ))?,
                                state: vulcan_session_core::v1::CollaborationRunControl::Stopped,
                                changed_at_ms: 20,
                            },
                        )
                    },
                )
                .unwrap();
            let before = parent
                .read(|document| {
                    (
                        document.durable_revision(),
                        document.collaboration_state().clone(),
                    )
                })
                .unwrap();
            assert!(
                crate::agent_stream::queued_admission::publish(
                    &parent,
                    &child,
                    &facts,
                    1,
                    parent_turn,
                    || Ok(())
                )
                .is_err()
            );
            assert_eq!(
                parent
                    .read(|document| (
                        document.durable_revision(),
                        document.collaboration_state().clone()
                    ))
                    .unwrap(),
                before
            );
            parent
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.set_collaboration_run_control(
                            vulcan_session_core::v1::collaboration::SetRunControlInput {
                                operation_id: CollaborationOperationId::try_new(format!(
                                    "queue-run-{execution}"
                                ))?,
                                state: vulcan_session_core::v1::CollaborationRunControl::Running,
                                changed_at_ms: 20,
                            },
                        )
                    },
                )
                .unwrap();
            crate::agent_stream::queued_admission::publish(
                &parent,
                &child,
                &facts,
                1,
                parent_turn,
                || Ok(()),
            )
            .unwrap();
            // A repeated start keeps one reservation and one original result.
            // 重复启动只保留一份预留及原始结果。
            let revision = parent.read(|document| document.durable_revision()).unwrap();
            crate::agent_stream::queued_admission::publish(
                &parent,
                &child,
                &facts,
                1,
                parent_turn,
                || Ok(()),
            )
            .unwrap();
            assert_eq!(
                parent.read(|document| document.durable_revision()).unwrap(),
                revision
            );
        }
        if terminal_kind.is_some() && (execution != "queue-final" || !startup_abort) {
            // Simulated provider preparation crosses the same Reserved-to-Active domain boundary.
            // 模拟供应商准备跨越相同的 Reserved 到 Active 领域边界。
            parent
                .mutate_durable(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document.activate_child_task(ActivateChildTaskInput {
                            operation_id: CollaborationOperationId::try_new(format!(
                                "queued-child-activate:{execution}"
                            ))?,
                            execution_id: CollaborationExecutionId::try_new(execution)?,
                            task_id: SubagentTaskId::try_new(child_id.to_string())?,
                            active_at_ms: 20,
                        })
                    },
                )
                .unwrap();
        }
        sink = ProviderV1StreamSink::new(
            child.clone(),
            Arc::new(ProjectResourceCatalogRegistry::default()),
        );
        prepare_sink_request(&child, &mut sink);
        if execution != "queue-final" || !non_success {
            sink.append_assistant_delta(execution, 21).unwrap();
            sink.accept_terminal(terminal_facts(0)).await.unwrap();
        } else if !startup_abort
            && terminal_kind == Some(vulcan_session_boundary::SubagentMailboxEventKind::Failed)
        {
            // Real failed API facts precede the host failure notification.
            // 真实失败 API 事实先于宿主失败通知。
            let mut terminal = terminal_facts(0);
            terminal.status = ApiCallStatus::Failed;
            terminal.end_reason = Some(ApiEndReason::Unknown);
            terminal.error = Some(RedactedApiError {
                category: ApiErrorCategory::Unknown,
                error_code: Some("queue_failure".into()),
                message: "controlled queued failure".into(),
            });
            terminal.provider_source = None;
            sink.accept_terminal(terminal).await.unwrap();
        }
        if execution == "queue-middle" && published_successor {
            // Reproduce the previously committed broad range before the original reservation is settled.
            // 在原预留结算之前，复现先前已提交的宽范围。
            let text = sink.completed_assistant_text().unwrap().clone();
            // Build opaque evidence from the exact completed child source.
            // 从精确已完成子来源构造不透明凭据。
            let input = child
                .read_full_durable(|document| {
                    document.build_subagent_return_input_since(
                        SubagentReturnBuildRequest {
                            final_turn_id: document
                                .events()
                                .iter()
                                .find(|event| event.seq == text.seq)
                                .unwrap()
                                .turn_id
                                .unwrap(),
                            child_task_id: SubagentTaskId::try_new(child_id.to_string()).unwrap(),
                            source_assistant_text_seq: text.seq,
                        },
                        None,
                    )
                })
                .unwrap()
                .unwrap();
            parent
                .append_execution_subagent_return(
                    CollaborationExecutionId::try_new(execution).unwrap(),
                    input,
                    22,
                )
                .unwrap();
        }
    }
    assert_eq!(
        parent
            .read(|document| document.collaboration_state().child_tasks[&original].state)
            .unwrap(),
        if terminal_kind.is_some() {
            CollaborationChildTaskState::Terminal
        } else {
            CollaborationChildTaskState::Active
        }
    );
    // Freeze existing provider-visible returns to verify recovery never edits a previously delivered answer.
    // 冻结既有供应商可见回流，验证恢复绝不编辑先前已投递回答。
    let previous = parent
        .read(|document| {
            document
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventBody::SubagentReturn(_)))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap();
    // Runtime feedback remains offline and is published only after durable results.
    // 运行时反馈保持离线，且只在可靠结果之后发布。
    let runtime = crate::tests::test_support::test_agent_runtime_bridge();
    if delivery != QueuedDelivery::ProcessExit {
        runtime
            .task_runtime
            .expect_durable_task_return(child_id, "queue-final")
            .unwrap();
    }
    bridge.agent_runtime = Some(runtime.clone());
    if delivery != QueuedDelivery::ProcessExit {
        bridge
            .detached_subagent_task_listeners
            .insert(child_id, "queue-final".into());
    }
    // Suppress automatic Provider dispatch while still exercising every durable terminal transition.
    // 抑制自动供应商分发，同时执行全部可靠终态转换。
    bridge
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_run_phases
        .insert(parent_id, crate::SessionRunPhase::Stopped);
    // Shared ownership mirrors the running service without touching user sessions.
    // 共享归属镜像运行服务，不接触用户会话。
    if stop_parent {
        bridge
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .session_run_phases
            .remove(&parent_id);
    }
    let state = Arc::new(std::sync::Mutex::new(bridge));
    // The final event is identical to the detached-runtime completed feedback shape.
    // 最终事件与分离运行时已完成反馈形状相同。
    let mailbox = vulcan_session_boundary::SubagentMailboxEvent {
        parent_thread_id: parent_id.into(),
        parent_turn_id: None,
        child_thread_id: child_id.into(),
        kind: terminal_kind.unwrap_or(vulcan_session_boundary::SubagentMailboxEventKind::Completed),
        timestamp_ms: 30,
        payload: json!({"status": match terminal_kind { Some(vulcan_session_boundary::SubagentMailboxEventKind::Failed) => "failed", Some(vulcan_session_boundary::SubagentMailboxEventKind::Stopped) => "stopped", _ => "completed" }, "execution_id":"queue-final", "result_text":"queue-final", "error_text":"controlled queued failure"}),
    };
    // Exact task and execution metadata are required by the production parser.
    // 生产解析器要求精确任务及执行元数据。
    let event = DetachedTaskRuntimeEvent::ParentRuntime { parent_session_id: parent_id,
        event: RuntimeEvent::new(RuntimeEventKind::DelegateFinished, "Completed", "queue-final").with_metadata(Some(json!({"kind":"subagent_task", "payload":{"subagent_task_id":child_id.to_string(), "execution_id":"queue-final", "subagent_mailbox_event":mailbox}}))) };
    // The real event adapter exercises predecessor recovery, current return, feedback and listener release.
    // 真实事件适配器执行前驱恢复、当前回流、反馈及监听器释放。
    let app = crate::ServiceEventEmitter::new(vec![]);
    // Telemetry uses the same source identity while all model work remains synthetic.
    // 遥测使用相同来源身份，全部模型工作保持合成。
    let mut telemetry =
        crate::agent_stream::ProviderTelemetryHookGuard::new_with_persistence(state.clone(), None);
    if replace_listener || stop_parent {
        // Hold the actual lifecycle gate so the terminal yields exactly inside predecessor recovery.
        // 持有实际生命周期锁，使终态恰好在前驱恢复内部让出执行。
        let registry = state.lock().unwrap().session_runtime_registry.clone();
        // Explicit ownership removes timing and sleeps from this race regression.
        // 显式归属使该竞态回归不依赖时序及休眠。
        let gate = registry
            .acquire_lifecycle_transition(parent_id)
            .await
            .unwrap();
        // Poll the production event adapter until it waits on the held canonical gate.
        // 轮询生产事件适配器，直到它等待被持有的规范锁。
        let handling = crate::agent_stream::handle_webui_detached_subagent_task_event(
            &app,
            &state,
            &controller,
            &runtime,
            child_id,
            "queue-final",
            &child,
            &mut telemetry,
            &mut sink,
            event,
        );
        tokio::pin!(handling);
        assert!(futures_util::poll!(&mut handling).is_pending());
        if replace_listener {
            state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .insert(child_id, "queue-newer".into());
        }
        if stop_parent {
            state
                .lock()
                .unwrap()
                .workspace_snapshot
                .as_mut()
                .unwrap()
                .session_run_phases
                .insert(parent_id, crate::SessionRunPhase::Stopped);
        }
        drop(gate);
        handling.await.unwrap();
        if stop_parent {
            assert!(state.lock().unwrap().active_stream_tasks.is_empty());
            assert!(
                !state
                    .lock()
                    .unwrap()
                    .detached_subagent_task_listeners
                    .contains_key(&child_id)
            );
            assert!(
                parent
                    .read(|document| document.pending_subagent_return_dispatch())
                    .unwrap()
                    .unwrap()
                    .is_some()
            );
            return;
        }
        assert_eq!(
            state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .get(&child_id)
                .map(String::as_str),
            Some("queue-newer")
        );
        assert!(
            runtime
                .task_runtime
                .take_subagent_events_for_parent(parent_id.into())
                .unwrap()
                .is_empty()
        );
        assert!(!parent.read(|document| document.events().iter().any(|event|
            matches!(&event.event, EventBody::SubagentReturn(returned) if returned.child_root_action_id.as_str() == "queue-final"))).unwrap());
        return;
    }
    if delivery == QueuedDelivery::ProcessExit {
        // Cold storage retains the exact reserved successor while no Provider execution can resume.
        // 冷存储保留精确预留后继，同时不存在可续跑的供应商执行。
        let execution = CollaborationExecutionId::try_new("queue-final").unwrap();
        let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(
            &storage.v1_storage_locator(),
        )
        .unwrap();
        let record = loaded.document.collaboration_state().child_tasks[&execution].clone();
        assert_eq!(record.state, CollaborationChildTaskState::Reserved);
        assert!(
            crate::agent_stream::queued_admission::recover_unprepared(
                &state, &parent, &child, &record
            )
            .unwrap()
        );
        let terminal_record = parent
            .read(|document| document.collaboration_state().child_tasks[&execution].clone())
            .unwrap();
        assert!(
            !crate::agent_stream::queued_admission::recover_unprepared(
                &state,
                &parent,
                &child,
                &terminal_record
            )
            .unwrap()
        );
    } else if startup_abort {
        // A cancelled start waits for the real admission gate before deciding whether a reservation exists.
        // 取消启动在判定预留是否存在前等待真实准入锁。
        let registry = state.lock().unwrap().session_runtime_registry.clone();
        let gate = registry
            .acquire_lifecycle_transition(parent_id)
            .await
            .unwrap();
        let owner =
            crate::agent_stream::startup_rejection::capture_owner(&child, "queue-final").unwrap();
        let DetachedTaskRuntimeEvent::ParentRuntime {
            event: ref aborted, ..
        } = event
        else {
            unreachable!()
        };
        let settling = crate::agent_stream::queued_admission::settle_startup_abort(
            &app,
            &state,
            &runtime,
            parent_id,
            child_id,
            "queue-final",
            &child,
            owner,
            aborted,
        );
        tokio::pin!(settling);
        assert!(futures_util::poll!(&mut settling).is_pending());
        assert!(
            parent
                .read(|document| document.collaboration_state().child_tasks
                    [&CollaborationExecutionId::try_new("queue-final").unwrap()]
                    .non_success_result
                    .is_none())
                .unwrap()
        );
        drop(gate);
        settling.await.unwrap();
    } else {
        crate::agent_stream::handle_webui_detached_subagent_task_event(
            &app,
            &state,
            &controller,
            &runtime,
            child_id,
            "queue-final",
            &child,
            &mut telemetry,
            &mut sink,
            event.clone(),
        )
        .await
        .unwrap();
    }
    parent
        .read(|document| {
            assert_eq!(
                document.collaboration_state().child_tasks[&original].state,
                CollaborationChildTaskState::Terminal
            );
            assert!(
                document
                    .completion_obligations(parent_turn)
                    .open_child_task_ids
                    .is_empty()
            );
            for old in &previous {
                assert_eq!(
                    document.events().iter().find(|event| event.seq == old.seq),
                    Some(old)
                );
            }
            // Both distinct source contents must reach the parent; a successor cannot stand in for its original.
            // 两份不同来源正文都必须到达父会话；后继不能替代原始结果。
            let returns = document
                .events()
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::SubagentReturn(returned) => Some(returned),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                returns.len(),
                if published_successor
                    || terminal_kind
                        == Some(vulcan_session_boundary::SubagentMailboxEventKind::Completed)
                {
                    3
                } else {
                    2
                }
            );
            assert!(
                returns
                    .iter()
                    .any(
                        |returned| returned.child_root_action_id.as_str() == original.as_str()
                            && returned.content == "original-result-marker"
                    )
            );
            if non_success {
                let kind = terminal_kind.unwrap();
                // The failed/stopped successor has its own exact receipt, distinct from successful predecessors.
                // 失败或停止的后继拥有独立精确回执，与成功前驱区分。
                let record = &document.collaboration_state().child_tasks
                    [&CollaborationExecutionId::try_new("queue-final").unwrap()];
                assert_eq!(record.state, CollaborationChildTaskState::Terminal);
                assert_eq!(
                    record.terminal_outcome,
                    Some(
                        if kind == vulcan_session_boundary::SubagentMailboxEventKind::Failed {
                            vulcan_session_core::v1::CollaborationChildTaskTerminalOutcome::Failed
                        } else {
                            vulcan_session_core::v1::CollaborationChildTaskTerminalOutcome::Stopped
                        }
                    )
                );
                assert!(record.non_success_result.is_some());
                assert!(
                    !returns
                        .iter()
                        .any(|returned| returned.child_root_action_id.as_str() == "queue-final")
                );
            } else {
                assert!(
                    returns
                        .iter()
                        .any(
                            |returned| returned.child_root_action_id.as_str() == "queue-final"
                                && returned.content == "queue-final"
                        )
                );
            }
        })
        .unwrap();
    assert!(
        !state
            .lock()
            .unwrap()
            .detached_subagent_task_listeners
            .contains_key(&child_id)
    );
    // Duplicate terminal delivery cannot repeat durable results or mailbox feedback.
    // 重复终态投递不能重复可靠结果或邮箱反馈。
    let revision = parent.read(|document| document.durable_revision()).unwrap();
    crate::agent_stream::handle_webui_detached_subagent_task_event(
        &app,
        &state,
        &controller,
        &runtime,
        child_id,
        "queue-final",
        &child,
        &mut telemetry,
        &mut sink,
        event,
    )
    .await
    .unwrap();
    assert_eq!(
        parent.read(|document| document.durable_revision()).unwrap(),
        revision
    );
    assert_eq!(
        runtime
            .task_runtime
            .take_subagent_events_for_parent(parent_id.into())
            .unwrap(),
        if delivery == QueuedDelivery::ProcessExit {
            vec![]
        } else {
            vec![mailbox]
        }
    );
    // The parent's next physical answer consumes all returns and closes the logical turn normally.
    // 父会话下一个物理回答消费全部回流并正常关闭逻辑回合。
    crate::agent_collaboration::project_collaboration_before_api(&state, &parent, 39)
        .await
        .unwrap();
    let mut parent_sink = ProviderV1StreamSink::new(
        parent.clone(),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    prepare_sink_request(&parent, &mut parent_sink);
    parent_sink
        .append_assistant_delta("parent final", 40)
        .unwrap();
    parent_sink
        .accept_terminal(terminal_facts(0))
        .await
        .unwrap();
    assert!(
        parent_sink
            .take_finished_durability()
            .turn_completion
            .is_some()
    );
    // Strict cold loading verifies events and collaboration receipts agree after every transition.
    // 严格冷加载验证每次转换后事件及协作回执一致。
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(
        &storage.v1_storage_locator(),
    )
    .unwrap();
    assert!(loaded.document.active_turn().is_none_or(|turn| turn.closed));
    assert!(loaded.document.metadata.open_turn_start_seq.is_none());
}

/// Consecutive queued turns cannot strand the original spawn after the final answer.
/// 连续排队回合不能在最终回答后遗留原派生任务。
#[tokio::test]
async fn queued_child_successor_settles_original_before_final_publication() {
    queued_child_chain(false, false, false, None, QueuedDelivery::Live).await;
}

/// Already published overlapping prefixes recover through the real final-event adapter without history edits.
/// 已发布重叠前缀通过真实最终事件适配器恢复，且不编辑历史。
#[tokio::test]
async fn queued_child_successor_repairs_existing_overlap_and_parent_completion() {
    queued_child_chain(true, false, false, None, QueuedDelivery::Live).await;
}

/// A listener replaced during recovery must not publish stale feedback or remove its successor.
/// 恢复期间被替换的监听器不得发布过期反馈或移除其后继。
#[tokio::test]
async fn queued_child_successor_rechecks_listener_after_recovery_yields() {
    queued_child_chain(false, true, false, None, QueuedDelivery::Live).await;
}

/// A stop observed while recovery waits must preserve returns without starting another parent request.
/// 恢复等待期间观测到的停止必须保留回流，但不得启动新的父请求。
/// Uses the real lifecycle gate rather than timing delays; returns no value.
/// 使用真实生命周期锁而非时序延迟；不返回值。
#[tokio::test]
async fn queued_child_successor_preserves_parent_stop_during_recovery() {
    queued_child_chain(false, false, true, None, QueuedDelivery::Live).await;
}

/// A queued Provider failure releases its own reservation and reaches the parent exactly once.
/// 排队供应商失败释放自身预留，并且只向父会话回流一次。
#[tokio::test]
async fn queued_child_successor_failure_publishes_durable_result() {
    queued_child_chain(
        false,
        false,
        false,
        Some(vulcan_session_boundary::SubagentMailboxEventKind::Failed),
        QueuedDelivery::Live,
    )
    .await;
}

/// A queued stop seals its exact turn without treating it as successful output.
/// 排队停止封闭其精确回合，不将其视为成功输出。
#[tokio::test]
async fn queued_child_successor_stop_publishes_durable_result() {
    queued_child_chain(
        false,
        false,
        false,
        Some(vulcan_session_boundary::SubagentMailboxEventKind::Stopped),
        QueuedDelivery::Live,
    )
    .await;
}

/// Normal successful queues retain complete results after every execution gains durable ownership.
/// 每次执行取得可靠归属后，正常成功队列仍保留完整结果。
#[tokio::test]
async fn queued_child_successor_success_retains_all_owned_results() {
    queued_child_chain(
        false,
        false,
        false,
        Some(vulcan_session_boundary::SubagentMailboxEventKind::Completed),
        QueuedDelivery::Live,
    )
    .await;
}

/// Startup abort cannot race ahead of durable admission and lose its accepted reservation.
/// 启动中止不能抢在可靠准入之前处理而丢失已接纳预留。
#[tokio::test]
async fn queued_child_successor_startup_abort_waits_for_admission() {
    queued_child_chain(
        false,
        false,
        false,
        Some(vulcan_session_boundary::SubagentMailboxEventKind::Failed),
        QueuedDelivery::StartupAbort,
    )
    .await;
}

/// A process exit before profile capture turns the reserved successor into a durable failure without replay.
/// 配置捕获前进程退出将预留后继转换为可靠失败，不重放执行。
#[tokio::test]
async fn queued_child_successor_unprepared_process_exit_is_recoverable() {
    queued_child_chain(
        false,
        false,
        false,
        Some(vulcan_session_boundary::SubagentMailboxEventKind::Failed),
        QueuedDelivery::ProcessExit,
    )
    .await;
}
