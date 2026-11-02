use super::{
    RuntimeEventProjectionBarrierRegistry, VulcanManagedToolRuntimeBridge,
    acknowledge_runtime_event_projection_barrier_from_event,
    acknowledge_tool_batch_projection_barrier_from_event, attach_runtime_event_projection_barrier,
    cancel_runtime_event_projection_barrier, insert_runtime_event_projection_barrier,
    reject_runtime_event_projection_barrier_from_event,
    reject_runtime_event_projection_barrier_from_event_with_reason,
    remove_runtime_event_projection_barrier, runtime_event_requires_tool_execution_safety_barrier,
};
use crate::agent_runtime::events::RuntimeEventKind;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::task_runtime::TaskRuntime;
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
    ToolInvocationRequest, ToolInvocationResult, TranscriptRole,
};
use vulcan_session_core::v1::document::PreparedToolReceipt;
use vulcan_session_core::v1::ids::{ApiCallId, RequestStreamId, Seq, ToolCallId};
use vulcan_tools_manager::{
    VULCAN_TOOL_ERROR_DETAIL_KIND, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolHostResult, VulcanToolInvocationResult, VulcanToolOrigin, VulcanToolSourceKind,
    VulcanToolsManager,
};

mod file_durability;
mod tool_chain_steps;

/// Verifies a host credential never reaches model text or structured durable tool details.
/// 验证宿主凭据不会到达模型文本或结构化可靠工具详情。
#[test]
fn host_credential_is_redacted_from_tool_model_and_host_results() {
    let secret = String::from("w09-service-token-1234567890abcdef");
    let runtime = TaskRuntime::new();
    runtime
        .register_sensitive_tool_output_value(secret.clone())
        .expect("long host token should register");
    let (manager, definitions) = build_demo_manager();
    let bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build")
            .with_sensitive_tool_output_values(runtime.sensitive_tool_output_values());
    let result = ToolInvocationResult {
        tool_call_id: String::from("call-1"),
        tool_name: String::from("shell_command"),
        model_output: format!("file content: {secret}"),
        host_result: Some(json!({ "nested": [secret, { "detail": "safe" }] })),
        is_error: false,
    };
    let filtered = bridge
        .redact_sensitive_tool_result(result)
        .expect("registered filter should apply");
    assert_eq!(filtered.model_output, "file content: [redacted]");
    assert_eq!(
        filtered.host_result,
        Some(json!({
            "nested": ["[redacted]", { "detail": "safe" }]
        }))
    );
}

/// Installs one exact durable local identity before a bridge test dispatches the tool body.
/// 在桥接测试分派工具主体前安装一条精确的持久化本地身份。
fn install_test_invocation_identity(
    runtime_bridge: &VulcanManagedToolRuntimeBridge,
    request: &ToolInvocationRequest,
    api_call_id: u64,
    tool_call_id: u64,
    tool_seq: u64,
) -> PreparedToolReceipt {
    let receipt = PreparedToolReceipt {
        turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
        api_call_id: ApiCallId::try_new(api_call_id).expect("test API id should be valid"),
        tool_call_id: ToolCallId::try_new(tool_call_id).expect("test tool id should be valid"),
        request_stream_id: RequestStreamId::try_new(request.request_stream_id.clone())
            .expect("test request stream id should be valid"),
        tool_seq: Seq::try_new(tool_seq).expect("test tool seq should be valid"),
    };
    runtime_bridge
        .install_prepared_invocation_identities(
            std::slice::from_ref(request),
            vec![receipt.clone()],
        )
        .expect("durable test identity should install");
    receipt
}

/// Returns one demo tools manager plus the resolved tool definitions used by the bridge tests.
/// 返回一份供桥接测试复用的演示工具管理器及其已解析工具定义集合。
fn build_demo_manager() -> (
    VulcanToolsManager,
    Vec<vulcan_tools_manager::ResolvedToolDefinition>,
) {
    let manager = VulcanToolsManager::new(vec![
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                String::from("demo_tool"),
                String::from("demo_tool"),
                String::from("Demo tool"),
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::Native,
                    "test",
                    String::from("demo_tool"),
                ),
            ),
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
            invoke: Arc::new(|args, context| {
                Box::pin(async move {
                    Ok(VulcanToolInvocationResult::from_model_output(json!({
                        "args": args,
                        "session_id": context.session_id,
                        "turn_id": context.turn_id.value(),
                        "api_call_id": context.api_call_id.value(),
                        "tool_call_id": context.tool_call_id.value(),
                        "provider_tool_call_id": context.provider_tool_call_id,
                    }))
                    .with_host_result(VulcanToolHostResult {
                        kind: String::from("change_set"),
                        payload: json!({ "path": "src/main.rs", "op": "read" }),
                    }))
                })
            }),
        },
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor::new(
                String::from("failing_tool"),
                String::from("failing_tool"),
                String::from("Failing demo tool"),
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::Native,
                    "test",
                    String::from("failing_tool"),
                ),
            ),
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
            descriptor: VulcanToolDescriptor::new(
                String::from("structured_failure_tool"),
                String::from("structured_failure_tool"),
                String::from("Structured failure demo tool"),
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::Native,
                    "test",
                    String::from("structured_failure_tool"),
                ),
            ),
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
            descriptor: VulcanToolDescriptor::new(
                String::from("invalid_structured_failure_tool"),
                String::from("invalid_structured_failure_tool"),
                String::from("Invalid structured failure demo tool"),
                None,
                VulcanToolOrigin::new(
                    VulcanToolSourceKind::Native,
                    "test",
                    String::from("invalid_structured_failure_tool"),
                ),
            ),
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
        descriptor: VulcanToolDescriptor::new(
            String::from("panic_tool"),
            String::from("panic_tool"),
            String::from("Panic demo tool"),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::Native,
                "test",
                String::from("panic_tool"),
            ),
        ),
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
        descriptor: VulcanToolDescriptor::new(
            String::from("gated_tool"),
            String::from("gated_tool"),
            String::from("Gated demo tool"),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::Native,
                "test",
                String::from("gated_tool"),
            ),
        ),
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

/// Drives one tool future while acting as the projection host: each live event is collected and
/// its barrier acknowledged, as the kernel stream consumer does in production.
/// 以投影宿主身份驱动一个工具 future：收集每条实时事件并确认其屏障，与生产中内核流消费者一致。
async fn run_with_projection_host<F: std::future::Future>(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    future: F,
) -> (F::Output, Vec<ProviderStreamDelta>) {
    tokio::pin!(future);
    let mut events = Vec::new();
    loop {
        tokio::select! {
            output = &mut future => {
                events.extend(drain_live_events(receiver));
                return (output, events);
            }
            Some(item) = receiver.recv() => {
                let delta = item.expect("live event should not fail");
                if let ProviderStreamDelta::RuntimeEvent(event) = &delta {
                    acknowledge_runtime_event_projection_barrier_from_event(event)
                        .expect("projection barrier registry should remain available");
                }
                events.push(delta);
            }
        }
    }
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

    assert!(insert_error.contains("register a test barrier"));
    assert!(remove_error.contains("remove a test barrier"));
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
                config.system_prompt.clone(),
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
        request_stream_id: String::from("request-stream-call-1"),
        tool_call_id: String::from("call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    let identity = install_test_invocation_identity(&runtime_bridge, &request, 101, 102, 103);

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("core tool request should execute");

    assert_eq!(result.tool_call_id, "call-1");
    assert_eq!(result.tool_name, "demo_tool");
    let model_output: serde_json::Value =
        serde_json::from_str(&result.model_output).expect("model output should be JSON");
    assert_eq!(model_output["args"]["path"], "src/main.rs");
    assert_eq!(model_output["session_id"], Uuid::nil().to_string());
    assert_eq!(model_output["api_call_id"], identity.api_call_id.value());
    assert_eq!(model_output["tool_call_id"], identity.tool_call_id.value());
    assert_eq!(model_output["provider_tool_call_id"], "call-1");
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "change_set",
            "payload": { "path": "src/main.rs", "op": "read" }
        }))
    );
}

/// Verifies an image result is saved as a generated attachment and the model sees only its locator.
/// 验证图片结果保存为生成附件，模型只看到其定位符。
#[test]
fn managed_tool_runtime_bridge_saves_result_images_as_generated_attachments() {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 3))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("test PNG should encode");
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
    let workspace = tempfile::tempdir().expect("test workspace should be created");
    let manager = VulcanToolsManager::new(vec![VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            String::from("screenshot"),
            String::from("screenshot"),
            String::from("Screenshot tool"),
            None,
            VulcanToolOrigin::new(
                VulcanToolSourceKind::Native,
                "test",
                String::from("screenshot"),
            ),
        ),
        parameters_schema: json!({ "type": "object" }),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Visible,
        request_streaming: None,
        invoke: Arc::new(move |_args, _context| {
            let data = data.clone();
            Box::pin(async move {
                Ok(VulcanToolInvocationResult::from_model_output(json!({
                    "content": [
                        {"type": "text", "text": "captured"},
                        {"type": "image", "data": data, "mimeType": "image/png"}
                    ]
                })))
            })
        }),
    }])
    .with_workspace_root(Some(workspace.path().to_path_buf()));
    let definitions = manager.resolve_for_execution_mode("Build").definitions;
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-shot"),
        tool_call_id: String::from("call-shot"),
        tool_name: String::from("screenshot"),
        arguments: json!({}),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    install_test_invocation_identity(&runtime_bridge, &request, 111, 112, 113);

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("image tool request should execute");

    assert!(!result.is_error, "{}", result.model_output);
    let attachments =
        crate::agent_runtime::provider_v1_bridge::provider_v1_generated_attachments_from_tool_result(
            &result,
        )
        .expect("saved images form a valid generated-attachment host result");
    assert_eq!(attachments.len(), 1);
    assert!(std::path::Path::new(&attachments[0].local_path).is_file());
    let locator = format!(
        "attachment://{}",
        attachments[0]
            .asset_id
            .as_deref()
            .expect("saved image has an asset id")
    );
    assert!(result.model_output.starts_with("captured"));
    assert!(result.model_output.contains(locator.as_str()));
    assert!(!result.model_output.contains("\"image\""));
}

/// Verifies parallel tool bodies receive their own exact Provider identity without cross-talk.
/// 验证并行工具主体各自收到精确 Provider 身份且不会串线。
#[test]
fn parallel_managed_tools_receive_exact_invocation_identity() {
    let (manager, definitions) = build_demo_manager();
    let session_id = Uuid::from_u128(0xA11CE);
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        session_id,
        "Build",
    ));
    let first = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-parallel-call-a"),
        tool_call_id: String::from("parallel-call-a"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "a.txt" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let second = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-parallel-call-b"),
        tool_call_id: String::from("parallel-call-b"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "b.txt" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    let first_identity = PreparedToolReceipt {
        turn_id: vulcan_session_core::v1::ids::TurnId::try_new(175).expect("synthetic turn"),
        api_call_id: ApiCallId::try_new(201).expect("first API id should be valid"),
        tool_call_id: ToolCallId::try_new(202).expect("first tool id should be valid"),
        request_stream_id: RequestStreamId::try_new(first.request_stream_id.clone())
            .expect("first request stream id should be valid"),
        tool_seq: Seq::try_new(203).expect("first tool seq should be valid"),
    };
    let second_identity = PreparedToolReceipt {
        turn_id: vulcan_session_core::v1::ids::TurnId::try_new(175).expect("synthetic turn"),
        api_call_id: ApiCallId::try_new(201).expect("second API id should be valid"),
        tool_call_id: ToolCallId::try_new(204).expect("second tool id should be valid"),
        request_stream_id: RequestStreamId::try_new(second.request_stream_id.clone())
            .expect("second request stream id should be valid"),
        tool_seq: Seq::try_new(205).expect("second tool seq should be valid"),
    };
    runtime_bridge
        .install_prepared_invocation_identities(
            &[first.clone(), second.clone()],
            vec![first_identity.clone(), second_identity.clone()],
        )
        .expect("parallel durable identities should install");

    let (first_result, second_result) = runtime.block_on(async {
        tokio::join!(
            runtime_bridge.execute_tool(&first),
            runtime_bridge.execute_tool(&second)
        )
    });
    let first_result = first_result.expect("first parallel tool should execute");
    let second_result = second_result.expect("second parallel tool should execute");
    for (result, expected_call_id, expected_path, expected_identity) in [
        (first_result, "parallel-call-a", "a.txt", first_identity),
        (second_result, "parallel-call-b", "b.txt", second_identity),
    ] {
        let output: serde_json::Value =
            serde_json::from_str(&result.model_output).expect("parallel output should be JSON");
        assert_eq!(output["session_id"], session_id.to_string());
        assert_eq!(output["turn_id"], expected_identity.turn_id.value());
        assert_eq!(output["api_call_id"], expected_identity.api_call_id.value());
        assert_eq!(
            output["tool_call_id"],
            expected_identity.tool_call_id.value()
        );
        assert_eq!(output["provider_tool_call_id"], expected_call_id);
        assert_eq!(output["args"]["path"], expected_path);
    }
}

/// Verifies the production ToolBatchPrepared barrier installs the exact durable identities before parallel bodies start.
/// 验证生产 ToolBatchPrepared 屏障会在并行工具主体启动前安装精确的持久化身份。
#[test]
fn prepared_tool_batch_barrier_installs_exact_parallel_invocation_identities() {
    let (manager, definitions) = build_demo_manager();
    let session_id = Uuid::from_u128(0xB47C4);
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        session_id,
        "Build",
    ));
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    let requests = vec![
        ToolInvocationRequest {
            request_stream_id: String::from("request-stream-prepared-a"),
            tool_call_id: String::from("provider-call-prepared-a"),
            tool_name: String::from("demo_tool"),
            arguments: json!({ "path": "prepared-a.txt" }),
            argument_parse_error: None,
            raw_arguments: Default::default(),
            user_request: Default::default(),
        },
        ToolInvocationRequest {
            request_stream_id: String::from("request-stream-prepared-b"),
            tool_call_id: String::from("provider-call-prepared-b"),
            tool_name: String::from("demo_tool"),
            arguments: json!({ "path": "prepared-b.txt" }),
            argument_parse_error: None,
            raw_arguments: Default::default(),
            user_request: Default::default(),
        },
    ];
    let identities = vec![
        PreparedToolReceipt {
            turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
            api_call_id: ApiCallId::try_new(211).expect("prepared API id should be valid"),
            tool_call_id: ToolCallId::try_new(212).expect("first prepared tool id should be valid"),
            request_stream_id: RequestStreamId::try_new(requests[0].request_stream_id.clone())
                .expect("first prepared request stream should be valid"),
            tool_seq: Seq::try_new(213).expect("first prepared tool seq should be valid"),
        },
        PreparedToolReceipt {
            turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
            api_call_id: ApiCallId::try_new(211).expect("prepared API id should be valid"),
            tool_call_id: ToolCallId::try_new(214)
                .expect("second prepared tool id should be valid"),
            request_stream_id: RequestStreamId::try_new(requests[1].request_stream_id.clone())
                .expect("second prepared request stream should be valid"),
            tool_seq: Seq::try_new(215).expect("second prepared tool seq should be valid"),
        },
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        let prepare_executor = executor.clone();
        let prepare_requests = requests.clone();
        let prepare_task =
            tokio::spawn(
                async move { prepare_executor.prepare_tool_batch(&prepare_requests).await },
            );
        let prepared_event = receiver
            .recv()
            .await
            .expect("prepared batch event should be sent")
            .expect("prepared batch event should be valid");
        let ProviderStreamDelta::RuntimeEvent(prepared_event) = prepared_event else {
            panic!("prepared batch delta should be a runtime event");
        };
        assert_eq!(prepared_event.kind, RuntimeEventKind::ToolBatchPrepared);
        assert!(
            acknowledge_tool_batch_projection_barrier_from_event(
                &prepared_event,
                identities.clone(),
            )
            .expect("prepared batch barrier registry should remain available")
        );
        prepare_task
            .await
            .expect("prepared batch task should join")
            .expect("prepared batch should accept its durable identities");

        let ((first_result, second_result), _events) =
            run_with_projection_host(&mut receiver, async {
                tokio::join!(
                    executor.execute_tool(&requests[0]),
                    executor.execute_tool(&requests[1]),
                )
            })
            .await;
        for (result, identity, provider_call_id) in [
            (
                first_result.expect("first prepared tool should execute"),
                &identities[0],
                "provider-call-prepared-a",
            ),
            (
                second_result.expect("second prepared tool should execute"),
                &identities[1],
                "provider-call-prepared-b",
            ),
        ] {
            let output: serde_json::Value = serde_json::from_str(&result.model_output)
                .expect("prepared tool output should be JSON");
            assert_eq!(output["session_id"], session_id.to_string());
            assert_eq!(output["api_call_id"], identity.api_call_id.value());
            assert_eq!(output["tool_call_id"], identity.tool_call_id.value());
            assert_eq!(output["provider_tool_call_id"], provider_call_id);
        }
    });
}

/// Verifies an unregistered provider tool receives a stable recovery binding and remains a normal model-visible tool error.
/// 验证未注册的 Provider 工具会获得稳定恢复绑定，并继续作为普通的模型可见工具错误返回。
#[test]
fn managed_tool_runtime_bridge_preserves_unregistered_tool_error_path() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-call-unknown-1"),
        tool_call_id: String::from("call-unknown-1"),
        tool_name: String::from("invented_tool"),
        arguments: json!({ "query": "README" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let same_binding_request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-call-unknown-2"),
        tool_call_id: String::from("call-unknown-2"),
        tool_name: request.tool_name.clone(),
        arguments: json!({ "query": "Cargo.toml" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let other_binding_request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-call-unknown-3"),
        tool_call_id: String::from("call-unknown-3"),
        tool_name: String::from("other_invented_tool"),
        arguments: json!({}),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };

    let fingerprint = runtime_bridge
        .binding_fingerprint_for_request(&request)
        .expect("unregistered binding should have a recovery fingerprint");
    assert_eq!(
        fingerprint,
        runtime_bridge
            .binding_fingerprint_for_request(&same_binding_request)
            .expect("the same unregistered binding should remain stable")
    );
    assert_ne!(
        fingerprint,
        runtime_bridge
            .binding_fingerprint_for_request(&other_binding_request)
            .expect("a different unregistered binding should remain distinguishable")
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    install_test_invocation_identity(&runtime_bridge, &request, 301, 302, 303);
    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("unregistered tool should return one controller-visible result");

    assert!(result.is_error);
    assert!(result.model_output.contains("invented_tool"));
}

/// Verifies host-managed tool invocation failures are downgraded into one error result returned to the controller instead of aborting the whole turn.
/// 验证宿主管理工具调用失败会被降级为回传给控制器的一条错误结果，而不是直接中断整轮执行。
#[test]
fn managed_tool_runtime_bridge_converts_tool_failures_into_error_results() {
    let (manager, definitions) = build_demo_manager();
    let runtime_bridge =
        VulcanManagedToolRuntimeBridge::new(Arc::new(manager), &definitions, Uuid::nil(), "Build");
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-call-error-1"),
        tool_call_id: String::from("call-error-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    install_test_invocation_identity(&runtime_bridge, &request, 401, 402, 403);

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
                "error": "LuaSkills 工具解析失败：演示错误"
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
        request_stream_id: String::from("request-stream-call-structured-error-1"),
        tool_call_id: String::from("call-structured-error-1"),
        tool_name: String::from("structured_failure_tool"),
        arguments: json!({}),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    install_test_invocation_identity(&runtime_bridge, &request, 501, 502, 503);

    let result = runtime
        .block_on(runtime_bridge.execute_tool(&request))
        .expect("structured tool failure should remain controller-visible");

    assert!(result.is_error);
    assert!(result.model_output.starts_with("# Tool failed:"));
    assert!(
        result
            .model_output
            .contains("structured persistence failure")
    );
    assert!(result.model_output.contains("persist"));
    assert_eq!(
        result.host_result,
        Some(json!({
            "kind": "tool_error",
            "payload": {
                "tool_call_id": "call-structured-error-1",
                "tool_name": "structured_failure_tool",
                "status": "failed",
                "error": "structured persistence failure",
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
        request_stream_id: String::from("request-stream-call-invalid-structured-error-1"),
        tool_call_id: String::from("call-invalid-structured-error-1"),
        tool_name: String::from("invalid_structured_failure_tool"),
        arguments: json!({}),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    install_test_invocation_identity(&runtime_bridge, &request, 601, 602, 603);

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
        request_stream_id: String::from("request-stream-call-plan-blocked-1"),
        tool_call_id: String::from("call-plan-blocked-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");
    install_test_invocation_identity(&runtime_bridge, &request, 701, 702, 703);

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
                argument_parse_error: None,
                raw_arguments: Default::default(),
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
                String::from("system"),
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
    let prepared_request = ToolInvocationRequest {
        request_stream_id: String::from("tool-item-1"),
        tool_call_id: String::from("tool-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(&runtime_bridge, &prepared_request, 801, 802, 803);

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
    // The tool record is identified by its structured metadata, never by its display title.
    // 工具记录通过结构化元数据识别，绝不依赖其显示标题。
    assert!(controller.snapshot().records.iter().any(|record| {
        let metadata_text = |key: &str| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(key))
                .and_then(serde_json::Value::as_str)
        };
        record.role == TranscriptRole::Tool
            && metadata_text("tool_name") == Some("demo_tool")
            && metadata_text("kind") == Some("change_set")
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
                argument_parse_error: None,
                raw_arguments: Default::default(),
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
                String::from("system"),
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
    let prepared_request = ToolInvocationRequest {
        request_stream_id: String::from("tool-item-1"),
        tool_call_id: String::from("tool-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "README" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(&runtime_bridge, &prepared_request, 901, 902, 903);

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
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-panic-call-1"),
        tool_call_id: String::from("panic-call-1"),
        tool_name: String::from("panic_tool"),
        arguments: json!({ "query": "山姆零食" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1001, 1002, 1003);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let (result, events) = runtime.block_on(run_with_projection_host(
        &mut receiver,
        executor.execute_tool(&request),
    ));
    let result = result.expect("panicking tool should still return one controller-visible result");

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
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-failing-call-1"),
        tool_call_id: String::from("failing-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "演示错误" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1101, 1102, 1103);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let (result, events) = runtime.block_on(run_with_projection_host(
        &mut receiver,
        executor.execute_tool(&request),
    ));
    let result = result
        .expect("managed tool failure should still return a normal controller-visible result");

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
        Arc::clone(&runtime_bridge),
        sender,
        Some(String::from(
            "Tool calls are not allowed during context compaction.",
        )),
        true,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-rejected-call-1"),
        tool_call_id: String::from("rejected-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "must not run" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1201, 1202, 1203);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let (result, events) = runtime.block_on(run_with_projection_host(
        &mut receiver,
        executor.execute_tool(&request),
    ));
    let result = result.expect("policy rejection should be returned as a normal tool result");

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
        Arc::clone(&runtime_bridge),
        sender,
        Some(String::from("Scoped rejection is not active.")),
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-empty-scope-call-1"),
        tool_call_id: String::from("empty-scope-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "scope empty should still run" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1301, 1302, 1303);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let (result, events) = runtime.block_on(run_with_projection_host(
        &mut receiver,
        executor.execute_tool(&request),
    ));
    let result = result.expect("empty rejection scope should allow the host tool call to run");

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
        Arc::clone(&runtime_bridge),
        sender,
        Some(String::from(
            "This tool is reserved for the parent session.",
        )),
        false,
        vec![String::from("failing_tool")],
    );
    let rejected_request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-scoped-rejected-call-1"),
        tool_call_id: String::from("scoped-rejected-call-1"),
        tool_name: String::from("failing_tool"),
        arguments: json!({ "query": "must be rejected before the failing body runs" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    let allowed_request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-scoped-allowed-call-1"),
        tool_call_id: String::from("scoped-allowed-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    runtime_bridge
        .install_prepared_invocation_identities(
            &[rejected_request.clone(), allowed_request.clone()],
            vec![
                PreparedToolReceipt {
                    turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1)
                        .expect("synthetic turn"),
                    api_call_id: ApiCallId::try_new(1401).expect("batch API id should be valid"),
                    tool_call_id: ToolCallId::try_new(1402)
                        .expect("rejected tool id should be valid"),
                    request_stream_id: RequestStreamId::try_new(
                        rejected_request.request_stream_id.clone(),
                    )
                    .expect("rejected request stream id should be valid"),
                    tool_seq: Seq::try_new(1403).expect("rejected tool seq should be valid"),
                },
                PreparedToolReceipt {
                    turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1)
                        .expect("synthetic turn"),
                    api_call_id: ApiCallId::try_new(1401).expect("batch API id should be valid"),
                    tool_call_id: ToolCallId::try_new(1404)
                        .expect("allowed tool id should be valid"),
                    request_stream_id: RequestStreamId::try_new(
                        allowed_request.request_stream_id.clone(),
                    )
                    .expect("allowed request stream id should be valid"),
                    tool_seq: Seq::try_new(1405).expect("allowed tool seq should be valid"),
                },
            ],
        )
        .expect("scoped batch durable identities should install");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    let (rejected_result, mut events) = runtime.block_on(run_with_projection_host(
        &mut receiver,
        executor.execute_tool(&rejected_request),
    ));
    let rejected_result =
        rejected_result.expect("scoped policy rejection should return a normal tool result");
    let (allowed_result, allowed_events) = runtime.block_on(run_with_projection_host(
        &mut receiver,
        executor.execute_tool(&allowed_request),
    ));
    let allowed_result = allowed_result.expect("unlisted tool should execute normally");
    events.extend(allowed_events);

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
    // allowedFinishedMetadata is the exact transient completion boundary consumed by session projection.
    // allowedFinishedMetadata 是会话投影消费的精确瞬时完成边界。
    let allowed_finished_metadata = events
        .iter()
        .find_map(|event| match event {
            ProviderStreamDelta::RuntimeEvent(runtime_event)
                if runtime_event.kind == RuntimeEventKind::ToolFinished
                    && runtime_event
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("tool_call_id"))
                        .and_then(serde_json::Value::as_str)
                        == Some(allowed_request.tool_call_id.as_str()) =>
            {
                runtime_event.metadata.as_ref()
            }
            _ => None,
        })
        .expect("allowed tool completion metadata should be emitted");
    assert_eq!(
        allowed_finished_metadata.get("host_result"),
        allowed_result.host_result.as_ref()
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
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-gated-call-1"),
        tool_call_id: String::from("gated-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "先显示工具调用" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1501, 1502, 1503);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // barrierOverride enables real acknowledgements without mutating the process-wide production switch.
        // barrierOverride 启用真实确认且不修改进程级生产开关。
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
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-gated-rejected-call-1"),
        tool_call_id: String::from("gated-rejected-call-1"),
        tool_name: String::from("gated_tool"),
        arguments: json!({ "query": "拒绝不安全工具边界" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1601, 1602, 1603);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // barrierOverride enables the production-shaped mandatory gate for this current-thread test.
        // barrierOverride 为当前线程测试启用生产形态的强制门禁。
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
            error
                .to_string()
                .contains("durability barrier was rejected"),
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
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-barrier-call-1"),
        tool_call_id: String::from("barrier-call-1"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1701, 1702, 1703);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // barrierOverride isolates real projection acknowledgements to this current-thread runtime.
        // barrierOverride 将真实投影确认隔离到当前线程运行时。
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

/// Verifies a rejected ToolFinished barrier preserves the original host error as a fatal Core error.
/// 验证被拒绝的 ToolFinished 屏障会把宿主原始错误保留为致命 Core 错误。
#[test]
fn streaming_tool_executor_propagates_tool_finished_rejection_reason() {
    // manager and definitions provide the real managed-tool catalog used by the bridge.
    // manager 与 definitions 提供桥接层使用的真实托管工具目录。
    let (manager, definitions) = build_demo_manager();
    // runtime_bridge owns the invocation identity and executes the demo tool body.
    // runtime_bridge 持有调用身份并执行演示工具主体。
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(manager),
        &definitions,
        Uuid::nil(),
        "Build",
    ));
    // sender and receiver expose the same live runtime-event channel used by production streams.
    // sender 与 receiver 暴露生产流使用的同一实时运行时事件通道。
    let (sender, mut receiver): (
        ProviderLiveDeltaSender,
        tokio::sync::mpsc::UnboundedReceiver<Result<ProviderStreamDelta, String>>,
    ) = tokio::sync::mpsc::unbounded_channel();
    // executor emits mandatory start and finish projection barriers around the tool body.
    // executor 在工具主体前后发出强制的开始与完成投影屏障。
    let executor = StreamingRuntimeToolExecutor::new(
        Arc::clone(&runtime_bridge),
        sender,
        None,
        false,
        Vec::new(),
    );
    // request is the exact managed invocation whose terminal barrier will be rejected.
    // request 是其终态屏障将被拒绝的精确托管调用。
    let request = ToolInvocationRequest {
        request_stream_id: String::from("request-stream-finish-rejected"),
        tool_call_id: String::from("finish-rejected-call"),
        tool_name: String::from("demo_tool"),
        arguments: json!({ "path": "src/main.rs" }),
        argument_parse_error: None,
        raw_arguments: Default::default(),
        user_request: Default::default(),
    };
    install_test_invocation_identity(runtime_bridge.as_ref(), &request, 1801, 1802, 1803);
    // runtime keeps event receipt and task completion ordering deterministic.
    // runtime 让事件接收与任务完成顺序保持确定性。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        // task runs the tool concurrently so the test can acknowledge each emitted boundary.
        // task 并发运行工具，使测试能够依次确认每个已发出的边界。
        let task = tokio::spawn(async move { executor.execute_tool(&request).await });
        // started_event is the first mandatory recovery boundary emitted before execution.
        // started_event 是执行前发出的第一条强制恢复边界。
        let started_event = receiver
            .recv()
            .await
            .expect("tool-start event should be sent")
            .expect("tool-start event should be ok");
        // started_event narrows the received delta to the authoritative runtime event.
        // started_event 将收到的增量收窄为权威运行时事件。
        let ProviderStreamDelta::RuntimeEvent(started_event) = started_event else {
            panic!("first delta should be a tool-start runtime event");
        };
        assert!(
            acknowledge_runtime_event_projection_barrier_from_event(&started_event)
                .expect("tool-start barrier acknowledgement")
        );
        // finished_event is the terminal durability boundary emitted after the tool body succeeds.
        // finished_event 是工具主体成功后发出的终态持久化边界。
        let finished_event = receiver
            .recv()
            .await
            .expect("tool-finished event should be sent")
            .expect("tool-finished event should be ok");
        // finished_event narrows the second delta to the exact terminal runtime event.
        // finished_event 将第二条增量收窄为精确终态运行时事件。
        let ProviderStreamDelta::RuntimeEvent(finished_event) = finished_event else {
            panic!("second delta should be a tool-finished runtime event");
        };
        // original_error is the canonical lifecycle rejection that must survive every boundary.
        // original_error 是必须跨越全部边界保留的规范生命周期拒绝。
        let original_error =
            "Invalid change-set field at payload.files[0].hunks: lifecycle changes";
        assert!(
            reject_runtime_event_projection_barrier_from_event_with_reason(
                &finished_event,
                original_error,
            )
            .expect("tool-finished barrier rejection")
        );

        // error is the fatal Core result observed by the controller-facing caller.
        // error 是面向控制器的调用方观察到的致命 Core 结果。
        let error = task
            .await
            .expect("tool task should join")
            .expect_err("rejected terminal barrier must fail the tool call");
        assert!(matches!(
            error,
            VulcanCoreError::ToolDurabilityFailure { ref tool_name, ref message, .. }
                if tool_name == "demo_tool" && message.contains(original_error)
        ));
    });
}
