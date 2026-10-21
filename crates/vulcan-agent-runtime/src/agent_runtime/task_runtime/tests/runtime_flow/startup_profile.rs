//! Production startup ordering tests for durable child preparation.
//! 可靠子准备的生产启动顺序验证。

use super::*;
use crate::tooling::collaboration::{AgentCollaborationCommand, NativeAgentCollaborationRuntime};
use collaboration_lifecycle::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskState, CollaborationExecutionId, CollaborationOperationId,
};
use vulcan_session_core::v1::{
    ApiCallId, RequestStreamId, Seq, SubagentTaskId, ToolCallId, TurnId,
};
use vulcan_tools_manager::VulcanToolInvocationContext;

/// Startup subscription and abort are mutually ordered, so no cancellation is mistaken for an empty live stream.
/// 启动订阅与中止互相有序，取消不会被误认为空的活动流。
/// Returns nothing; both cancellation orders use real runtime streams without Provider execution.
/// 不返回值；两种取消顺序均使用真实运行时事件流，不执行 Provider。
#[tokio::test]
async fn active_startup_subscription_rejects_cancelled_owner_and_delivers_later_abort() {
    let runtime = Arc::new(TaskRuntime::new());
    let parent = Uuid::new_v4();
    let request = build_task_start_test_invocation(parent, Uuid::new_v4(), "strict-startup-stream");
    runtime
        .open_detached_task_event_stream_for_run(request.task_id, &request.execution_id)
        .unwrap();
    runtime
        .abort_detached_task_startup(&request, "cancel before admission")
        .unwrap();
    assert!(
        runtime
            .subscribe_active_detached_task_events(request.task_id, &request.execution_id)
            .is_err()
    );
    runtime
        .open_detached_task_event_stream_for_run(request.task_id, &request.execution_id)
        .unwrap();
    let stream = runtime
        .subscribe_active_detached_task_events(request.task_id, &request.execution_id)
        .unwrap();
    futures_util::pin_mut!(stream);
    runtime
        .abort_detached_task_startup(&request, "cancel after admission")
        .unwrap();
    assert!(
        matches!(stream.next().await, Some(DetachedTaskRuntimeEvent::StartupAborted { parent_session_id, .. }) if parent_session_id == parent)
    );
    assert!(stream.next().await.is_none());
    assert!(
        runtime
            .subscribe_active_detached_task_events(request.task_id, &request.execution_id)
            .is_err()
    );
}

/// Accepts reservation while observing every forbidden transition after failed preparation.
/// 接受预留，同时观测准备失败后不应发生的所有转换。
#[derive(Debug, Default)]
struct PreparationAdmissionHost {
    /// Counts initial host admissions before any display start event.
    /// 统计任何展示启动事件之前的初始宿主准入。
    created: AtomicUsize,
    /// Retains the actual resolved snapshot received by the initial creation port.
    /// 保留初始创建端口收到的真实已解析快照。
    creation: Mutex<Option<ChildExecutionProfileCaptureRequest>>,
    /// Counts acknowledged reservations before request validation.
    /// 统计请求校验前已确认的预留。
    reserved: AtomicUsize,
    /// Counts profile writes; the invalid agent must fail before this boundary.
    /// 统计配置写入；无效代理必须在此边界前失败。
    captured: AtomicUsize,
    /// Counts activation attempts, which must remain zero on admission rejection.
    /// 统计激活尝试，准入拒绝时必须保持零。
    activated: AtomicUsize,
    /// Captures the exact native compaction invocation after route resolution.
    /// 捕获路由解析后的精确原生压缩调用。
    compaction: Mutex<Option<TaskExecutionRequest>>,
    /// Records the caller-selected phase without deriving it from runtime activity.
    /// 记录调用方选择的阶段，不从运行时活动状态推导。
    compaction_phase:
        Mutex<Option<crate::agent_runtime::prompt_compaction::ChildTaskCompactionPhase>>,
    /// Signals exact host arrival so background maintenance tests never infer completion from timing.
    /// 通知精确宿主到达，使后台维护测试不按时间推测完成。
    compaction_arrived: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl NativeAgentCollaborationRuntime for PreparationAdmissionHost {
    /// Records native compaction controls and returns a distinct host failure without provider execution.
    /// 记录原生压缩控制，返回独立宿主错误而不执行供应商。
    async fn compact_child_task(
        &self,
        request: crate::agent_runtime::prompt_compaction::ChildTaskCompactionRequest,
    ) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
        assert!(request.preparation.conversation_state.messages.is_empty());
        assert_eq!(request.preparation.session_id, request.invocation.task_id);
        assert_eq!(
            request.preparation.runtime_policy,
            vulcan_session_core::ExecutionRuntimePolicy::task_session()
        );
        *self.compaction.lock().unwrap() = Some(request.invocation);
        *self.compaction_phase.lock().unwrap() = Some(request.phase);
        self.compaction_arrived.notify_one();
        Err(String::from("Unexpected child compaction in test host."))
    }
    /// Captures real preparation and rejects before file admission or Provider dispatch.
    /// 捕获真实准备，并在文件准入或 Provider 分发之前拒绝。
    async fn create_child_task(
        &self,
        _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskReservationRequest,
        preparation: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<
        crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskLifecycleReceipt,
        String,
    > {
        self.created.fetch_add(1, Ordering::SeqCst);
        *self.creation.lock().unwrap() = Some(preparation);
        Err(String::from("Initial creation rejected by test host."))
    }
    /// Records unexpected capture and returns an explicit boundary error.
    /// 记录非预期捕获并返回明确边界错误。
    async fn capture_child_execution_profile(
        &self,
        _: ChildExecutionProfileCaptureRequest,
    ) -> Result<(), String> {
        self.captured.fetch_add(1, Ordering::SeqCst);
        Err("Unexpected capture after invalid preparation.".into())
    }

    /// Rejects model commands; startup only uses host lifecycle operations.
    /// 拒绝模型命令；启动只使用宿主生命周期操作。
    async fn invoke(
        &self,
        _: VulcanToolInvocationContext,
        _: AgentCollaborationCommand,
    ) -> Result<Value, String> {
        Err("Unexpected model command.".into())
    }

    /// Rejects question waiting because no model execution has started.
    /// 拒绝问题等待，因为模型执行尚未开始。
    async fn wait_for_incoming_question(
        &self,
        _: VulcanToolInvocationContext,
    ) -> Result<Value, String> {
        Err("Unexpected question wait.".into())
    }

    /// Acknowledges the exact supplied reservation without starting a provider.
    /// 确认精确传入的预留，不启动供应商。
    async fn reserve_child_task(
        &self,
        request: ChildTaskReservationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        self.reserved.fetch_add(1, Ordering::SeqCst);
        Ok(ChildTaskLifecycleReceipt {
            execution_id: CollaborationExecutionId::try_new(request.execution.execution_id)
                .unwrap(),
            operation_id: request.operation_id,
            task_id: request.execution.task_id,
            state: CollaborationChildTaskState::Reserved,
            parent_turn_id: TurnId::try_new(1).unwrap(),
            document_revision: 1,
            changed: true,
        })
    }

    /// Records an invalid early activation and rejects it before any background execution.
    /// 记录非法提前激活，在任何后台执行之前拒绝。
    async fn activate_child_task(
        &self,
        _: ChildTaskActivationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        self.activated.fetch_add(1, Ordering::SeqCst);
        Err("Activation preceded durable preparation.".into())
    }

    /// Rejects direct terminal mutation; the real startup guard owns abort publication.
    /// 拒绝直接终态修改；真实启动守卫负责中止发布。
    async fn terminal_child_task(
        &self,
        _: ChildTaskTerminalRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String> {
        Err("Unexpected direct terminal mutation.".into())
    }
}

/// Native compaction delegates even with empty resident history and propagates the host result.
/// 即使驻留历史为空，原生压缩仍委托宿主，并传递宿主结果。
/// Returns nothing; compares ordinary execution history with control-only compaction preparation.
/// 不返回值；对比普通执行历史与仅携带控制的压缩准备。
#[test]
fn native_compaction_uses_host_history_and_preserves_execution_identity() {
    // The compaction path polls the full agent stream, so it gets the same stack production reserves.
    // 压缩路径会轮询完整的智能体流，因此使用与生产相同的预留栈。
    std::thread::Builder::new()
        .name(String::from("native-compaction-identity"))
        .stack_size(crate::AGENT_STREAM_WORKER_STACK_BYTES)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("native compaction test runtime")
                .block_on(
                    native_compaction_uses_host_history_and_preserves_execution_identity_body(),
                )
        })
        .expect("spawn native compaction test thread")
        .join()
        .expect("native compaction test thread completed");
}

/// Body of `native_compaction_uses_host_history_and_preserves_execution_identity`.
/// `native_compaction_uses_host_history_and_preserves_execution_identity` 的测试主体。
async fn native_compaction_uses_host_history_and_preserves_execution_identity_body() {
    for resident_history_present in [false, true] {
        // The host owns durable history independently from the resident snapshot.
        // 宿主独立于驻留快照持有可靠历史。
        let parent = Uuid::new_v4();
        // Each case owns a distinct task and immutable execution identity.
        // 每种情况持有独立任务及不可变执行身份。
        let child = Uuid::new_v4();
        // This host observes dispatch without invoking a network provider.
        // 此宿主观测派发而不调用网络供应商。
        let host = Arc::new(PreparationAdmissionHost::default());
        // Use the actual installed test route to exercise model resolution before host admission.
        // 使用真实安装的测试路由，执行宿主准入前的模型解析。
        let mut context = build_task_start_test_runtime_context(parent);
        context.foundation = Arc::new(
            crate::agent_runtime::execution_preparation::tests::test_runtime_foundation(
                &context.config,
            ),
        );
        context.request_template.execution_agent_id = Some(
            context
                .foundation
                .active_agent_definition()
                .unwrap()
                .id
                .clone(),
        );
        context.request_template.provider_id_override = Some("openrouter".into());
        context.request_template.model_id_override = Some("openai/gpt-5.6-sol".into());
        context.request_template.connection_id_override = Some("test-openrouter".into());
        context.request_template.native_agent_collaboration_runtime = Some(host.clone());
        // A nonempty stale resident message must remain ordinary history but never become host summary input.
        // 非空旧驻留消息应保留为普通历史，但绝不能变成宿主摘要输入。
        let mut invocation = build_task_start_test_invocation(parent, child, "host-compaction");
        if resident_history_present {
            invocation
                .snapshot
                .messages
                .push(vulcan_core::types::ConversationMessage {
                    id: "resident-history".into(),
                    payload: vulcan_core::types::ConversationPayload::UserText {
                        text: "Resident history is not durable compaction authority.".into(),
                        attachments: Vec::new(),
                    },
                });
        }
        // Resolve the same task policy consumed by the production compaction entry.
        // 解析生产压缩入口消费的同一任务策略。
        let mut resolved = test_resolved_config(
            &build_test_controller_config("Base prompt"),
            &invocation.invocation_config,
        );
        // An explicit provider owns the whole model id, including the model's own slash.
        // 显式供应商持有完整模型标识，包括模型自身的斜杠。
        resolved.effective_config.provider_model.provider_id = Some("openrouter".into());
        resolved.effective_config.provider_model.model_id = Some("openai/gpt-5.6-sol".into());
        // Ordinary preparation still converts the supplied execution snapshot.
        // 普通准备仍转换传入的执行快照。
        let (_, ordinary) =
            build_task_runtime_preparation_request(&context, &invocation, &resolved).unwrap();
        assert_eq!(
            ordinary.conversation_state.messages.len(),
            usize::from(resident_history_present)
        );
        // Empty history used to bypass compaction before consulting the durable host.
        // 空历史此前会在询问可靠宿主之前绕过压缩。
        let error = run_task_session_context_compaction(
            Arc::new(TaskRuntime::new()),
            context.clone(),
            invocation.clone(),
            resolved.clone(),
            crate::agent_runtime::prompt_compaction::ChildTaskCompactionPhase::ActiveBusiness,
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Unexpected child compaction in test host.");
        // All original ownership coordinates must survive routing unchanged.
        // 所有原始归属坐标必须在路由后保持不变。
        let captured = host.compaction.lock().unwrap().clone().unwrap();
        assert_eq!(captured.task_id, child);
        assert_eq!(captured.current_session_id, parent);
        assert_eq!(captured.root_session_id, parent);
        assert_eq!(captured.execution_id, "host-compaction");
        assert_eq!(
            *host.compaction_phase.lock().unwrap(),
            Some(crate::agent_runtime::prompt_compaction::ChildTaskCompactionPhase::ActiveBusiness)
        );
        assert_eq!(captured.prompt, "review this change");
        assert_eq!(
            captured.snapshot.messages.len(),
            usize::from(resident_history_present)
        );
        // Post-completion dispatch must declare maintenance and use the final outcome, not startup history.
        // 完成后派发必须声明维护，并使用最终结果而非启动历史。
        let maintenance_host = Arc::new(PreparationAdmissionHost::default());
        context.request_template.native_agent_collaboration_runtime =
            Some(maintenance_host.clone());
        resolved
            .effective_config
            .context_compaction
            .auto_compaction_enabled = true;
        // Distinct terminal history makes accidental reuse of either resident case observable.
        // 独立终态历史使误用任一驻留情况都可被观察。
        let terminal_snapshot = CoreConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: "terminal-history".into(),
                payload: vulcan_core::types::ConversationPayload::UserText {
                    text: "Final committed history".into(),
                    attachments: Vec::new(),
                },
            }],
            ..Default::default()
        };
        // The native host receives maintenance immediately so it can own publication waiting and cancellation.
        // 原生宿主立即接收维护，以便持有发布等待与取消。
        let maintenance_runtime = Arc::new(TaskRuntime::new());
        maintenance_runtime
            .expect_durable_task_return(child, &invocation.execution_id)
            .unwrap();
        super::super::super::runtime_execution::spawn_task_session_context_compaction(
            Arc::clone(&maintenance_runtime),
            context,
            invocation,
            TaskExecutionOutcome {
                task_id: child,
                controller_config: build_test_controller_config("Final controller"),
                resolved_config: resolved,
                snapshot: terminal_snapshot,
                provider_turns_executed: 1,
                tool_calls_processed: 0,
                final_assistant_message_id: None,
                final_output_text: "Complete".into(),
                execution_status: TaskExecutionStatus::Completed,
                usage_segments: Vec::new(),
            },
        );
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            maintenance_host.compaction_arrived.notified(),
        )
        .await
        .unwrap();
        assert_eq!(
            *maintenance_host.compaction_phase.lock().unwrap(),
            Some(crate::agent_runtime::prompt_compaction::ChildTaskCompactionPhase::CompletedBusinessMaintenance)
        );
        // The same stable business identity survives while its final payload replaces the launch snapshot.
        // 稳定业务身份保持不变，而最终载荷替换启动快照。
        let maintenance = maintenance_host.compaction.lock().unwrap().clone().unwrap();
        assert_eq!(maintenance.execution_id, "host-compaction");
        assert_eq!(maintenance.snapshot.messages[0].id, "terminal-history");
        assert_eq!(
            maintenance.controller_config.unwrap().system_prompt,
            "Final controller"
        );
    }
}

/// Missing capture authority or failed preparation cannot activate an already reserved child.
/// 缺少捕获权威或准备失败不能激活已经预留的子任务。
/// Returns nothing; exercises the production entry, projection acknowledgement and startup cleanup.
/// 不返回值；执行生产入口、投影确认及启动清理。
#[tokio::test]
async fn durable_child_preparation_failure_prevents_activation() {
    for (capture_host_present, valid_preparation, is_resumed) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, true, true),
    ] {
        // Each failure owns an independent runtime and exact execution binding.
        // 每种失败持有独立运行时和精确执行绑定。
        let runtime = Arc::new(TaskRuntime::new());
        let parent = Uuid::new_v4();
        let child = Uuid::new_v4();
        let host = Arc::new(PreparationAdmissionHost::default());
        let mut context = build_task_start_test_runtime_context(parent);
        if valid_preparation {
            context.foundation = Arc::new(
                crate::agent_runtime::execution_preparation::tests::test_runtime_foundation(
                    &context.config,
                ),
            );
            // Use the fixture's actual built agent identity, not the invalid placeholder from the rejection fixture.
            // 使用夹具实际构建的代理身份，不沿用拒绝夹具中的无效占位身份。
            context.request_template.execution_agent_id = Some(
                context
                    .foundation
                    .active_agent_definition()
                    .unwrap()
                    .id
                    .clone(),
            );
            // Match the explicit route installed by the in-memory fixture, as the real host request does.
            // 与内存夹具安装的显式路由一致，和真实宿主请求相同。
            context.request_template.provider_id_override = Some("openrouter".into());
            context.request_template.model_id_override = Some("openai/gpt-5.6-sol".into());
            context.request_template.connection_id_override = Some("test-openrouter".into());
        } else {
            context.request_template.execution_agent_id = Some("missing-admission-agent".into());
        }
        if capture_host_present {
            context.request_template.native_agent_collaboration_runtime = Some(host.clone());
        }
        let mut invocation =
            build_task_start_test_invocation(parent, child, "preparation-rejection");
        invocation.is_resumed = is_resumed;
        runtime
            .register_child_task_lifecycle_binding(ChildTaskLifecycleBinding {
                port: host.clone(),
                reservation: ChildTaskReservationRequest {
                    invocation: VulcanToolInvocationContext {
                        turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1)
                            .expect("synthetic turn"),
                        session_id: parent.to_string(),
                        request_stream_id: RequestStreamId::try_new("admission-stream").unwrap(),
                        api_call_id: ApiCallId::try_new(1).unwrap(),
                        tool_call_id: ToolCallId::try_new(1).unwrap(),
                        tool_seq: Seq::try_new(3).unwrap(),
                        controller_tool_call_id: "admission-call".into(),
                        provider_tool_call_id: None,
                    },
                    operation_id: CollaborationOperationId::try_new("admission-reservation")
                        .unwrap(),
                    execution: ChildTaskExecutionIdentity {
                        task_session_id: child,
                        execution_id: invocation.execution_id.clone(),
                        task_id: SubagentTaskId::try_new(child.to_string()).unwrap(),
                    },
                    reserved_at_ms: 1,
                },
            })
            .unwrap();
        // Drain and acknowledge actual start events, including an enabled host projection barrier.
        // 消费并确认真实启动事件，包括可能启用的宿主投影屏障。
        let (sender, mut receiver) = unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let event_reader = tokio::spawn(async move {
            // Failed preparation and rejected creation must not publish a misleading start event.
            // 准备失败及创建被拒绝时，不得发布误导性的启动事件。
            let mut starts = 0;
            while let Some(event) = receiver.recv().await {
                if let Ok(ProviderStreamDelta::RuntimeEvent(event)) = event {
                    if event.kind == RuntimeEventKind::DelegateStarted {
                        starts += 1;
                    }
                    crate::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(&event).unwrap();
                }
            }
            starts
        });
        let error = Box::pin(run_task_with_shared_runtime_context(
            runtime.clone(),
            context,
            build_test_controller_config("Base prompt"),
            sender,
            invocation,
        ))
        .await
        .unwrap_err();
        assert_eq!(event_reader.await.unwrap(), 0);
        assert!(
            error.contains(if valid_preparation {
                "Initial creation rejected by test host."
            } else if capture_host_present {
                "missing-admission-agent"
            } else {
                "profile capture host"
            }),
            "{error}"
        );
        assert_eq!(host.reserved.load(Ordering::SeqCst), 0);
        assert_eq!(
            host.created.load(Ordering::SeqCst),
            usize::from(valid_preparation)
        );
        if valid_preparation {
            let creation = host.creation.lock().unwrap().clone().unwrap();
            assert_eq!(creation.task_id, child);
            assert_eq!(creation.parent_session_id, parent);
            assert_eq!(creation.root_session_id, parent);
            assert_eq!(creation.execution_id, "preparation-rejection");
            assert_eq!(creation.connection_id, "test-openrouter");
            assert_eq!(creation.invocation_config.prompt, "review this change");
            assert_eq!(creation.runtime_identity.public_task_id, "agent-review");
        }
        assert_eq!(host.captured.load(Ordering::SeqCst), 0);
        assert_eq!(host.activated.load(Ordering::SeqCst), 0);
        assert!(!runtime.task_has_active_or_starting_run(child).unwrap());
        assert!(
            runtime
                .detached_task_event_streams
                .read()
                .unwrap()
                .keys()
                .all(|key| key.task_id != child)
        );
    }
}
