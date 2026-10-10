//! Real file mutation followed by a rejected terminal receipt, isolated from user storage.
//! 真实文件修改后拒绝终态回执，与用户存储隔离。

use super::*;
use crate::agent_runtime::events::ToolExecutionRecoveryBoundary;
use crate::tooling::native::{NativeToolExecutionRequest, build_file_write_tool};
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

/// Exercises the real file adapter, managed bridge and controller with terminal persistence rejection.
/// 使用终态持久化拒绝验证真实文件适配器、托管桥接层和控制器。
/// No parameters or return value; verifies disk bytes, one invocation and no subsequent provider request.
/// 无参数或返回值；验证磁盘字节、单次执行以及没有后续供应商请求。
#[test]
fn file_write_receipt_rejection_preserves_disk_effects_without_replay() {
    // Keep every mutation in an automatically cleaned temporary workspace.
    // 将所有变更限制在自动清理的临时工作区。
    let directory = tempfile::tempdir().expect("temporary workspace");
    // The original file makes this an actual modification rather than a fabricated applied receipt.
    // 原文件使本次调用成为真实修改，而非伪造的已应用回执。
    let path = directory.path().join("receipt.txt");
    std::fs::write(&path, "before\n").unwrap();
    // Count body entries because idempotent writes could hide an accidental replay in final bytes.
    // 统计主体进入次数，因为幂等写入可能让最终字节掩盖意外重放。
    let calls = Arc::new(AtomicUsize::new(0));
    // Use the production native definition and schema, with only workspace/context supplied by the test.
    // 使用生产原生定义和 Schema，仅由测试提供工作区及上下文。
    let file = build_file_write_tool();
    // The invocation closure owns its isolated workspace and counter.
    // 调用闭包持有隔离工作区及计数器。
    let workspace = directory.path().to_path_buf();
    // Share the body counter with the post-failure assertions.
    // 与失败后的断言共享主体计数器。
    let invocation_calls = Arc::clone(&calls);
    // The manager delegates to the real file adapter instead of returning a synthetic ChangeSet.
    // 管理器委派给真实文件适配器，而非返回合成变更集。
    let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            file.descriptor.id.clone(),
            file.descriptor.name.clone(),
            file.descriptor.description.clone(),
            file.descriptor.help_text.clone(),
            VulcanToolOrigin::new(VulcanToolSourceKind::Native, "test", "file_write"),
        ),
        parameters_schema: file.args_schema.clone(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: file.request_streaming.clone(),
        invoke: Arc::new(move |arguments, _context| {
            // Each invocation retains the authoritative adapter and its own request context.
            // 每次调用保留权威适配器及自身请求上下文。
            let file = Arc::clone(&file);
            // Clone only the workspace path; never switch the process working directory.
            // 仅克隆工作区路径，绝不切换进程工作目录。
            let workspace = workspace.clone();
            invocation_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                file.invoke(NativeToolExecutionRequest {
                    arguments,
                    session_id: None,
                    native_browser_runtime: None,
                    host_project_id: None,
                    workspace_root: Some(workspace),
                    controller_runtime: None,
                    goal_runtime: None,
                    knowledge_access: None,
                    vce_retrieval_handler: None,
                    spill_root: None,
                    enabled_prompt_skill_names: Vec::new(),
                    prepared_prompt_skills: Vec::new(),
                    prompt_skill_user_root: None,
                    project_vmm: Default::default(),
                    session_vmm: Default::default(),
                })
                .await
                .map_err(|error| error.to_string())
            })
        }),
    }]);
    // Resolve the same catalog used by the managed execution bridge.
    // 解析托管执行桥接层使用的同一目录。
    let definitions = manager.resolve_for_execution_mode("Build").definitions;
    // The managed bridge owns the durable invocation identity.
    // 托管桥接层持有持久调用身份。
    let bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    // The channel exposes production recovery boundaries to the rejecting test host.
    // 通道向执行拒绝的测试宿主暴露生产恢复边界。
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    // Exercise the real streaming executor, including mandatory start/finish acknowledgement.
    // 验证真实流式执行器，包括强制开始和完成确认。
    let executor = StreamingRuntimeToolExecutor::new(bridge, sender, None, false, Vec::new());
    // A repeated provider response would replay the same write if the controller incorrectly continued.
    // 若控制器错误继续，重复的供应商响应将重放同一次写入。
    let response = ProviderTurnResponse {
        assistant_text: String::new(),
        assistant_reasoning: String::new(),
        provider_message_id: None,
        provider_message_state: None,
        tool_calls: vec![ConversationToolCall {
            id: String::from("receipt-stream"),
            tool_call_id: Some(String::from("receipt-call")),
            tool_name: String::from("file_write"),
            arguments: json!({"path":"receipt.txt", "content":"after\n"}),
            argument_parse_error: None,
            raw_arguments: Default::default(),
        }],
        continuation: None,
        usage: None,
    };
    // Two planned responses detect a forbidden follow-up without contacting any external service.
    // 两个计划响应用于发现禁止的后续请求，不访问外部服务。
    let provider = MockControllerProviderAdapter::new(vec![response.clone(), response]);
    // More than one allowed turn ensures the durability error, not the turn limit, stops execution.
    // 允许多轮，确保停止执行的是持久化错误而非轮数上限。
    let mut controller = VulcanCoreController::new(
        ControllerConfig {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system"),
            ),
            temperature: None,
            max_turns: 3,
            additional_params: None,
            context_injection: None,
        },
        ConversationSnapshot::default(),
    )
    .unwrap();
    // Thread-local barrier configuration requires all coordinating futures on the same runtime thread.
    // 线程局部屏障配置要求所有协调 Future 使用同一运行时线程。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        // The host accepts pre-execution persistence and rejects only the completed result.
        // 宿主接受执行前持久化，仅拒绝已完成的结果。
        let host = async {
            loop {
                // Every boundary carries its authoritative typed request/result.
                // 每个边界携带其权威类型化请求或结果。
                let delta = receiver
                    .recv()
                    .await
                    .expect("executor boundary")
                    .expect("valid delta");
                let ProviderStreamDelta::RuntimeEvent(event) = delta else {
                    panic!("expected runtime event")
                };
                match event
                    .tool_execution_boundary
                    .as_ref()
                    .expect("recovery boundary")
                {
                    ToolExecutionRecoveryBoundary::BatchPrepared(requests) => {
                        assert_eq!(requests.len(), 1);
                        assert_eq!(calls.load(Ordering::SeqCst), 0);
                        // Identity follows the emitted request instead of guessing its stream identifier.
                        // 身份来自已发出的请求，而非猜测流标识。
                        let request = requests.first().unwrap();
                        assert!(
                            acknowledge_tool_batch_projection_barrier_from_event(
                                &event,
                                vec![PreparedToolReceipt {
                                    api_call_id: ApiCallId::try_new(1).unwrap(),
                                    tool_call_id: ToolCallId::try_new(2).unwrap(),
                                    request_stream_id: RequestStreamId::try_new(
                                        request.request_stream_id.clone()
                                    )
                                    .unwrap(),
                                    tool_seq: Seq::try_new(3).unwrap(),
                                }]
                            )
                            .unwrap()
                        );
                    }
                    ToolExecutionRecoveryBoundary::Started { .. } => {
                        assert_eq!(calls.load(Ordering::SeqCst), 0);
                        assert_eq!(std::fs::read(&path).unwrap(), b"before\n");
                        assert!(
                            acknowledge_runtime_event_projection_barrier_from_event(&event)
                                .unwrap()
                        );
                    }
                    ToolExecutionRecoveryBoundary::Finished(result) => {
                        assert!(!result.is_error, "real file operation must succeed");
                        assert_eq!(std::fs::read(&path).unwrap(), b"after\n");
                        assert_eq!(calls.load(Ordering::SeqCst), 1);
                        // Validate the genuine receipt before injecting failure at its storage acknowledgement.
                        // 在存储确认处注入失败前，校验真实回执。
                        let receipt = result.host_result.as_ref().expect("native ChangeSet");
                        assert_eq!(receipt["payload"]["mode"], "applied");
                        vulcan_change_set::persistent_change_set_from_host_result(receipt)
                            .unwrap()
                            .unwrap();
                        assert!(
                            reject_runtime_event_projection_barrier_from_event_with_reason(
                                &event,
                                "Injected terminal receipt storage failure",
                            )
                            .unwrap()
                        );
                        break;
                    }
                }
            }
        };
        // Bound test failure time; a lost barrier acknowledgement must not hang the test runner.
        // 限定测试失败时间，丢失屏障确认不得挂住测试执行器。
        let (result, ()) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                controller.run_user_turn(&provider, &executor, &AllowAllToolPolicy, "write once"),
                host
            )
        })
        .await
        .expect("receipt rejection must settle");
        // The terminal controller result must preserve the already-applied side effect and prohibit replay.
        // 控制器终态必须保留已生效副作用并禁止重放。
        let error = result.expect_err("terminal receipt failure must stop the controller");
        let VulcanCoreError::ToolDurabilityFailure {
            details: Some(details),
            ..
        } = error
        else {
            panic!("expected typed persistence failure, got {error:?}");
        };
        assert!(details.side_effect_observed);
        assert!(details.terminal);
        assert!(!details.retryable);
        assert_eq!(
            details.upstream_code.as_deref(),
            Some("tool_result_persistence_failed")
        );
        assert!(details.message.contains("already applied"));
        assert!(
            details
                .message
                .contains("Injected terminal receipt storage failure")
        );
    });
    assert_eq!(std::fs::read(&path).unwrap(), b"after\n");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "file tool must not be replayed"
    );
    assert_eq!(
        provider.built_requests().len(),
        1,
        "no provider request after receipt failure"
    );
}
