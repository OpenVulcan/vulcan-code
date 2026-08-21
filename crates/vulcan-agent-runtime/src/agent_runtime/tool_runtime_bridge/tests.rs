use super::{
    RuntimeEventProjectionBarrierRegistry, VulcanManagedToolRuntimeBridge,
    acknowledge_runtime_event_projection_barrier_from_event,
    attach_runtime_event_projection_barrier, cancel_runtime_event_projection_barrier,
    clear_runtime_event_projection_barriers, insert_runtime_event_projection_barrier,
    reject_runtime_event_projection_barrier_from_event, remove_runtime_event_projection_barrier,
    runtime_event_projection_barriers_enabled,
    runtime_event_requires_tool_execution_safety_barrier,
    set_runtime_event_projection_barriers_test_override,
};
use crate::agent_runtime::events::RuntimeEventKind;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use async_trait::async_trait;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, oneshot};
use uuid::Uuid;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::tool::{AllowAllToolPolicy, VulcanToolExecutor};
use vulcan_core::types::{
    ControllerConfig, ConversationPayload, ConversationSnapshot, ConversationToolCall,
    CoreError as VulcanCoreError, ProviderRequestSnapshot, ProviderTurnResponse,
    ToolInvocationRequest,
};
use vulcan_tools_manager::{
    VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolHostResult, VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};

/// Returns one demo tools manager plus the resolved tool definitions used by the bridge tests.
/// 返回一份供桥接测试复用的演示工具管理器及其已解析工具定义集合。
fn build_demo_manager() -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("demo_tool"),
                name: String::from("demo_tool"),
                description: String::from("Demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(|args, session_id| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_model_output(json!({
                        "args": args,
                        "session_id": session_id,
                    }))
                    .with_host_result(VulcanToolHostResult {
                        kind: String::from("change_set"),
                        payload: json!({ "path": "src/main.rs", "op": "read" }),
                    }))
                })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("failing_tool"),
                name: String::from("failing_tool"),
                description: String::from("Failing demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Hidden,
            request_streaming: None,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async move { Err(String::from("LuaSkills 工具解析失败：演示错误")) })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("structured_failure_tool"),
                name: String::from("structured_failure_tool"),
                description: String::from("Structured failure demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({ "type": "object" }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_tool_error_details(
                        "structured persistence failure",
                        json!({ "phase": "persist" }),
                    ))
                })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: String::from("invalid_structured_failure_tool"),
                name: String::from("invalid_structured_failure_tool"),
                description: String::from("Invalid structured failure demo tool"),
                help_text: None,
                source: VulcanToolSourceKind::Native,
            },
            parameters_schema: json!({ "type": "object" }),
            ai_visible: true,
            plan_policy: VulcanPlanToolPolicy::Visible,
            request_streaming: None,
            invoke: Arc::new(|_args, _session_id| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_model_output(json!({
                        "status": "failed"
                    }))
                    .with_host_result(VulcanToolHostResult {
                        kind: String::from(VULCAN_TOOL_ERROR_DETAIL_KIND),
                        payload: json!({ "status": "failed" }),
                    }))
                })
            }),
        },
    ]);
    let resolved = manager.resolve_for_execution_mode("Build");
    (manager, resolved.definitions)
}

/// Returns one demo tools manager that contains a tool panicking during asynchronous invocation.
/// 返回一份包含异步调用期间 panic 工具的演示工具管理器。
fn build_panic_demo_manager() -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("panic_tool"),
            name: String::from("panic_tool"),
            description: String::from("Panic demo tool"),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(|_args, _session_id| {
            Box::pin(async move {
                panic!("模拟搜索节点 panic");
            })
        }),
    }]);
    let resolved = manager.resolve_for_execution_mode("Build");
    (manager, resolved.definitions)
}

/// Returns a tools manager whose tool body waits until the test explicitly lets it continue.
/// 返回一份工具主体会等待测试显式放行的工具管理器。
fn build_gated_demo_manager(
    entered_tool_body: Arc<AtomicBool>,
    release_tool_body: Arc<Notify>,
) -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("gated_tool"),
            name: String::from("gated_tool"),
            description: String::from("Gated demo tool"),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |_args, _session_id| {
            let entered_tool_body = Arc::clone(&entered_tool_body);
            let release_tool_body = Arc::clone(&release_tool_body);
            Box::pin(async move {
                entered_tool_body.store(true, Ordering::SeqCst);
                release_tool_body.notified().await;
                Ok(VulcanToolInvocationResult::from_model_output(json!(
                    "gated done"
                )))
            })
        }),
    }]);
    let resolved = manager.resolve_for_execution_mode("Build");
    (manager, resolved.definitions)
}

/// Drains all live runtime events currently queued by a streaming tool executor.
/// 清空流式工具执行器当前已经排队的全部实时运行事件。
fn drain_live_events(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
) -> Vec<ProviderStreamDelta> {
    let mut events = Vec::new();
    while let Ok(item) = receiver.try_recv() {
        events.push(item.expect("live event should not fail"));
    }
    events
}

/// Owns one projection-barrier override isolated to the current test thread.
/// 持有一份隔离到当前测试线程的投影屏障覆盖。
struct RuntimeProjectionBarrierTestOverrideGuard;

impl RuntimeProjectionBarrierTestOverrideGuard {
    /// Enables projection barriers only for the current test thread.
    /// 仅为当前测试线程启用投影屏障。
    ///
    /// # Returns
    /// 返回值
    /// A scope guard that clears the override during normal return or panic unwinding.
    /// 返回在正常退出或 panic 展开时清除覆盖的作用域守卫。
    fn enabled() -> Self {
        set_runtime_event_projection_barriers_test_override(Some(true));
        Self
    }
}

impl Drop for RuntimeProjectionBarrierTestOverrideGuard {
    /// Clears the current-thread override during test cleanup.
    /// 在测试清理阶段清除当前线程覆盖。
    fn drop(&mut self) {
        set_runtime_event_projection_barriers_test_override(None);
    }
}

/// Verifies the test override is thread-local and clears during panic unwinding.
/// 验证测试覆盖仅作用于当前线程，并会在 panic 展开期间清除。
#[test]
fn projection_barrier_test_override_is_thread_local_and_unwind_safe() {
    // panicResult captures deliberate unwinding while the override guard owns current-thread state.
    // panicResult 捕获覆盖守卫持有当前线程状态期间的刻意展开。
    let panic_result = std::panic::catch_unwind(|| {
        // barrierOverride enables only the current test thread for this nested scope.
        // barrierOverride 仅为当前测试线程的嵌套作用域启用屏障。
        let _barrier_override = RuntimeProjectionBarrierTestOverrideGuard::enabled();
        assert!(runtime_event_projection_barriers_enabled());
        // otherThreadEnabled observes the production default from a distinct thread without the override.
        // otherThreadEnabled 从没有覆盖的独立线程观察生产默认值。
        let other_thread_enabled = std::thread::spawn(runtime_event_projection_barriers_enabled)
            .join()
            .expect("barrier isolation thread should join");
        assert!(!other_thread_enabled);
        panic!("trigger projection-barrier override cleanup");
    });

    assert!(panic_result.is_err());
    assert!(
        !runtime_event_projection_barriers_enabled(),
        "panic unwinding must clear the current-thread test override"
    );
}

/// Verifies every registry mutation helper reports a poisoned authoritative lock.
/// 验证每个注册表变更辅助函数都会报告权威锁中毒。
#[test]
fn projection_barrier_registry_helpers_report_poisoned_lock() {
    // registry is a local authoritative barrier table that can be poisoned without affecting other tests.
    // registry 是可在不影响其他测试的前提下中毒的局部权威屏障表。
    let registry = Mutex::new(RuntimeEventProjectionBarrierRegistry::new());
    // poisonResult proves subsequent helper calls cannot acquire authoritative state.
    // poisonResult 证明后续辅助函数无法取得权威状态。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = registry
            .lock()
            .expect("local barrier registry should lock before poisoning");
        panic!("poison local projection barrier registry");
    }));
    assert!(poison_result.is_err());
    // sender is the pending acknowledgement value rejected by the poisoned insert helper.
    // sender 是被中毒插入辅助函数拒绝的待确认值。
    let (sender, _receiver) = oneshot::channel();

    // insertError identifies the failed registration operation.
    // insertError 标识失败的注册操作。
    let insert_error = insert_runtime_event_projection_barrier(
        &registry,
        String::from("poisoned-insert"),
        sender,
        "register a test barrier",
    )
    .expect_err("poisoned registry must reject insertion");
    // removeError identifies the failed removal operation.
    // removeError 标识失败的移除操作。
    let remove_error = remove_runtime_event_projection_barrier(
        &registry,
        "poisoned-remove",
        "remove a test barrier",
    )
    .expect_err("poisoned registry must reject removal");
    // clearError identifies the failed bulk cleanup operation.
    // clearError 标识失败的批量清理操作。
    let clear_error = clear_runtime_event_projection_barriers(&registry, "clear test barriers")
        .expect_err("poisoned registry must reject clearing");

    assert!(insert_error.contains("register a test barrier"));
    assert!(remove_error.contains("remove a test barrier"));
    assert!(clear_error.contains("clear test barriers"));
}

/// Verifies missing and already-abandoned acknowledgements remain legitimate non-error outcomes.
/// 验证缺失屏障与等待方已放弃的确认仍是合法非错误结果。
#[test]
fn projection_barrier_acknowledgement_preserves_legitimate_absence() {
    // eventWithoutBarrier has no projection metadata and therefore no acknowledgement responsibility.
    // eventWithoutBarrier 不含投影元数据，因此没有确认责任。
    let event_without_barrier = crate::agent_runtime::events::RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "No barrier",
        "No barrier metadata",
    );
    assert!(
        !acknowledge_runtime_event_projection_barrier_from_event(&event_without_barrier)
            .expect("missing barrier metadata should remain a valid outcome")
    );

    // barrierOverride enables a real isolated barrier for the abandoned-waiter case.
    // barrierOverride 为等待方已放弃场景启用真实隔离屏障。
    let _barrier_override = RuntimeProjectionBarrierTestOverrideGuard::enabled();
    // event and barrier share the exact acknowledgement channel created by the runtime.
    // event 与 barrier 共享运行时创建的精确确认通道。
    let (event, barrier) =
        attach_runtime_event_projection_barrier(crate::agent_runtime::events::RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "Abandoned barrier",
            "Receiver dropped before acknowledgement",
        ))
        .expect("isolated barrier should attach");
    drop(barrier);

    assert!(
        !acknowledge_runtime_event_projection_barrier_from_event(&event)
            .expect("abandoned waiter should remain a valid late acknowledgement")
    );
}

/// Verifies only an executor-owned ToolStarted barrier is classified as a real tool side-effect gate.
/// 验证只有执行器持有屏障的 ToolStarted 才会被分类为真实工具副作用门禁。
#[test]
fn tool_execution_safety_classification_rejects_synthetic_tool_started_events() {
    // syntheticContextEvent mirrors startup and discovery events that historically reuse ToolStarted for display.
    // syntheticContextEvent 模拟历史上为展示而复用 ToolStarted 的启动与发现事件。
    let synthetic_context_event = crate::agent_runtime::events::RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "执行上下文",
        "已进入真实执行阶段",
    );
    assert!(!runtime_event_requires_tool_execution_safety_barrier(
        &synthetic_context_event
    ));

    // barrierOverride enables the production ownership marker for an actual executor-owned start event.
    // barrierOverride 为真实执行器持有的开始事件启用生产所有权标记。
    let _barrier_override = RuntimeProjectionBarrierTestOverrideGuard::enabled();
    let (executor_event, executor_barrier) =
        attach_runtime_event_projection_barrier(crate::agent_runtime::events::RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · call-1 · shell_command",
            "started",
        ))
        .expect("tool-start barrier should attach");
    assert!(runtime_event_requires_tool_execution_safety_barrier(
        &executor_event
    ));
    cancel_runtime_event_projection_barrier(executor_barrier)
        .expect("tool-start test barrier should be cancelled");

    // terminalEvent proves a display-order barrier never becomes a pre-side-effect gate.
    // terminalEvent 证明仅用于显示排序的终态屏障绝不会变成副作用前置门禁。
    let (terminal_event, terminal_barrier) =
        attach_runtime_event_projection_barrier(crate::agent_runtime::events::RuntimeEvent::new(
            RuntimeEventKind::ToolFinished,
            "工具返回 · call-1 · shell_command",
            "completed",
        ))
        .expect("tool-finished barrier should attach");
    assert!(!runtime_event_requires_tool_execution_safety_barrier(
        &terminal_event
    ));
    cancel_runtime_event_projection_barrier(terminal_barrier)
        .expect("tool-finished test barrier should be cancelled");
}

/// Captures pre-planned provider turn responses and records the built request history for assertions.
/// 持有预先规划的 Provider 回合响应，并记录构建出的请求历史供断言使用。
#[derive(Debug, Clone)]
struct MockControllerProviderAdapter {
    /// Provider identifier exposed to the controller.
    /// 暴露给控制器的 Provider 标识。
    provider_id: String,
    /// Model identifier exposed to the controller.
    /// 暴露给控制器的模型标识。
    model_id: String,
    /// Planned turn responses returned sequentially.
    /// 按顺序返回的计划回合响应队列。
    planned_responses: Arc<Mutex<VecDeque<ProviderTurnResponse>>>,
    /// Built request snapshots captured during the loop.
    /// 在循环过程中捕获到的请求快照集合。
    built_requests: Arc<Mutex<Vec<ProviderRequestSnapshot>>>,
}

impl MockControllerProviderAdapter {
    /// Builds one mock controller provider adapter from a response queue.
    /// 根据响应队列构建一个模拟控制器 Provider 适配器。
    fn new(planned_responses: Vec<ProviderTurnResponse>) -> Self {
        Self {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            planned_responses: Arc::new(Mutex::new(VecDeque::from(planned_responses))),
            built_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the built request snapshots captured by the adapter so far.
    /// 返回适配器截至当前捕获到的请求快照集合。
    fn built_requests(&self) -> Vec<ProviderRequestSnapshot> {
        self.built_requests
            .lock()
            .expect("lock built requests")
            .clone()
    }
}

#[async_trait]
impl VulcanProviderAdapter for MockControllerProviderAdapter {
    type Request = ProviderRequestSnapshot;
    type Response = ProviderTurnResponse;

    /// Returns the stable provider identifier exposed by the mock adapter.
    /// 返回模拟适配器暴露的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier exposed by the mock adapter.
    /// 返回模拟适配器暴露的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one simple request snapshot directly from the controller snapshot for later assertions.
    /// 直接根据控制器快照构建简单请求快照，供后续断言使用。
    async fn build_request(
        &self,
        snapshot: &ConversationSnapshot,
        config: &ControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let request = ProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                config.system_prompt.clone().clone(),
            ),
            prompt: String::from("controller-integration"),
            prompt_in_history: false,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: Vec::new(),
            additional_params: config.additional_params.clone(),
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        self.built_requests
            .lock()
            .expect("lock built requests")
            .push(request.clone());
        Ok(request)
    }

    /// Returns the next queued response or fails when the queue is empty.
    /// 返回下一个排队响应，若队列为空则返回失败。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        self.planned_responses
            .lock()
            .expect("lock planned responses")
            .pop_front()
            .ok_or_else(|| VulcanCoreError::InvalidState {
                message: String::from("模拟控制器 Provider 响应已耗尽。"),
            })
    }
}

/// Verifies the shared host-managed runtime bridge executes one resolved tool request and preserves host metadata.
/// 验证共享宿主管理运行时桥接层能够执行单条已解析工具请求，并保留宿主元数据。
#[test]
fn managed_tool_runtime_bridge_executes_core_tool_request() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("core tool request should execute");

    assert_eq!(result.tool_call_id, "call-1");
    assert_eq!(result.tool_name, "demo_tool");
    assert!(result.model_output.contains("\"src/main.rs\""));
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "change_set",
            "payload": { "path": "src/main.rs", "op": "read" }
        }))
    );
}

/// Verifies host-managed tool invocation failures are downgraded into one error result returned to the controller instead of aborting the whole turn.
/// 验证宿主管理工具调用失败会被降级为回传给控制器的一条错误结果，而不是直接中断整轮执行。
#[test]
fn managed_tool_runtime_bridge_converts_tool_failures_into_error_results() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-error-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("failing tool should still return one controller-visible result");

    assert_eq!(result.tool_call_id, "call-error-1");
    assert_eq!(result.tool_name, "failing_tool");
    assert!(result.is_error);
    assert!(result.model_output.contains("Tool `failing_tool` failed"));
    assert!(
        result
            .model_output
            .contains("LuaSkills 工具解析失败：演示错误")
    );
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "tool_error",
            "payload": {
                "tool_call_id": "call-error-1",
                "tool_name": "failing_tool",
                "status": "failed",
                "error": "LuaSkills 工具解析失败：演示错误",
                "arguments": { "query": "README" }
            }
        }))
    );
}

/// Verifies a tool-owned structured error keeps its host payload and Core error semantics.
/// 验证工具自有结构化错误会保留宿主载荷与 Core 错误语义。
#[test]
fn managed_tool_runtime_bridge_preserves_structured_tool_error_results() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-structured-error-1"),
        tool_name: String::from("structured_failure_tool"),
        arguments: json!({}),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("structured tool failure should remain controller-visible");

    assert!(result.is_error);
    assert!(result.model_output.contains("\"status\":\"failed\""));
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "tool_error",
            "payload": {
                "tool_call_id": "call-structured-error-1",
                "tool_name": "structured_failure_tool",
                "status": "failed",
                "error": "structured persistence failure",
                "arguments": {},
                "details": {
                    "phase": "persist"
                }
            }
        }))
    );
}

/// Verifies malformed tool-owned error details become an explicit canonical protocol failure.
/// 验证格式错误的工具自有错误详情会转换为显式规范协议失败。
#[test]
fn managed_tool_runtime_bridge_rejects_structured_error_without_error_text() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-invalid-structured-error-1"),
        tool_name: String::from("invalid_structured_failure_tool"),
        arguments: json!({}),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("protocol failure should remain controller-visible");

    assert!(result.is_error);
    assert!(
        result
            .model_output
            .contains("without a non-empty `error` string")
    );
    let host_result = result
        .host_result
        .expect("protocol failure should use canonical host metadata");
    assert_eq!(host_result["kind"], "tool_error");
    assert_eq!(
        host_result["payload"]["tool_call_id"],
        "call-invalid-structured-error-1"
    );
    assert_eq!(
        host_result["payload"]["tool_name"],
        "invalid_structured_failure_tool"
    );
}

/// Verifies Plan mode blocks disallowed tools at the shared bridge instead of hiding them from the provider request.
/// 验证 Plan 模式会在共享桥接层拦截不允许的工具，而不是从 Provider 请求中隐藏它们。
#[test]
fn managed_tool_runtime_bridge_blocks_disallowed_plan_tool_invocation() {
    let (manager, definitions) = build_demo_manager();
    assert!(
        definitions
            .iter()
            .any(|definition| { definition.descriptor.name == "failing_tool" })
    );
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Plan");
    let request = ToolInvocationRequest {
        tool_call_id: String::from("call-plan-blocked-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("plan-blocked tool should return model-visible guidance");

    assert_eq!(result.tool_call_id, "call-plan-blocked-1");
    assert_eq!(result.tool_name, "failing_tool");
    assert!(!result.is_error);
    assert!(result.model_output.contains("Plan Mode is active"));
    assert!(!result.model_output.contains("演示错误"));
    assert_eq!(result.host_result, None);
}

/// Verifies the host-owned `vulcan-core` loop can execute through the shared workbench tool bridge and preserve host metadata in canonical records.
/// 验证宿主自有的 `vulcan-core` 循环能够经由共享工作台工具桥接执行，并在规范记录中保留宿主元数据。
#[test]
fn managed_tool_runtime_bridge_integrates_with_core_controller_loop() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let provider = MockControllerProviderAdapter::new(vec![
        ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::from("先调用工具获取文件状态。"),
            provider_message_id: Some(String::from("provider-1")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("tool-call-1")),
                tool_name: String::from("demo_tool"),
                arguments: json!({ "path": "src/main.rs" }),
            }],
            continuation: None,
            usage: None,
        },
        ProviderTurnResponse {
            assistant_text: String::from("工具执行完成，已经拿到宿主结果。"),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("provider-2")),
            provider_message_state: None,
            tool_calls: Vec::new(),
            continuation: None,
            usage: None,
        },
    ]);
    let mut controller = VulcanCoreController::new(
        ControllerConfig {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system").clone(),
            ),
            temperature: Some(0.2),
            max_turns: 3,
            additional_params: None,
            context_injection: None,
        },
        ConversationSnapshot::default(),
    )
    .expect("valid core conversation snapshot");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(controller.run_user_turn(
            &provider,
            &runtime_bridge,
            &AllowAllToolPolicy,
            "请检查主文件",
        ))
        .expect("core controller loop should succeed");

    assert_eq!(result.provider_turns_executed, 2);
    assert_eq!(result.tool_calls_processed, 1);
    assert_eq!(provider.built_requests().len(), 2);
    assert!(matches!(
        &controller.snapshot().messages[2].payload,
        ConversationPayload::ToolResult { text, .. } if text.contains("\"src/main.rs\"")
    ));
    assert!(controller.snapshot().records.iter().any(|record| {
        record.title == "调用 demo_tool 工具"
            && record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("kind"))
                .and_then(serde_json::Value::as_str)
                == Some("change_set")
    }));
}

/// Verifies the controller loop keeps running after one host-managed tool failure and can still receive a follow-up assistant response.
/// 验证宿主管理工具失败后控制器循环仍会继续运行，并且依然可以接收后续助手响应。
#[test]
fn managed_tool_runtime_bridge_keeps_controller_loop_alive_after_tool_failure() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let provider = MockControllerProviderAdapter::new(vec![
        ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::from("先尝试调用会失败的工具。"),
            provider_message_id: Some(String::from("provider-1")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: String::from("tool-item-1"),
                tool_call_id: Some(String::from("tool-call-1")),
                tool_name: String::from("failing_tool"),
                arguments: json!({ "query": "README" }),
            }],
            continuation: None,
            usage: None,
        },
        ProviderTurnResponse {
            assistant_text: String::from("我已收到工具错误原因，接下来会调整策略继续处理。"),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("provider-2")),
            provider_message_state: None,
            tool_calls: Vec::new(),
            continuation: None,
            usage: None,
        },
    ]);
    let mut controller = VulcanCoreController::new(
        ControllerConfig {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            system_prompt: String::from("system"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system").clone(),
            ),
            temperature: Some(0.2),
            max_turns: 3,
            additional_params: None,
            context_injection: None,
        },
        ConversationSnapshot::default(),
    )
    .expect("valid core conversation snapshot");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(controller.run_user_turn(
            &provider,
            &runtime_bridge,
            &AllowAllToolPolicy,
            "请继续分析 README",
        ))
        .expect("controller loop should continue after one tool failure");

    assert_eq!(result.provider_turns_executed, 2);
    assert_eq!(result.tool_calls_processed, 1);
    assert!(matches!(
        &controller.snapshot().messages[2].payload,
        ConversationPayload::ToolResult { text, .. }
            if text.contains("Tool `failing_tool` failed")
                && text.contains("LuaSkills 工具解析失败：演示错误")
    ));
    assert!(matches!(
        controller
            .snapshot()
            .messages
            .last()
            .map(|message| &message.payload),
        Some(ConversationPayload::Assistant { text, .. })
            if text == "我已收到工具错误原因，接下来会调整策略继续处理。"
    ));
}

/// Verifies the streaming executor turns a panicking host tool into a model-visible tool error instead of crashing the controller task.
/// 验证流式执行器会将 panic 的宿主工具转换为模型可见的工具错误，而不是让控制器任务崩溃。
#[test]
fn streaming_tool_executor_converts_tool_panics_into_error_results() {
    let (manager, definitions) = build_panic_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("panic-call-1"),
        tool_name: String::from("panic_tool"),
        arguments: json!({ "query": "山姆零食" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("panicking tool should still return one controller-visible result");
    let events = drain_live_events(&mut receiver);

    assert!(result.is_error);
    assert!(result.model_output.contains("Tool `panic_tool` failed"));
    assert!(result.model_output.contains("模拟搜索节点 panic"));
    assert!(matches!(
        result.host_result,
        Some(ref value)
            if value["kind"] == "tool_error"
                && value["payload"]["tool_name"] == "panic_tool"
                && value["payload"]["error"]
                    .as_str()
                    .is_some_and(|message| message.contains("模拟搜索节点 panic"))
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolStarted
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolFinished
                && runtime_event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
    )));
}

/// Verifies a controller-visible tool error is projected with failed display state.
/// 验证控制器可见工具错误会投影为失败展示状态。
#[test]
fn streaming_tool_executor_marks_controller_error_results_failed() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("failing-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "演示错误" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("managed tool failure should still return a normal controller-visible result");
    let events = drain_live_events(&mut receiver);

    assert!(result.is_error);
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolFinished
                && runtime_event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
    )));
}

/// Verifies rejected tool requests return a friendly tool result without entering the tool body.
/// 验证被拒绝的工具请求会返回友好工具结果，且不会进入工具主体。
#[test]
fn streaming_tool_executor_rejects_tool_requests_before_host_execution() {
    let entered_tool_body = Arc::new(AtomicBool::new(false));
    let release_tool_body = Arc::new(Notify::new());
    let (manager, definitions) = build_gated_demo_manager(
        Arc::clone(&entered_tool_body),
        Arc::clone(&release_tool_body),
    );
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Compaction",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        runtime_bridge,
        sender,
        Some(String::from(
            "Tool calls are not allowed during context compaction.",
        )),
        true,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        tool_call_id: String::from("rejected-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "must not run" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("policy rejection should be returned as a normal tool result");
    let events = drain_live_events(&mut receiver);

    assert!(!result.is_error);
    assert!(
        result
            .model_output
            .contains("Tool request rejected by runtime policy")
    );
    assert!(
        result
            .model_output
            .contains("Tool calls are not allowed during context compaction")
    );
    assert_eq!(
        result
            .host_result
            .as_ref()
            .and_then(|value| value.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("tool_result")
    );
    assert_eq!(
        result
            .host_result
            .as_ref()
            .and_then(|value| value.get("payload"))
            .and_then(|payload| payload.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    assert!(
        !entered_tool_body.load(Ordering::SeqCst),
        "rejected tool request must not enter the host tool body"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolStarted
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolFinished
                && runtime_event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("completed")
    )));
}

/// Verifies a rejection reason without an all-tools flag or scoped tool ids does not block normal tool calls.
/// 验证仅存在拒绝说明但没有全工具标记或限定工具标识时不会阻止普通工具调用。
#[test]
fn streaming_tool_executor_does_not_reject_when_scope_is_empty() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        runtime_bridge,
        sender,
        Some(String::from("Scoped rejection is not active.")),
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        tool_call_id: String::from("empty-scope-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "scope empty should still run" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let result = runtime
        .block_on(executor.execute_tool(&request))
        .expect("empty rejection scope should allow the host tool call to run");
    let events = drain_live_events(&mut receiver);

    assert!(result.is_error);
    assert!(events.iter().any(|event| matches!(
        event,
        ProviderStreamDelta::RuntimeEvent(runtime_event)
            if runtime_event.kind == RuntimeEventKind::ToolStarted
    )));
}

/// Verifies scoped policy rejection only blocks the listed tool identifiers.
/// 验证限定范围的策略拒绝只会阻止列出的工具标识。
#[test]
fn streaming_tool_executor_rejects_only_scoped_tool_ids() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        runtime_bridge,
        sender,
        Some(String::from(
            "This tool is reserved for the parent session.",
        )),
        false,
        vec![String::from("failing_tool")],
    );
    let rejected_request = ToolInvocationRequest {
        tool_call_id: String::from("scoped-rejected-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "must be rejected before the failing body runs" }),
    };
    let allowed_request = ToolInvocationRequest {
        tool_call_id: String::from("scoped-allowed-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let rejected_result = runtime
        .block_on(executor.execute_tool(&rejected_request))
        .expect("scoped policy rejection should return a normal tool result");
    let allowed_result = runtime
        .block_on(executor.execute_tool(&allowed_request))
        .expect("unlisted tool should execute normally");
    let events = drain_live_events(&mut receiver);

    assert!(!rejected_result.is_error);
    assert!(
        rejected_result
            .model_output
            .contains("Tool request rejected by runtime policy")
    );
    assert_eq!(
        rejected_result
            .host_result
            .as_ref()
            .and_then(|value| value.get("payload"))
            .and_then(|payload| payload.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    assert!(!allowed_result.is_error);
    assert_eq!(
        allowed_result
            .host_result
            .as_ref()
            .and_then(|value| value.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("change_set")
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                ProviderStreamDelta::RuntimeEvent(runtime_event)
                    if runtime_event.kind == RuntimeEventKind::ToolFinished
            ))
            .count(),
        2
    );
}

/// Verifies the streaming executor yields the tool-start event before the actual tool body begins.
/// 验证流式执行器会在真实工具主体开始前先产出工具开始事件。
#[test]
fn streaming_tool_executor_yields_tool_started_before_tool_body_runs() {
    let entered_tool_body = Arc::new(AtomicBool::new(false));
    let release_tool_body = Arc::new(Notify::new());
    let (manager, definitions) = build_gated_demo_manager(
        Arc::clone(&entered_tool_body),
        Arc::clone(&release_tool_body),
    );
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("gated-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "先显示工具调用" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // barrierOverride enables real acknowledgements without mutating the process-wide production switch.
        // barrierOverride 启用真实确认且不修改进程级生产开关。
        let _barrier_override = RuntimeProjectionBarrierTestOverrideGuard::enabled();
        let task = tokio::spawn(async move { executor.execute_tool(&request).await });
        let first_event = receiver
            .recv()
            .await
            .expect("tool-start event should be sent")
            .expect("tool-start event should be ok");

        // startedEvent is the exact UI-visible event whose acknowledgement gates the tool body.
        // startedEvent 是以确认操作阻塞工具主体的精确界面可见事件。
        let ProviderStreamDelta::RuntimeEvent(started_event) = first_event else {
            panic!("first delta should be a tool-start runtime event");
        };
        assert_eq!(started_event.kind, RuntimeEventKind::ToolStarted);
        assert_eq!(
            started_event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_call_id"))
                .and_then(serde_json::Value::as_str),
            Some("gated-call-1")
        );
        assert!(
            !entered_tool_body.load(Ordering::SeqCst),
            "tool body should not run before the UI-visible start event is observable"
        );
        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&started_event)
                .expect("tool-start barrier registry should remain available"),
            "tool-start barrier should acknowledge"
        );

        release_tool_body.notify_one();
        // finishedEvent is the terminal event that must also release its pending barrier.
        // finishedEvent 是同样必须释放待确认屏障的终态事件。
        let finished_event = receiver
            .recv()
            .await
            .expect("tool-finished event should be sent")
            .expect("tool-finished event should be ok");
        let ProviderStreamDelta::RuntimeEvent(finished_event) = finished_event else {
            panic!("second delta should be a tool-finished runtime event");
        };
        assert_eq!(finished_event.kind, RuntimeEventKind::ToolFinished);
        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&finished_event)
                .expect("tool-finished barrier registry should remain available"),
            "tool-finished barrier should acknowledge"
        );
        let result = task
            .await
            .expect("tool task should join")
            .expect("tool call should complete");
        assert_eq!(result.model_output, "gated done");
    });
}

/// Verifies a host-side safety rejection fails the tool call before the external tool body can run.
/// 验证宿主侧安全拒绝会在外部工具主体运行前让工具调用失败。
#[test]
fn streaming_tool_executor_rejects_tool_body_when_safety_barrier_fails() {
    // enteredToolBody records the only externally observable body-entry boundary.
    // enteredToolBody 记录唯一可从外部观察的主体进入边界。
    let entered_tool_body = Arc::new(AtomicBool::new(false));
    // releaseToolBody would unblock the body if the safety regression accidentally allowed it to start.
    // releaseToolBody 会在安全回归意外放行主体时解除其阻塞。
    let release_tool_body = Arc::new(Notify::new());
    let (manager, definitions) = build_gated_demo_manager(
        Arc::clone(&entered_tool_body),
        Arc::clone(&release_tool_body),
    );
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("gated-rejected-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "拒绝不安全工具边界" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // barrierOverride enables the production-shaped mandatory gate for this current-thread test.
        // barrierOverride 为当前线程测试启用生产形态的强制门禁。
        let _barrier_override = RuntimeProjectionBarrierTestOverrideGuard::enabled();
        // task waits on the ToolStarted barrier before it can enter the gated manager.
        // task 在进入受控管理器前等待 ToolStarted 屏障。
        let task = tokio::spawn(async move { executor.execute_tool(&request).await });
        let started_event = receiver
            .recv()
            .await
            .expect("tool-start event should be sent")
            .expect("tool-start event should be ok");
        let ProviderStreamDelta::RuntimeEvent(started_event) = started_event else {
            panic!("first delta should be a tool-start runtime event");
        };
        assert_eq!(started_event.kind, RuntimeEventKind::ToolStarted);
        assert!(
            reject_runtime_event_projection_barrier_from_event(&started_event)
                .expect("tool-start barrier rejection should reach the registry"),
            "tool-start barrier should reject the exact waiter"
        );

        let error = task
            .await
            .expect("tool task should join")
            .expect_err("rejected safety barrier must fail the tool call");
        assert!(
            error.to_string().contains("safety barrier was rejected"),
            "tool failure should retain the controlled safety diagnostic"
        );
        assert!(
            !entered_tool_body.load(Ordering::SeqCst),
            "rejected safety barrier must prevent tool-body entry"
        );
    });
}

/// Verifies the streaming executor waits for the terminal tool event projection before returning to Core.
/// 验证流式执行器会等待工具终态事件完成投影后再返回给 Core。
#[test]
fn streaming_tool_executor_waits_for_tool_finished_projection_before_returning() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor =
        StreamingRuntimeToolExecutor::new(runtime_bridge, sender, None, false, Vec::new());
    let request = ToolInvocationRequest {
        tool_call_id: String::from("barrier-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // barrierOverride isolates real projection acknowledgements to this current-thread runtime.
        // barrierOverride 将真实投影确认隔离到当前线程运行时。
        let _barrier_override = RuntimeProjectionBarrierTestOverrideGuard::enabled();
        let task = tokio::spawn(async move { executor.execute_tool(&request).await });

        let started_event = receiver
            .recv()
            .await
            .expect("tool-start event should be sent")
            .expect("tool-start event should be ok");
        let ProviderStreamDelta::RuntimeEvent(started_event) = started_event else {
            panic!("first delta should be a tool-start runtime event");
        };
        assert_eq!(started_event.kind, RuntimeEventKind::ToolStarted);
        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&started_event)
                .expect("tool-start barrier registry should remain available"),
            "tool-start barrier should acknowledge"
        );

        let finished_event = receiver
            .recv()
            .await
            .expect("tool-finished event should be sent")
            .expect("tool-finished event should be ok");
        let ProviderStreamDelta::RuntimeEvent(finished_event) = finished_event else {
            panic!("second delta should be a tool-finished runtime event");
        };
        assert_eq!(finished_event.kind, RuntimeEventKind::ToolFinished);
        assert!(
            !task.is_finished(),
            "execute_tool must wait until the terminal projection is acknowledged"
        );

        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&finished_event)
                .expect("tool-finished barrier registry should remain available"),
            "tool-finished barrier should acknowledge"
        );
        let result = task
            .await
            .expect("tool task should join")
            .expect("tool call should complete");
        assert!(result.model_output.contains("src/main.rs"));
    });
}
